use kcr_gateway_networking_k8s_io::v1::gateways::{
    Gateway, GatewayListeners, GatewayListenersTls, GatewayListenersTlsCertificateRefs,
    GatewayListenersTlsMode, GatewaySpec,
};
use kube::api::{DeleteParams, ObjectMeta, Patch, PatchParams};
use kube::{Api, Client, Error};
use std::collections::BTreeMap;
use tracing::{Level, event, instrument};

#[instrument(skip(client))]
pub async fn deploy(
    client: Client,
    name: &str,
    namespace: &str,
    gateway_class_name: &str,
    certificate_secret_name: &str,
    labels: BTreeMap<String, String>,
) -> Result<Gateway, Error> {
    let object: Gateway = Gateway {
        metadata: ObjectMeta {
            name: Some(name.to_owned()),
            namespace: Some(namespace.to_owned()),
            labels: Some(labels.clone()),
            ..ObjectMeta::default()
        },
        spec: GatewaySpec {
            gateway_class_name: gateway_class_name.to_owned(),
            listeners: vec![GatewayListeners {
                name: "https".to_owned(),
                protocol: "HTTPS".to_owned(),
                port: 443,
                tls: Some(GatewayListenersTls {
                    mode: Some(GatewayListenersTlsMode::Terminate),
                    certificate_refs: Some(vec![GatewayListenersTlsCertificateRefs {
                        kind: Some("Secret".to_owned()),
                        name: certificate_secret_name.to_owned(),
                        ..Default::default()
                    }]),
                    ..Default::default()
                }),
                ..Default::default()
            }],
            ..Default::default()
        },
        ..Gateway::default()
    };

    event!(Level::INFO, name, namespace, "Creating Gateway");

    // Create the pvc defined above
    let service_api: Api<Gateway> = Api::namespaced(client, namespace);
    let params = PatchParams::apply(name);
    service_api
        .patch(name, &params, &Patch::Apply(&object))
        .await
}

#[instrument(skip(client))]
pub async fn delete(client: Client, name: String, namespace: String) -> Result<(), Error> {
    event!(Level::INFO, name, namespace, "Deleting Gateway");

    let api: Api<Gateway> = Api::namespaced(client, namespace.as_str());
    match api.delete(name.as_str(), &DeleteParams::default()).await {
        Ok(_) => Ok(()),
        Err(e) => {
            match e {
                // If the resource doesn't exist, we can ignore the error
                Error::Api(er) => {
                    if er.reason == "NotFound" {
                        return Ok(());
                    };
                    Err(Error::Api(er))
                }
                _ => Err(e),
            }
        }
    }
}
