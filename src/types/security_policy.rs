use kcr_gateway_networking_k8s_io::v1::httproutes::{
    HTTPRoute, HttpRouteParentRefs, HttpRouteRules, HttpRouteRulesBackendRefs,
    HttpRouteRulesMatches, HttpRouteRulesMatchesPath, HttpRouteRulesMatchesPathType, HttpRouteSpec,
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
    hostname: &str,
    gateway_class_name: &str,
    certificate_secret_name: &str,
    labels: BTreeMap<String, String>,
) -> Result<SecurityPolicy, Error> {
    let object: HTTPRoute = HTTPRoute {
        metadata: ObjectMeta {
            name: Some(name.to_owned()),
            namespace: Some(namespace.to_owned()),
            labels: Some(labels.clone()),
            ..Default::default()
        },
        spec: HttpRouteSpec {
            parent_refs: Some(vec![HttpRouteParentRefs {
                name: gateway_class_name.to_owned(),
                ..Default::default()
            }]),
            hostnames: Some(vec![hostname.to_owned()]),
            rules: Some(vec![HttpRouteRules {
                backend_refs: Some(vec![HttpRouteRulesBackendRefs {
                    kind: Some("Service".to_owned()),
                    name: format!("{name}-web"),
                    port: Some(8080),
                    weight: Some(1),
                    ..Default::default()
                }]),
                matches: Some(vec![HttpRouteRulesMatches {
                    path: Some(HttpRouteRulesMatchesPath {
                        r#type: Some(HttpRouteRulesMatchesPathType::PathPrefix),
                        value: Some("/".to_owned()),
                    }),
                    ..Default::default()
                }]),
                ..Default::default()
            }]),
        },
        ..Default::default()
    };

    event!(Level::INFO, name, namespace, "Creating HTTPRoute");

    // Create the pvc defined above
    let service_api: Api<HTTPRoute> = Api::namespaced(client, namespace);
    let params = PatchParams::apply(name);
    service_api
        .patch(name, &params, &Patch::Apply(&object))
        .await
}

#[instrument(skip(client))]
pub async fn delete(client: Client, name: String, namespace: String) -> Result<(), Error> {
    event!(Level::INFO, name, namespace, "Deleting HTTPRoute");

    let api: Api<HTTPRoute> = Api::namespaced(client, namespace.as_str());
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
