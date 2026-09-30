use futures::{future, FutureExt};

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
async fn register_provided_packages(
    pkgman: &PackageManager,
    ntwrk_client: &mut NetworkClient,
) -> Result<(), CafError> {
    let futures = pkgman.all_package_ids_iter()?.map(async |package_id| {
        ntwrk_client
            .clone()
            .start_providing(package_id.to_string())
            .await
    });

    future::join_all(futures).await;

    Ok(())
}

pub async fn start_pkg_provider(
    pkgman: &PackageManager,
    ntwrk: &mut Network,
    ntwrk_client: &mut NetworkClient,
) -> Result<(), CafError> {
    register_provided_packages(pkgman, ntwrk_client).await?;
    loop {
        match ntwrk.next_event().await {
            Event::InboundRequest { request, channel } => {
                // TODO: maybe respond with an error if this node doesn't have the package
                if let Ok(package) = pkgman.retrieve_package(&request) {
                    ntwrk_client.respond_package(package.content, channel).await;
                }
            }
        }
    }
}
