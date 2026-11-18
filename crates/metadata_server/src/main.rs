use directories::ProjectDirs;
use observability::{
    error::{CafError, WrapErrorInResult},
    tracing::setup_tracing,
};

use std::net::SocketAddr;

const CRATE_NAME: &str = env!("CARGO_PKG_NAME");
const DEFAULT_PORT: u16 = 3300;
const DEFAULT_ADDR: [u8; 4] = [0, 0, 0, 0];

#[tokio::main]
async fn main() -> Result<(), CafError> {
    // TODO falling back to some static default path can be an option
    let project_dir =
        ProjectDirs::from("", "", CRATE_NAME).expect("unable to resolve the project directories");

    let _gaurd = setup_tracing(project_dir.data_dir().to_path_buf());

    let app = metadata_server::server::create_app();
    let addr = SocketAddr::from((DEFAULT_ADDR, DEFAULT_PORT));

    tracing::info!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .wrap_err(format!("unable to start listening on the address {}", addr))?;

    axum::serve(listener, app)
        .await
        .wrap_err("unabele to start the server")?;

    Ok(())
}
