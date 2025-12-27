use futures::{
    channel::{mpsc, oneshot},
    prelude::*,
};
use std::{
    collections::{hash_map, HashMap, HashSet},
    pin,
};

use libp2p::{
    identify, identity, kad,
    multiaddr::Protocol,
    noise,
    request_response::{self, OutboundRequestId, ProtocolSupport, ResponseChannel},
    swarm::{NetworkBehaviour, Swarm, SwarmEvent},
    tcp, yamux, Multiaddr, PeerId, StreamProtocol,
};
use std::{error::Error, time::Duration};
use tokio::spawn;

use crate::package;

#[derive(Debug)]
pub enum Event {
    InboundRequest {
        request: package::PackageId,
        channel: ResponseChannel<package::Package>,
    },
}

enum Command {
    Dial {
        peer_id: PeerId,
        peer_addr: Multiaddr,
        sender: oneshot::Sender<Result<(), Box<dyn Error + Send>>>,
    },
    GetProviders {
        package_name: String,
        sender: oneshot::Sender<HashSet<PeerId>>,
    },
    RequestPackage {
        package_name: String,
        peer_id: PeerId,
        sender: oneshot::Sender<Result<package::Package, Box<dyn Error + Send>>>,
    },
    ResponsePackage {
        package: package::Package,
        channel: ResponseChannel<package::Package>,
    },
    StartProviding {
        package_name: String,
        sender: oneshot::Sender<()>,
    },
    Bootstrap {
        sender: oneshot::Sender<Result<(), Box<dyn Error + Send>>>,
    },
}

#[derive(NetworkBehaviour)]
struct Behaviour {
    request_response: request_response::cbor::Behaviour<package::PackageId, package::Package>,
    kademlia: kad::Behaviour<kad::store::MemoryStore>,
    identify: identify::Behaviour,
}

#[derive(Clone)]
pub struct NetworkConfig {
    idle_connection_timeout: Duration,
    client_address: String,
}

impl NetworkConfig {
    const DEFAULT_IDLE_CONNECTION_TIMEOUT: Duration = Duration::from_secs(60);
    const DEFAULT_CLIENT_ADDRESS: &'static str = "/ip4/0.0.0.0/tcp/0";

    pub fn new() -> Self {
        Self {
            idle_connection_timeout: NetworkConfig::DEFAULT_IDLE_CONNECTION_TIMEOUT,
            client_address: NetworkConfig::DEFAULT_CLIENT_ADDRESS.to_string(),
        }
    }
}

#[derive(Clone)]
pub struct NetworkClient {
    sender: mpsc::Sender<Command>,
}

impl NetworkClient {
    fn new(sender: mpsc::Sender<Command>) -> Self {
        return NetworkClient { sender };
    }

    pub async fn request_package(
        &mut self,
        peer_id: PeerId,
        package_name: String,
    ) -> Result<package::Package, Box<dyn Error + Send>> {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(Command::RequestPackage {
                package_name,
                peer_id,
                sender,
            })
            .await
            .expect("receiver not to be dropped");
        receiver.await.expect("sender not to be dropped")
    }

    pub async fn dial(
        &mut self,
        peer_id: PeerId,
        peer_addr: Multiaddr,
    ) -> Result<(), Box<dyn Error + Send>> {
        let (sender, receiver) = oneshot::channel();

        self.sender
            .send(Command::Dial {
                peer_id: peer_id,
                peer_addr: peer_addr,
                sender: sender,
            })
            .await
            .expect("command receiver not to be dropped");

        receiver.await.expect("sender not to be dropped")
    }

    pub async fn bootstrap(&mut self) -> Result<(), Box<dyn Error + Send>> {
        let (sender, receiver) = oneshot::channel();

        self.sender
            .send(Command::Bootstrap { sender })
            .await
            .expect("command receiver not to be dropped");

        receiver.await.expect("sender not to be dropped")
    }

