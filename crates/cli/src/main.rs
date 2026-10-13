use std::process::ExitCode;

use clap::{Parser, Subcommand};
use observability::error::{CafError, WrapErrorInResult};
use protocol::protocol::{GetActivePackageVersionRequest, cli_service_client::CliServiceClient};
use tonic::transport::Channel;

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
        eprintln!("{}", err);
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

    println!("{} {}", resp.package_name, resp.version);

    Ok(())
}
