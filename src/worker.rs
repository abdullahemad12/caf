use futures::FutureExt;

use crate::errors::{CafError, WrapErrorInResult};
use crate::network::{Event, Network, NetworkClient, Peer};
use crate::pkgman::{Package, PackageId, PackageManager};

pub async fn bootstrap(
    preconfigured_bootstrap_peers: Vec<Peer>,
    ntwrk_client: &mut NetworkClient,
) -> Result<(), CafError> {
    if preconfigured_bootstrap_peers.is_empty() {
        return Err(CafError::new(
            "TODO: dynamic resolving of bootstrap peers. For now needs to be provided manually",
        ));
    }

    for peer in preconfigured_bootstrap_peers {
        ntwrk_client
            .dial(peer.clone())
            .await
            .wrap_err("expect dial to succeed")?
    }

    ntwrk_client.bootstrap().await?;

    return Ok(());
}

// TODO: this should loop over all packages and register them in kademlia
// if the number of packages exceeds some limit, maybe pick a random set every
// 10 minutes or so to provide
async fn register_provided_packages(ntwrk_client: &mut NetworkClient) {
    ntwrk_client.start_providing("".to_string().clone()).await;
}

pub async fn start_pkg_provider(
    pkgman: &PackageManager,
    ntwrk: &mut Network,
    ntwrk_client: &mut NetworkClient,
) {
    register_provided_packages(ntwrk_client).await;
    match ntwrk.next_event().await {
        Event::InboundRequest { request, channel } => {
            // TODO: do I need to respond with an error here if this node does not have the package
            if let Ok(package) = pkgman.retrieve_package(&request) {
                ntwrk_client.respond_package(package.content, channel).await;
            }
        }
    }
}

// TODO: this should listen on a unix socket for cli commands
pub async fn start_pkg_consumer(
    pkgman: &PackageManager,
    ntwrk_client: &mut NetworkClient,
) -> Result<(), CafError> {
    let name = "".to_string();
    let version: String = "7.0".into(); // TODO add option to get it from cli args
                                        // TODO If the version is not provided by the user default to getting the latest
                                        // version.. Make a request with an optional parameter (version) that would get the
                                        // hash of the package based on the version if it is provided and defaults to the
                                        // latest if not

    let providers = ntwrk_client.get_providers(name.clone()).await;

    // TODO: This shouldn't return with an error. Instead it should respond to the cli with an
    // error
    if providers.is_empty() {
        return Err(CafError::new(format!(
            "could not find providers for file {}",
            name
        )));
    };

    let package_id = PackageId { name, version };
    let requests: Vec<_> = providers
        .into_iter()
        .map(|it| {
            let mut network_client = ntwrk_client.clone();
            let package_id = package_id.clone();
            async move { network_client.request_package(it, package_id).await }.boxed()
        })
        .collect();

    // we only need one of them to respond
    let package_content = futures::future::select_ok(requests)
        .await
        .wrap_err("downloading the package from the providers failed")?
        .0;

    // TODO: verify the package using a hash
    pkgman.install_package(Package {
        id: package_id,
        content: package_content,
    })?;

    return Ok(());
}
