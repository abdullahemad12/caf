use clap::Parser;
use libp2p::Multiaddr;
use observability::error::{CafError, WrapErrorInResult};
use std::{fs, path::PathBuf};

use crate::{
    cli_server::CliServer,
    network::Peer,
    pkgman::{Package, PackageId, TarballCompressedPackageContent},
    provider::{bootstrap, start_pkg_provider},
};

mod cli_server;
mod lock;
mod network;
mod pkgman;
mod provider;
mod utils;

pub const PROJECT_NAME: &str = "caf";

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
async fn main() -> Result<(), CafError> {
    let opt = Opt::parse();

    // storing the lock in a variable ensures
    // that the file is not dropped until the
    // main function exists
    let _caf_lock = lock::acquire_caf_lock().unwrap();

    let (mut ntwrk, ntwrk_client) = network::Network::init(network::NetworkConfig::new()).unwrap();

    let read_pkgman =
        pkgman::ReadOnlyPackageManager::new(get_root_dir_path(opt.home_dir.clone())?)?;

    let mut provider_client = ntwrk_client.clone();
    let provider = start_pkg_provider(read_pkgman, &mut ntwrk, &mut provider_client);
    if !opt.no_bootstrap {
        bootstrap(opt.bootstrap_nodes, &mut ntwrk_client.clone()).await?
    }

    let pkg_man = pkgman::PackageManager::new(get_root_dir_path(opt.home_dir.clone())?)?;

    pkg_man
        .install_package(Package {
            id: PackageId {
                name: "test".to_string(),
                version: "7.0".to_string(),
            },
            content: TarballCompressedPackageContent("test pkg".as_bytes().to_vec()),
        })
        .unwrap();

    let cli_server = CliServer::start(pkg_man, ntwrk_client.clone());

    let (provider_res, cli_server_res) = tokio::join!(provider, cli_server);
    provider_res?;
    cli_server_res?;

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
