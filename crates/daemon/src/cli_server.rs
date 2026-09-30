use futures::{
    channel::{mpsc, oneshot},
    StreamExt,
};
use interprocess::local_socket::{
    traits::ListenerExt, GenericNamespaced, ListenerOptions, ToNsName,
};

use crate::{
    errors::{CafError, WrapErrorInResult},
    network::NetworkClient,
    pkgman::{Package, PackageId, PackageManager},
};

enum CliCommand {
    GetActivePackageVersion {
        pkg_name: String,
        back_sender: oneshot::Sender<Result<PackageId, CafError>>,
    },
}

impl CliCommand {
    fn new(raw_command: string, back_sender: oneshot::Sender<Result<PackageId, CafError>>) -> Self {
        // TODO: parse the string
        let commnand = CliCommand::GetActivePackageVersion {
            pkg_name: raw_command,
        };

        CliCommandEvent {
            command: GetActivePackageVersion {
                pkg_name: raw_command,
            },
            back_sender,
        }
    }
}

// TODO: Maybe it is a good idea to maintain / persist the commands and their results in a database
// since the cli itself might break while this daemon is running and potentially successfully
// completing the operation
// TODO: implement security checks. Idea:
// 1. Before the daemon starts:
//      a. check the installed version of the cli
//      b. download the hash of this version of the cli executable
//      c. keep the hash in memory for later use
//
// When a command is received:
// 1. Check that the process name corresponds to the absolute path of the cli executable that we
//    expect
// 2. compute the hash of the cli executable file.
// 3. compare it against the known hash of the executable
const CHANNEL_BUFFER_SIZE: usize = 10;
async fn start_cli_server(
    project_name: String,
    pkgman: &PackageManager,
    ntwrk_client: &mut NetworkClient,
) -> Result<(), CafError> {
    let socket_name = format!("{}.sock", project_name)
        .to_ns_name::<GenericNamespaced>()
        .wrap_err("create the cli server socket name")?;

    let listener = ListenerOptions::new()
        .name(socket_name)
        .create_sync()
        .wrap_err("create the cli server listener")?;

    let (command_sender, command_receiver) = mpsc::channel(CHANNEL_BUFFER_SIZE);

    Ok(())
}

struct CommandProcessor<'a> {
    pkgman: &'a PackageManager,
    ntwrk_client: &'a NetworkClient,
    incoming_commands_recv: mpsc::Receiver<CliCommand>,
}

impl<'a> CommandProcessor<'a> {
    fn new(
        pkgman: &'a PackageManager,
        ntwrk_client: &'a NetworkClient,
        incoming_commands_recv: mpsc::Receiver<CliCommand>,
    ) -> Self {
        CommandProcessor {
            pkgman,
            ntwrk_client,
            incoming_commands_recv,
        }
    }

    async fn run_processor(&mut self) {
        loop {
            self.incoming_commands_recv.next().await.inspect(|command| {
                self.process_command(command);
            });
        }
    }

    async fn process_command(&mut self, command: &CliCommand) {
        match command {
            CliCommand::GetActivePackageVersion { pkg_name } => {
                // TODO: implement this
            }
        }
    }

    async fn get_active_package() -> Result<PackageId, CafError> {}
}

async fn process_command_loop(pkgman: &PackageManager, ntwrk_client: &mut NetworkClient) {}

// TODO: this should listen on a unix socket for cli commands
pub async fn start_pkg_consumer(
    pkgman: &PackageManager,
    ntwrk_client: &mut NetworkClient,
) -> Result<(), CafError> {
    let name = "".to_string();
    let version: String = "7.0".into(); // TODO add option to get it from cli args
                                        // TODO If the version is not provided by the user default to getting the latest
                                        // version.. Make a request with an optional parameter (version) that would get the
                                        // hash of the package based on the version if it is provided and defaults to the
                                        // latest if not

    let providers = ntwrk_client.get_providers(name.clone()).await;

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
            let mut network_client = ntwrk_client.clone();
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
    pkgman.install_package(Package {
        id: package_id,
        content: package_content,
    })?;

    return Ok(());
}
