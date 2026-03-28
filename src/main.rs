use clap::Parser;
use futures::FutureExt;
use libp2p::{multiaddr::Protocol, Multiaddr};
use std::{error::Error, io::Write, path::PathBuf};

mod network;
mod package;
mod pkgman;
mod utils;

#[derive(Parser, Debug)]
#[command(name = "libp2p file sharing example")]
struct Opt {
    #[arg(long)]
    boostrap: Option<Multiaddr>,

    #[arg(long)]
    listen_address: Option<Multiaddr>,

    #[command(subcommand)]
    argument: CliArgument,
}

#[derive(Debug, Parser)]
enum CliArgument {
    Provide {
        #[arg(long)]
        path: PathBuf,

        #[arg(long)]
        name: String,
    },
    Get {
        #[arg(long)]
        name: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let opt = Opt::parse();

    let (package_name, package_path) = match &opt.argument {
        CliArgument::Provide { name, path } => (name.clone(), path.clone()),
        CliArgument::Get { name } => (name.clone(), "".into()),
    };

    let (mut ntwrk, mut ntwrk_client) = network::Network::init(network::NetworkConfig::new())
        .expect("to initialize network successfully");

    let pkg_man = pkgman::PackageManager::new("fix me".to_string());

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
                        if let Ok(package) = pkg_man.retrieve_package(&request) {
                            ntwrk_client.respond_package(package.content, channel).await;
                        }
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
