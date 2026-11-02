use std::{io::Write, process::ExitCode};

use clap::{Parser, Subcommand};
use directories::ProjectDirs;
use observability::{
    error::{CafError, CafErrorCode, WrapErrorInResult},
    tracing::setup_tracing,
};
use protocol::protocol::{
    GetActivePackageVersionRequest, InstallPackageRequest, cli_service_client::CliServiceClient,
};
use tonic::transport::Channel;

#[macro_export]
macro_rules! caf_info {
    ($($arg:tt)*) => {
        tracing::event!(
            target: module_path!(),
            tracing::Level::INFO,
            message = format_args!(
                "☕{}",
                format_args!($($arg)*)
            )
        );
    };
}

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

    #[command(name = "install")]
    InstallPackage { user_package_id: String },
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
            tracing::error!("{err}");
            return ExitCode::FAILURE;
        }
    };

    let result = match command.command {
        Commands::GetActivePackageVersion { package_name } => {
            get_active_package_version(cli_client, package_name).await
        }
        Commands::InstallPackage { user_package_id } => {
            install_package(cli_client, user_package_id).await
        }
    };

    if let Err(err) = result {
        tracing::error!("{err}");
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

    caf_info!("{} {}", resp.package_name, resp.version);

    Ok(())
}

async fn install_package(
    cli_client: CliServiceClient<Channel>,
    user_package_id: String,
) -> Result<(), CafError> {
    let resp = send_and_track_installation_request(
        cli_client.clone(),
        InstallPackageRequest {
            user_package_id: user_package_id.clone(),
            force_install: false,
        },
    )
    .await;

    let installation_err = match resp {
        Ok(_) => {
            caf_info!("successfully installed {}", user_package_id.clone());
            return Ok(());
        }
        Err(err) => err,
    };

    if *installation_err.code() != CafErrorCode::PackageAlreadyInstalled {
        return Err(installation_err);
    }

    tracing::warn!("{} is already installed. re-installing", user_package_id);
    if !prompt_user_confirmation("proceed with installation?")? {
        return Err(installation_err);
    }

    send_and_track_installation_request(
        cli_client,
        InstallPackageRequest {
            user_package_id: user_package_id.clone(),
            force_install: true,
        },
    )
    .await?;

    caf_info!("successfully installed {}", user_package_id.clone());

    Ok(())
}

// TODO: this should be used later to also track the progress of the installation and report
// it to the user instead of blocking until everything is done
async fn send_and_track_installation_request(
    mut cli_client: CliServiceClient<Channel>,
    req: InstallPackageRequest,
) -> Result<(), CafError> {
    cli_client
        .install_package(req)
        .await
        .map(|_| ())
        .wrap_err("installing the package failed")
}

fn prompt_user_confirmation(confirmation_msg: impl Into<String>) -> Result<bool, CafError> {
    write!(std::io::stdout(), ":: {} [Y/n]: ", confirmation_msg.into())
        .wrap_err("unable to write the confiramtion message to stdout")?;

    std::io::stdout()
        .flush()
        .wrap_err("unable to flush the confirmation message to stdout")?;

    let mut user_input = String::new();
    std::io::stdin()
        .read_line(&mut user_input)
        .wrap_err("unable to read the confirmation from stdin")?;

    Ok(user_input.trim().to_lowercase() == "y")
}
