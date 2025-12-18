use std::{collections::BTreeMap, time::Duration};

use k8s_openapi::api::core::v1::Service;
use kube::{Api, Client, Error, ResourceExt, api::ListParams, core::ErrorResponse};
use kube_runtime::wait::{Condition, await_condition};
use tokio::task::JoinSet;
use tracing::{error, instrument};

use crate::{ActionType, labels, selector_labels, types::service};

#[instrument(skip(client))]
pub async fn create(
    client: Client,
    name: String,
    namespace: String,
    kind: String,
    replicas: i32,
    port: i32,
    action: ActionType,
) -> Result<(), crate::Error> {
    match action {
        ActionType::Create => {
            _create(client, name, namespace, kind, port, 0, replicas as usize).await?;
        }
        ActionType::Update => {
            let service_api: Api<Service> = Api::namespaced(client.clone(), namespace.as_str());
            let lp = ListParams::default()
                .match_any()
                .timeout(300)
                .labels(format!("app.kubernetes.io/instance={name}").as_str())
                .fields("spec.type=LoadBalancer");
            let existing_load_balancers = service_api.list(&lp).await?;
            let lb_count = existing_load_balancers.items.len();

            if lb_count > replicas as usize {
                // Handle excess load balancers
                let mut set = JoinSet::new();
                for idx in (replicas as usize)..lb_count {
                    let cli = client.clone();
                    let n = name.to_owned();
                    let ns = namespace.to_owned();

                    service::delete(cli, format!("{n}-p2p-{idx}"), ns.clone()).await?;
                }

                while let Some(res) = set.join_next().await {
                    res?;
                }
            } else if lb_count < replicas as usize {
                // Handle insufficient load balancers
                _create(
                    client,
                    name,
                    namespace,
                    kind,
                    port,
                    lb_count,
                    replicas as usize,
                )
                .await?;
            }
        }
    }

    Ok(())
}

#[instrument(skip(client))]
pub async fn get_external_ips(
    client: Client,
    name: String,
    namespace: String,
    replicas: i32,
) -> Result<BTreeMap<String, String>, crate::Error> {
    let mut external_addrs: BTreeMap<String, String> = BTreeMap::new();

    let mut set = JoinSet::new();
    for idx in 0..replicas {
        let cli = client.clone();
        let n = name.to_owned();
        let ns = namespace.to_owned();

        set.spawn(async move {
            wait(cli, format!("{n}-p2p-{idx}"), ns)
                .await
                .map(|ip_address| (format!("{n}-{idx}"), ip_address))
        });
    }

    while let Some(res) = set.join_next().await {
        let (pod_name, ip_address) = res??;
        external_addrs.insert(pod_name, ip_address);
    }

    Ok(external_addrs)
}

#[instrument(skip(client))]
pub async fn delete(client: Client, name: String, namespace: String) -> Result<(), Error> {
    let service_api: Api<Service> = Api::namespaced(client.clone(), namespace.as_str());
    let lp = ListParams::default()
        .match_any()
        .timeout(300)
        .labels(format!("app.kubernetes.io/instance={name}").as_str())
        .fields("spec.type=LoadBalancer");
    let existing_load_balancers = service_api.list(&lp).await?;

    let mut set = JoinSet::new();
    for lb in existing_load_balancers {
        let cli = client.clone();
        let ns = namespace.to_owned();

        set.spawn(service::delete(cli, lb.name_any(), ns.clone()));
    }

    while let Some(res) = set.join_next().await {
        match res {
            Ok(_) => (),
            Err(err) => {
                return Err(Error::Api(ErrorResponse {
                    status: "Failed".to_string(),
                    message: err.to_string(),
                    reason: "Failed to join set while deleting load balancers".to_string(),
                    code: 418,
                }));
            }
        }
    }

    Ok(())
}

#[instrument(skip(client))]
pub async fn wait(
    client: Client,
    name: String,
    namespace: String,
) -> std::result::Result<String, crate::Error> {
    let service_api: Api<Service> = Api::namespaced(client, namespace.as_str());

    let exists = await_condition(service_api, name.as_str(), external_ip_exists());
    let out = tokio::time::timeout(Duration::from_secs(300), exists).await?;
    match out {
        Ok(res) => match res.unwrap().status.unwrap().load_balancer.unwrap().ingress {
            Some(ingress) => {
                if !ingress.is_empty() {
                    return Ok(ingress[0].clone().ip.unwrap());
                } else {
                    Err(crate::Error::IngressListEmpty)
                }
            }
            None => Err(crate::Error::IngressListMissing),
        },
        Err(e) => Err(crate::Error::WaitError { source: e }),
    }
}

#[instrument]
fn external_ip_exists() -> impl Condition<Service> {
    move |obj: Option<&Service>| {
        if let Some(svc) = &obj {
            if let Some(status) = &svc.status {
                if let Some(lb) = &status.load_balancer {
                    if let Some(ingress) = &lb.ingress {
                        if let Some(_ip) = &ingress[0].ip {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }
}

async fn _create(
    client: Client,
    name: String,
    namespace: String,
    kind: String,
    port: i32,
    lower: usize,
    upper: usize,
) -> Result<(), crate::Error> {
    let mut set = JoinSet::new();

    for idx in lower..upper {
        let pod_name = format!("{name}-p2p-{idx}");
        let mut sl = selector_labels(name.clone(), kind.clone());
        sl.insert(
            "statefulset.kubernetes.io/pod-name".to_owned(),
            pod_name.clone(),
        );

        let cli = client.clone();
        let n = name.to_owned();
        let ns = namespace.to_owned();

        set.spawn(service::deploy(
            cli,
            format!("{n}-p2p-{idx}"),
            ns,
            "LoadBalancer",
            vec![service::Port {
                name: "p2p".to_string(),
                port,
                protocol: "TCP",
            }],
            (labels(name.clone(), kind.clone()), sl),
        ));

        while let Some(res) = set.join_next().await {
            match res {
                Ok(_) => (),
                Err(e) => error!(error = e.to_string()),
            }
        }
    }

    Ok(())
}
