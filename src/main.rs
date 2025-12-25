use clap::Parser;
use futures::FutureExt;
use libp2p::{multiaddr::Protocol, Multiaddr};
use std::{error::Error, io::Write, path::PathBuf};

mod network;
mod package;
mod pkgman;

#[derive(Parser, Debug)]
#[command(name = "libp2p file sharing example")]
struct Opt {
    #[arg(long)]
    peer: Option<Multiaddr>,

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

    let pkg_man =
        pkgman::PackageManager::new(package::PackageId { name: package_name }, package_path);

    if let Some(addr) = opt.peer {
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
                        if let Some(package) = pkg_man.retrieve_package(request) {
                            ntwrk_client.respond_package(package, channel).await;
                        }
                    }
                }
            }
        }
        CliArgument::Get { name } => {
            let providers = ntwrk_client.get_providers(name.clone()).await;

            if providers.is_empty() {
                return Err(format!("could not find providers for file {}", name).into());
            };

            let requests: Vec<_> = providers
                .into_iter()
                .map(|it| {
                    let mut network_client = ntwrk_client.clone();
                    let name = name.clone();
                    async move { network_client.request_package(it, name).await }.boxed()
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