    pub async fn start_providing(&mut self, package_name: String) {
        let (sender, receiver) = oneshot::channel();

        self.sender
            .send(Command::StartProviding {
                package_name,
                sender,
            })
            .await
            .expect("command receiver not to be dropped");

        receiver.await.expect("sender not to be dropped");
    }

    pub async fn respond_package(
        &mut self,
        package: package::Package,
        channel: ResponseChannel<package::Package>,
    ) {
        self.sender
            .send(Command::ResponsePackage { package, channel })
            .await
            .expect("command receiver not to be dropped");
    }

    pub async fn get_providers(&mut self, package_name: String) -> HashSet<PeerId> {
        let (sender, receiver) = oneshot::channel();

        self.sender
            .send(Command::GetProviders {
                package_name,
                sender,
            })
            .await
            .expect("command receiver not to be dropped");

        receiver.await.expect("sender not to be dropped")
    }
}

pub struct Network {
    stream: pin::Pin<Box<dyn Stream<Item = Event>>>,
}
macro_rules! protocol_version {
    () => {
        "0.0.1"
    };
}

impl Network {
    const PROTOCOL_VERSION: &'static str = protocol_version!();
    const PROTOCOL_NAME: &'static str = concat!("/caf/", protocol_version!());

    pub fn init(network_conf: NetworkConfig) -> Result<(Self, NetworkClient), Box<dyn Error>> {
        let id_keys = identity::Keypair::generate_ed25519();

        let peer_id = id_keys.public().to_peer_id();

        let mut swarm = libp2p::SwarmBuilder::with_existing_identity(id_keys)
            .with_tokio()
            .with_tcp(
                tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )?
            .with_behaviour(|key| Behaviour {
                kademlia: kad::Behaviour::new(
                    peer_id,
                    kad::store::MemoryStore::new(key.public().to_peer_id()),
                ),
                request_response: request_response::cbor::Behaviour::new(
                    [(
                        StreamProtocol::new(Network::PROTOCOL_NAME),
                        ProtocolSupport::Full,
                    )],
                    request_response::Config::default(),
                ),
                identify: identify::Behaviour::new(identify::Config::new(
                    Network::PROTOCOL_VERSION.to_string(),
                    key.public(),
                )),
            })?
            .with_swarm_config(|conf| {
                conf.with_idle_connection_timeout(network_conf.idle_connection_timeout)
            })
            .build();

        swarm
            .behaviour_mut()
            .kademlia
            .set_mode(Some(kad::Mode::Server));

        swarm.listen_on(network_conf.client_address.parse()?)?;

        let (command_sender, command_receiver) = mpsc::channel(0);
        let (event_sender, event_receiver) = mpsc::channel(0);

        let event_loop = EventLoop::new(swarm, command_receiver, event_sender);

        spawn(event_loop.run());

        Ok((
            Network {
                stream: Box::pin(event_receiver),
            },
            NetworkClient::new(command_sender),
        ))
    }

    pub async fn next_event(&mut self) -> Event {
        self.stream.next().await.expect("event to be 'Some'")
    }
}

struct EventLoop {
    swarm: Swarm<Behaviour>,
    command_receiver: mpsc::Receiver<Command>,
    event_sender: mpsc::Sender<Event>,
    pending_dial: HashMap<PeerId, oneshot::Sender<Result<(), Box<dyn Error + Send>>>>,
    pending_bootstrap: HashMap<kad::QueryId, oneshot::Sender<Result<(), Box<dyn Error + Send>>>>,
    pending_start_providing: HashMap<kad::QueryId, oneshot::Sender<()>>,
    pending_get_providers: HashMap<kad::QueryId, oneshot::Sender<HashSet<PeerId>>>,
    pending_request_package: HashMap<
        OutboundRequestId,
        oneshot::Sender<Result<package::Package, Box<dyn Error + Send>>>,
    >,
}

