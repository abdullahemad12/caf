use clap::Parser;
use libp2p::Multiaddr;
use std::{error::Error, fs, path::PathBuf};

use crate::{
    errors::{CafError, WrapErrorInResult},
    network::Peer,
    worker::{bootstrap, start_pkg_consumer, start_pkg_provider},
};

mod errors;
mod lock;
mod network;
mod pkgman;
mod utils;
mod worker;

pub const PROJECT_NAME: &str = env!("CARGO_PKG_NAME");

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Opt {
    // this option will be used for bootstrap nodes
    #[arg(short, long)]
    no_bootstrap: bool,

    #[arg(short, long)]
    bootstrap_nodes: Vec<Peer>,

    #[arg(short, long)]
    listen_address: Option<Multiaddr>,

    #[arg(short = 'd', long)]
    home_dir: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let opt = Opt::parse();

    lock::acquire_caf_lock().unwrap();

    let (mut ntwrk, ntwrk_client) = network::Network::init(network::NetworkConfig::new()).unwrap();

    let pkg_man = pkgman::PackageManager::new(get_root_dir_path(opt.home_dir).unwrap()).unwrap();

    let mut provider_client = ntwrk_client.clone();
    let provider = start_pkg_provider(&pkg_man, &mut ntwrk, &mut provider_client);
    if !opt.no_bootstrap {
        bootstrap(opt.bootstrap_nodes, &mut ntwrk_client.clone())
            .await
            .unwrap();

        let mut consumer_client = ntwrk_client.clone();
        let consumer = start_pkg_consumer(&pkg_man, &mut consumer_client);
        let (_, consumer_res) = tokio::join!(provider, consumer);
        consumer_res?;
    } else {
        provider.await
    }

    Ok(())
}

// TODO maybe this needs to be OS dependent
// TODO this needs to be configurable (e.g through cli options or through a config file)
fn get_root_dir_path(home: Option<PathBuf>) -> Result<PathBuf, CafError> {
    let path = home
        .unwrap_or(PathBuf::from("/home"))
        .join(PathBuf::from(&format!(
            "{}/.{}",
            PROJECT_NAME, PROJECT_NAME
        )));

    fs::create_dir_all(&path).wrap_err("the root directory couldn't be created")?;

    Ok(path)
}
