use futures::{
    SinkExt, StreamExt,
    channel::{mpsc, oneshot},
};

use protocol::protocol::{
    GetActivePackageVersionRequest, GetActivePackageVersionResponse, cli_service_server::CliService,
};
use tokio::spawn;
use tonic::{Request, Response, Status, transport::server::UdsConnectInfo};

use crate::{network::NetworkClient, pkgman::PackageManager};

use observability::error::{CafError, MapErrToStatus, WrapErrorInResult};

pub struct CliServer {
    _cmd_sender: mpsc::Sender<CliCommand>,
}

#[tonic::async_trait]
impl CliService for CliServer {
    async fn get_active_package_version(
        &self,
        req: Request<GetActivePackageVersionRequest>,
    ) -> Result<Response<GetActivePackageVersionResponse>, Status> {
        let (sender, receiver) = oneshot::channel();

        let cmd = CliCommand::GetActivePackageVersion {
            req: req.into_inner(),
            back_sender: sender,
        };

        if let Err(e) = self.enqueue_command(cmd).await {
            return Err(tonic::Status::internal(e.to_string()));
        }

        receiver
            .await
            .wrap_err("sender not to be dropped")
            .map_err_to_status()?
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
impl CliServer {
    fn authorization_interceptor(req: Request<()>) -> Result<Request<()>, Status> {
        let info = match req.extensions().get::<UdsConnectInfo>() {
            Some(info) => info,
            None => return Err(tonic::Status::permission_denied("UdsConnectInfo missing")),
        };

        println!(
            "request Info {}",
            info.peer_cred.map(|it| it.gid() as i32).unwrap_or(-1)
        );

        // TODO:
        // 1. get the path of the process's program file that made the connection
        // 2. get it's version by invoking it with the -v flag
        // 3. Hash this file
        // 4. compare it with a known hash of the this cli executable (e.g. by getting it from a
        //    centeral server like github)
        // 5. Cache the downloaded hash for next time

        Ok(req)
    }

    pub async fn start(
        pkgman: PackageManager,
        ntwrk_client: NetworkClient,
    ) -> Result<(), CafError> {
        let (_cmd_sender, cmd_recv) = mpsc::channel(CHANNEL_BUFFER_SIZE);

        let mut cmd_processor = CommandProcessor::new(pkgman, ntwrk_client, cmd_recv);
        let cli_server = Self { _cmd_sender };

        let processor_task = spawn(async move { cmd_processor.run().await });

        protocol::setup_server(cli_server, vec![Self::authorization_interceptor]).await?;

        // note the cli_server's ownership is moved to
        // the builder using add_service. This means
        // after the await, the cli_server is dropped
        // the cmd_sender is then dropped as it is owned by
        // the cli_server which consequently signals
        // the cmd_recv to return None and terminate
        // the loop
        processor_task
            .await
            .wrap_err("cli commands processor terminated with an error")?; // wait until the loop clean

        Ok(())
    }

    async fn enqueue_command(&self, cmd: CliCommand) -> Result<(), CafError> {
        self._cmd_sender
            .clone()
            .send(cmd)
            .await
            .wrap_err("enqueue command")
    }
}

enum CliCommand {
    GetActivePackageVersion {
        req: GetActivePackageVersionRequest,
        back_sender: oneshot::Sender<Result<Response<GetActivePackageVersionResponse>, Status>>,
    },
}

struct CommandProcessor {
    _pkgman: PackageManager,
    _ntwrk_client: NetworkClient,
    _commands_recv: mpsc::Receiver<CliCommand>,
}

impl CommandProcessor {
    fn new(
        _pkgman: PackageManager,
        _ntwrk_client: NetworkClient,
        _commands_recv: mpsc::Receiver<CliCommand>,
    ) -> Self {
        CommandProcessor {
            _pkgman,
            _ntwrk_client,
            _commands_recv,
        }
    }

    async fn run(&mut self) {
        loop {
            let command = match self._commands_recv.next().await {
                Some(command) => command,
                None => {
                    // when all the senders are dropped, the call to next() will return None
                    // signaling the end of the stream
                    self._commands_recv.close();
                    return;
                }
            };

            match command {
                CliCommand::GetActivePackageVersion { req, back_sender } => back_sender
                    .send(self.process_get_active_package_version(&req).await)
                    .unwrap_or_else(|_| {
                        eprintln!("error sending the response back for a get_active_package_version request of package: {}", req.package_name);
                    }),
            }
        }
    }

    async fn process_get_active_package_version(
        &mut self,
        req: &GetActivePackageVersionRequest,
    ) -> Result<Response<GetActivePackageVersionResponse>, Status> {
        self._pkgman
            .retrieve_active_package_version(&req.package_name)
            .map(|package_id| {
                Response::new(GetActivePackageVersionResponse {
                    package_name: package_id.name,
                    version: package_id.version,
                })
            })
            .map_err_to_status()
    }
}

// TODO: this should listen on a unix socket for cli commands
/*pub async fn start_pkg_consumer(
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
}*/