impl EventLoop {
    fn new(
        swarm: Swarm<Behaviour>,
        command_receiver: mpsc::Receiver<Command>,
        event_sender: mpsc::Sender<Event>,
    ) -> Self {
        Self {
            swarm,
            command_receiver,
            event_sender,
            pending_dial: Default::default(),
            pending_bootstrap: Default::default(),
            pending_start_providing: Default::default(),
            pending_get_providers: Default::default(),
            pending_request_package: Default::default(),
        }
    }

    async fn run(mut self) {
        loop {
            tokio::select! {
                event = self.swarm.select_next_some() => self.process_event(event).await,
                command = self.command_receiver.next() => match command {
                    Some(command) => self.process_command(command).await,
                        // the network event loop is shutdown when the Command channel is closed
                    None => return,
                }
            }
        }
    }

    async fn process_event(&mut self, event: SwarmEvent<BehaviourEvent>) {
        match event {
            SwarmEvent::Behaviour(BehaviourEvent::Kademlia(event)) => {
                self.process_kademlia_event(event).await;
            }
            SwarmEvent::Behaviour(BehaviourEvent::RequestResponse(event)) => {
                self.process_request_response_event(event).await;
            }
            SwarmEvent::Behaviour(BehaviourEvent::Identify(event)) => {
                self.process_identify_event(event).await;
            }
            SwarmEvent::NewListenAddr { address, .. } => eprintln!(
                "Local node is already listening on {:?}",
                address.with(Protocol::P2p(*self.swarm.local_peer_id()))
            ),
            SwarmEvent::IncomingConnection { .. } => {}
            SwarmEvent::ConnectionEstablished {
                peer_id, endpoint, ..
            } => {
                if endpoint.is_dialer() {
                    if let Some(sender) = self.pending_dial.remove(&peer_id) {
                        let _ = sender.send(Ok(()));
                    }
                }
            }
            SwarmEvent::ConnectionClosed { .. } => {}
            SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                if let Some(peer_id) = peer_id {
                    if let Some(sender) = self.pending_dial.remove(&peer_id) {
                        sender
                            .send(Err(Box::new(error)))
                            .expect("to send error to the consumer");
                    }
                }
            }
            SwarmEvent::IncomingConnectionError { error, .. } => {
                eprintln!("incoming connection error {:?}", error)
            }
            SwarmEvent::Dialing {
                peer_id: Some(peer_id),
                ..
            } => println!("Dialing peer {}", peer_id),
            unhandled => eprintln!("unhandled event {:?}", unhandled),
        }
    }

    async fn process_identify_event(&mut self, event: identify::Event) {
        match event {
            identify::Event::Received { peer_id, info, .. } => {
                info.listen_addrs.iter().for_each(|addr| {
                    self.swarm
                        .behaviour_mut()
                        .kademlia
                        .add_address(&peer_id, addr.clone());
                });
            }
            _ => eprintln!("unhandled identify event: {:?}", event),
        }
    }

    async fn process_kademlia_event(&mut self, event: kad::Event) {
        match event {
            kad::Event::OutboundQueryProgressed {
                id,
                result: kad::QueryResult::StartProviding(_),
                ..
            } => {
                // TODO: should this panic
                let sender: oneshot::Sender<()> = self.pending_start_providing.remove(&id).expect(
                    "to have the id for the completed query in the pending_start_providing map",
                );
                sender
                    .send(())
                    .expect("to confirm OutBoundQueryProgressed::StartPrividing to the consumer");
            }
            kad::Event::OutboundQueryProgressed {
                id,
                result:
                    kad::QueryResult::GetProviders(Ok(kad::GetProvidersOk::FoundProviders {
                        providers,
                        ..
                    })),
                ..
            } => {
                // TODO: should the None case be ignored
                if let Some(sender) = self.pending_get_providers.remove(&id) {
                    sender.send(providers).expect("receivers not to be dropped");

                    self.swarm
                        .behaviour_mut()
                        .kademlia
                        .query_mut(&id)
                        .expect("to have the query with the given id")
                        .finish()
                }
            }
            kad::Event::OutboundQueryProgressed {
                id,
                result: kad::QueryResult::Bootstrap(Ok(kad::BootstrapOk { .. })),
                ..
            } => {
                if let Some(sender) = self.pending_bootstrap.remove(&id) {
                    sender.send(Ok(())).expect("receiver not to be dropped");
                }
            }
            kad::Event::OutboundQueryProgressed {
                result:
                    kad::QueryResult::GetProviders(Ok(
                        kad::GetProvidersOk::FinishedWithNoAdditionalRecord { .. },
                    )),
                ..
            } => {}
            kad::Event::InboundRequest { request } => println!("Inbound request {:?}", request),
            unhandled => eprintln!("unhandled kademlia event: {:?}", unhandled),
        }
    }

    async fn process_request_response_event(
        &mut self,
        event: request_response::Event<package::PackageId, package::Package>,
    ) {
        match event {
            request_response::Event::Message { message, .. } => match message {
                request_response::Message::Request {
                    request, channel, ..
                } => {
                    self.event_sender
                        .send(Event::InboundRequest {
                            request: request,
                            channel,
                        })
                        .await
                        .expect("event_sender not to be dropped");
                }
                request_response::Message::Response {
                    request_id,
                    response,
                } => {
                    self.pending_request_package
                        .remove(&request_id)
                        .expect("request to still be pending")
                        .send(Ok(response))
                        .expect("channel not to be dropped");
                }
            },
            request_response::Event::OutboundFailure {
                request_id, error, ..
            } => {
                self.pending_request_package
                    .remove(&request_id)
                    .expect("request to still be pending")
                    .send(Err(Box::new(error)))
                    .expect("channel not to be dropped");
            }
            unhandled => eprintln!("unhandled request_response event {:?}", unhandled),
        };
    }

    async fn process_command(&mut self, command: Command) {
        match command {
            Command::Dial {
                peer_id,
                peer_addr,
                sender,
            } => match self.pending_dial.entry(peer_id) {
                hash_map::Entry::Vacant(entry) => {
                    self.swarm
                        .behaviour_mut()
                        .kademlia
                        .add_address(&peer_id, peer_addr.clone());

                    match self.swarm.dial(peer_addr.with(Protocol::P2p(peer_id))) {
                        Ok(()) => {
                            entry.insert(sender);
                        }
                        Err(err) => {
                            sender
                                .send(Err(Box::new(err)))
                                .expect("channel not to be dropped");
                        }
                    };
                }
                hash_map::Entry::Occupied(_) => {
                    eprintln!("already dialing the peer {:?}", peer_id);
                }
            },
            Command::GetProviders {
                package_name,
                sender,
            } => {
                let query_id = self
                    .swarm
                    .behaviour_mut()
                    .kademlia
                    .get_providers(package_name.into_bytes().into());

                self.pending_get_providers.insert(query_id, sender);
            }
            Command::RequestPackage {
                peer_id,
                package_name,
                sender,
            } => {
                let query_id = self
                    .swarm
                    .behaviour_mut()
                    .request_response
                    .send_request(&peer_id, package::PackageId { name: package_name });

                self.pending_request_package.insert(query_id, sender);
            }
            Command::ResponsePackage { channel, package } => {
                self.swarm
                    .behaviour_mut()
                    .request_response
                    .send_response(channel, package)
                    .expect("connection to pper to be still open");
            }
            Command::StartProviding {
                package_name,
                sender,
            } => {
                let query_id = self
                    .swarm
                    .behaviour_mut()
                    .kademlia
                    .start_providing(package_name.into_bytes().into())
                    .expect("no store error");

                self.pending_start_providing.insert(query_id, sender);
            }
            Command::Bootstrap { sender } => {
                let query_id = self
                    .swarm
                    .behaviour_mut()
                    .kademlia
                    .bootstrap()
                    .expect("no bootstrap store error");

                self.pending_bootstrap.insert(query_id, sender);
            }
        };
    }
}
