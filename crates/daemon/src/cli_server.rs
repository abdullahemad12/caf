use futures::{
    FutureExt, SinkExt, StreamExt,
    channel::{mpsc, oneshot},
    future,
};

use protocol::protocol::{
    GetActivePackageVersionRequest, GetActivePackageVersionResponse, InstallPackageRequest,
    InstallPackageResponse, cli_service_server::CliService,
};
use tokio::spawn;
use tonic::{Request, Response, Status, transport::server::UdsConnectInfo};

use crate::{
    network::NetworkClient,
    pkgman::{Package, PackageId, PackageManager},
};

use observability::error::{CafError, CafErrorCode, MapErrToStatus, WrapErrorInResult};

pub struct CliServer {
    _cmd_sender: mpsc::Sender<CliCommand>,
}

#[tonic::async_trait]
impl CliService for CliServer {
    async fn install_package(
        &self,
        req: Request<InstallPackageRequest>,
    ) -> Result<Response<InstallPackageResponse>, Status> {
        let (sender, receiver) = oneshot::channel();

        let cmd = CliCommand::InstallPackage {
            req: req.into_inner(),
            back_sender: sender,
        };

        self.execute_command(cmd, receiver).await
    }

    async fn get_active_package_version(
        &self,
        req: Request<GetActivePackageVersionRequest>,
    ) -> Result<Response<GetActivePackageVersionResponse>, Status> {
        let (sender, receiver) = oneshot::channel();

        let cmd = CliCommand::GetActivePackageVersion {
            req: req.into_inner(),
            back_sender: sender,
        };

        self.execute_command(cmd, receiver).await
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

        tracing::info!(
            "request info: {}",
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

    async fn execute_command<T>(
        &self,
        cmd: CliCommand,
        receiver: oneshot::Receiver<Result<Response<T>, Status>>,
    ) -> Result<Response<T>, Status> {
        self._cmd_sender
            .clone()
            .send(cmd)
            .await
            .wrap_err("enqueue command")
            .map_err_to_status()?;

        receiver
            .await
            .wrap_err("sender was dropped")
            .map_err_to_status()?
    }
}

enum CliCommand {
    GetActivePackageVersion {
        req: GetActivePackageVersionRequest,
        back_sender: oneshot::Sender<Result<Response<GetActivePackageVersionResponse>, Status>>,
    },
    InstallPackage {
        req: InstallPackageRequest,
        back_sender: oneshot::Sender<Result<Response<InstallPackageResponse>, Status>>,
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
                        tracing::error!(
                            "failed to send response back for get_active_package_version request for package: {}",
                            req.package_name
                        );
                    }),
                CliCommand::InstallPackage { req, back_sender } => back_sender
                    .send(self.process_install_package(&req).await)
                    .unwrap_or_else(|_| {
                        tracing::error!(
                            "failed to send response back for install_package request for package: {}",
                            req.user_package_id
                        );
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
                    version: package_id.version.to_string(),
                })
            })
            .map_err_to_status()
    }

    // TODO: currently this will block the rpc request until this is done. Perhaps it respond early
    // and let the cli poll for the progress of this request. This will allow the daemon to report
    // intermediate progress to the cli like how much percentage is already downloaded and what the
    // daemon is doing
    async fn process_install_package(
        &mut self,
        req: &InstallPackageRequest,
    ) -> Result<Response<InstallPackageResponse>, Status> {
        let package_id: PackageId = req.user_package_id.clone().try_into().map_err_to_status()?;

        let providers = self._ntwrk_client.get_providers(package_id.clone()).await;

        if providers.is_empty() {
            return Err(CafError::new(
                CafErrorCode::PackageVersionHasNoProviders,
                "no providers for the given package version",
            ))
            .map_err_to_status();
        }

        let requests: Vec<_> = providers
            .into_iter()
            .map(|it| {
                let mut network_client = self._ntwrk_client.clone();
                let package_id = package_id.clone();
                async move { network_client.request_package(it, package_id).await }.boxed()
            })
            .collect();

        // we only need one of them to respond
        let package_content = future::select_ok(requests).await.map_err_to_status()?.0;

        // TODO: Check for package integrity
        let package = Package::new(package_id.clone(), package_content);

        self._pkgman.install_package(package).map_err_to_status()?;

        self._ntwrk_client.clone().start_providing(package_id).await;

        Ok(Response::new(InstallPackageResponse {}))
    }
}
