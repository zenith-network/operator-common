use k8s_openapi::api::core::v1::{Service, ServicePort, ServiceSpec};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use kube::api::{DeleteParams, ObjectMeta, PostParams};
use kube::{Api, Client, Error};
use std::collections::BTreeMap;
use tracing::{Level, event, instrument};

#[derive(Debug, Clone)]
pub struct Port<'a> {
    pub name: String,
    pub port: i32,
    pub protocol: &'a str,
}

#[instrument(skip(client))]
pub async fn deploy<'a>(
    client: Client,
    name: String,
    namespace: String,
    service_type: &str,
    service_port: Vec<Port<'a>>,
    labels: (BTreeMap<String, String>, BTreeMap<String, String>),
) -> Result<Service, Error> {
    let mut service_ports: Vec<ServicePort> = Vec::new();

    for port in service_port {
        service_ports.push(ServicePort {
            name: Some(port.name),
            port: port.port,
            protocol: Some(port.protocol.to_string()),
            target_port: Some(IntOrString::Int(port.port)),
            ..ServicePort::default()
        });
    }

    let object: Service = Service {
        metadata: ObjectMeta {
            name: Some(name.to_owned()),
            namespace: Some(namespace.to_owned()),
            labels: Some(labels.0.clone()),
            ..ObjectMeta::default()
        },
        spec: Some(ServiceSpec {
            type_: Some(service_type.to_string()),
            ports: Some(service_ports),
            selector: Some(labels.1),
            ..ServiceSpec::default()
        }),
        ..Service::default()
    };

    event!(Level::INFO, name, namespace, "Creating Service");

    let service_api: Api<Service> = Api::namespaced(client, namespace.as_str());
    service_api.create(&PostParams::default(), &object).await
}

#[instrument(skip(client))]
pub async fn delete(client: Client, name: String, namespace: String) -> Result<(), Error> {
    event!(Level::INFO, name, namespace, "Deleting Service");

    let api: Api<Service> = Api::namespaced(client, namespace.as_str());
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
