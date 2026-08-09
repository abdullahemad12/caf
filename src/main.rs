use clap::Parser;
use futures::FutureExt;
use libp2p::{multiaddr::Protocol, Multiaddr};
use std::{error::Error, fs, io::Write, path::PathBuf};

use crate::errors::{CafError, WrapError};

mod errors;
mod lock;
mod network;
mod package;
mod pkgman;
mod utils;

pub const PROJECT_NAME: &str = env!("CARGO_PKG_NAME");

#[derive(Parser, Debug)]
#[command(name = "libp2p file sharing example")]
struct Opt {
    #[arg(long)]
    boostrap: Option<Multiaddr>,

    #[arg(long)]
    listen_address: Option<Multiaddr>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let opt = Opt::parse();

    lock::acquire_caf_lock().unwrap();

    let (mut ntwrk, mut ntwrk_client) =
        network::Network::init(network::NetworkConfig::new()).unwrap();

    let pkg_man = pkgman::PackageManager::new(get_root_dir_path().unwrap());

    if let Some(addr) = opt.boostrap {
        let Some(Protocol::P2p(peer_id)) = addr.iter().last() else {
            return Err("expect peer multiaddr to contain peer id".into());
        };

        ntwrk_client
            .dial(peer_id, addr)
            .await
            .expect("dial to succeed");

        ntwrk_client
            .bootstrap()
            .await
            .expect("bootstrap to succeed");
    }

    match opt.argument {
        CliArgument::Provide { name, .. } => {
            ntwrk_client.start_providing(name.clone()).await;

            loop {
                match ntwrk.next_event().await {
                    network::Event::InboundRequest { request, channel } => {
                        /*if let Ok(package) = pkg_man.retrieve_package(&request) {
                            ntwrk_client.respond_package(package.content, channel).await;
                        }*/
                    }
                }
            }
        }
        CliArgument::Get { name } => {
            let version: String = "7.0".into(); // TODO add option to get it from cli args
                                                // TODO If the version is not provided by the user default to getting the latest
                                                // version.. Make a request with an optional parameter (version) that would get the
                                                // hash of the package based on the version if it is provided and defaults to the
                                                // latest if not

            let providers = ntwrk_client.get_providers(name.clone()).await;

            if providers.is_empty() {
                return Err(format!("could not find providers for file {}", name).into());
            };

            let requests: Vec<_> = providers
                .into_iter()
                .map(|it| {
                    let mut network_client = ntwrk_client.clone();
                    let name = name.clone();
                    let version = version.clone();

                    async move {
                        network_client
                            .request_package(it, package::PackageId { name, version })
                            .await
                    }
                    .boxed()
                })
                .collect();

            // we only need one of them to respond
            let package = futures::future::select_ok(requests)
                .await
                .map_err(|_| "none of the providers returned a file")?
                .0;

            std::io::stdout().write_all(&package.0)?;
        }
    };

    Ok(())
}

// TODO maybe this needs to be OS dependent
// TODO this needs to be configurable (e.g through cli options or through a config file)
fn get_root_dir_path() -> Result<PathBuf, CafError> {
    let default_path = PathBuf::from(&format!("/home/{}/.{}", PROJECT_NAME, PROJECT_NAME));

    let path = default_path; // should be determined the following precedence: cli args > config file > default

    fs::create_dir_all(&path).wrap_err("the root directory couldn't be created")?;

    Ok(path)
}
