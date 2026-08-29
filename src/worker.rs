use futures::FutureExt;

use crate::errors::{CafError, WrapErrorInResult};
use crate::network::{Event, Network, NetworkClient};
use crate::pkgman::{Package, PackageId, PackageManager};

pub struct CafWorker {
    pkgman: PackageManager,
    network: Network,
    network_client: NetworkClient,
}

impl CafWorker {
    pub fn new(pkgman: PackageManager, network: Network, network_client: NetworkClient) -> Self {
        Self {
            pkgman,
            network,
            network_client,
        }
    }

    // TODO: this should loop over all packages and register them in kademlia
    // if the number of packages exceeds some limit, maybe pick a random set every
    // 10 minutes or so to provide
    pub async fn register_provided_packages(&mut self) {
        self.network_client
            .start_providing("".to_string().clone())
            .await;
    }

    // TODO: setup
    pub async fn start_pkg_provider(&mut self) {
        self.register_provided_packages().await;
        match self.network.next_event().await {
            Event::InboundRequest { request, channel } => {
                // TODO: do I need to respond with an error here if this node does not have the package
                if let Ok(package) = self.pkgman.retrieve_package(&request) {
                    self.network_client
                        .respond_package(package.content, channel)
                        .await;
                }
            }
        }
    }

    // TODO: this should listen on a unix socket for cli commands
    pub async fn start_pkg_consumer(&mut self) -> Result<(), CafError> {
        let name = "".to_string();
        let version: String = "7.0".into(); // TODO add option to get it from cli args
                                            // TODO If the version is not provided by the user default to getting the latest
                                            // version.. Make a request with an optional parameter (version) that would get the
                                            // hash of the package based on the version if it is provided and defaults to the
                                            // latest if not

        let providers = self.network_client.get_providers(name.clone()).await;

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
                let mut network_client = self.network_client.clone();
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
        self.pkgman.install_package(Package {
            id: package_id,
            content: package_content,
        })?;

        return Ok(());
    }
}
