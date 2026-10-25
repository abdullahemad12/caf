use std::process::ExitCode;

use clap::{Parser, Subcommand};
use directories::ProjectDirs;
use observability::{
    error::{CafError, WrapErrorInResult},
    tracing::setup_tracing,
};
use protocol::protocol::{GetActivePackageVersionRequest, cli_service_client::CliServiceClient};
use tonic::transport::Channel;

const CRATE_NAME: &str = env!("CARGO_PKG_NAME");

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    #[command(name = "active-version")]
    GetActivePackageVersion { package_name: String },
}

#[tokio::main]
async fn main() -> ExitCode {
    let command = Args::parse();

    // TODO falling back to some static default path can be an option
    let project_dirs =
        ProjectDirs::from("", "", CRATE_NAME).expect("unable to resolve the project directories");

    let _gaurd = setup_tracing(project_dirs.data_dir().to_path_buf());

    let cli_client = match protocol::setup_client().await {
        Ok(client) => client,
        Err(err) => {
            eprintln!("{}", err);
            return ExitCode::FAILURE;
        }
    };

    let result = match command.command {
        Commands::GetActivePackageVersion { package_name } => {
            get_active_package_version(cli_client, package_name).await
        }
    };

    if let Err(err) = result {
        tracing::error!("{}", err);
        return ExitCode::FAILURE;
    };

    ExitCode::SUCCESS
}

async fn get_active_package_version(
    mut cli_client: CliServiceClient<Channel>,
    package_name: String,
) -> Result<(), CafError> {
    let resp = cli_client
        .get_active_package_version(GetActivePackageVersionRequest { package_name })
        .await
        .wrap_err("getting active package version failed")?
        .into_inner();

    tracing::info!("{} {}", resp.package_name, resp.version);

    Ok(())
}
