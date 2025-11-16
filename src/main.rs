use anyhow::{bail, Context, Result};
use clap::{Parser, ValueEnum};
use futures::StreamExt;
use libp2p::{
    kad::{self, store::MemoryStore, Mode, PeerRecord, Record, RecordKey},
    multiaddr::Protocol,
    noise, ping,
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux, Multiaddr, PeerId, SwarmBuilder,
};
use std::fs;
use tokio::signal;

const FILE_TO_PROVIDE: &str = "shared-file.bin";

#[derive(Copy, Clone, Debug, ValueEnum)]
enum Role {
    Provider,
    Client,
}

#[derive(Parser, Debug)]
#[command(author, version, about = "Minimal libp2p provider node")]
struct Args {
    /// Multiaddr of the bootstrap node, e.g. /ip4/1.2.3.4/tcp/4001/p2p/<peer_id>
    #[arg(long)]
    bootstrap: String,
    /// Local listen address. Use tcp/0 to auto-select a port.
    #[arg(long, default_value = "/ip4/0.0.0.0/tcp/0")]
    listen: String,
    /// Whether to act as a provider (store + advertise) or client (download).
    #[arg(long, value_enum, default_value_t = Role::Provider)]
    role: Role,
}

#[derive(NetworkBehaviour)]
struct NodeBehaviour {
    kademlia: kad::Behaviour<MemoryStore>,
    ping: ping::Behaviour,
}

impl NodeBehaviour {
    fn new(local_peer_id: PeerId, role: Role) -> Self {
        let cfg = kad::Config::new(kad::PROTOCOL_NAME);
        let store = MemoryStore::new(local_peer_id);
        let mut kademlia = kad::Behaviour::with_config(local_peer_id, store, cfg);
        let mode = match role {
            Role::Provider => Mode::Server,
            Role::Client => Mode::Client,
        };
        kademlia.set_mode(Some(mode));

        Self {
            kademlia,
            ping: ping::Behaviour::default(),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let listen_addr: Multiaddr = args
        .listen
        .parse()
        .with_context(|| format!("invalid listen address {}", args.listen))?;

    let (bootstrap_peer_id, bootstrap_addr) = parse_bootstrap_addr(&args.bootstrap)?;

    let mut swarm = SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            tcp::Config::default().nodelay(true),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_behaviour(|key| NodeBehaviour::new(PeerId::from(key.public()), args.role))?
        .build();

    let local_peer_id = *swarm.local_peer_id();
    println!("Local peer id: {local_peer_id}");

    swarm
        .listen_on(listen_addr)
        .context("failed to start listening on requested address")?;

    swarm
        .behaviour_mut()
        .kademlia
        .add_address(&bootstrap_peer_id, bootstrap_addr.clone());

    // Dial the bootstrap node to speed up routing table population.
    let mut full_bootstrap_addr = bootstrap_addr.clone();
    full_bootstrap_addr.push(Protocol::P2p(bootstrap_peer_id.into()));
    if let Err(err) = swarm.dial(full_bootstrap_addr.clone()) {
        eprintln!("Dial to bootstrap peer failed: {err}");
    } else {
        println!("Dialing bootstrap peer at {full_bootstrap_addr}");
    }

    swarm
        .behaviour_mut()
        .kademlia
        .bootstrap()
        .context("failed to start bootstrap query")?;

    let record_key = RecordKey::new(&FILE_TO_PROVIDE.as_bytes().to_vec());
    if let Role::Provider = args.role {
        let data = fs::read(FILE_TO_PROVIDE)
            .with_context(|| format!("failed to read {FILE_TO_PROVIDE}"))?;

        let record = Record::new(record_key.clone(), data);
        swarm
            .behaviour_mut()
            .kademlia
            .start_providing(record_key.clone())?;

        swarm
            .behaviour_mut()
            .kademlia
            .put_record(record, kad::Quorum::One)?;

        println!("Advertising provider record and storing value for: {FILE_TO_PROVIDE}");
    } else {
        println!("Client mode: searching for providers and record for key {FILE_TO_PROVIDE}");
        let _ = swarm
            .behaviour_mut()
            .kademlia
            .get_providers(record_key.clone());
        let _ = swarm
            .behaviour_mut()
            .kademlia
            .get_record(record_key.clone());
    }

    let mut downloaded_bytes: Option<Vec<u8>> = None;
    loop {
        tokio::select! {
            _ = signal::ctrl_c() => {
                println!("Shutting down (Ctrl+C)");
                break;
            }
            event = swarm.select_next_some() => {
                if handle_event(event, &record_key, &args, &mut downloaded_bytes)? {
                    break;
                }
            },
        }
    }

    Ok(())
}

fn parse_bootstrap_addr(input: &str) -> Result<(PeerId, Multiaddr)> {
    let mut addr: Multiaddr = input
        .parse()
        .context("bootstrap address is not a valid multiaddr")?;
    let Some(Protocol::P2p(mh)) = addr.pop() else {
        bail!("bootstrap address must end with /p2p/<peer_id>");
    };
    let peer_id =
        PeerId::try_from(mh).context("bootstrap address contains an invalid peer id multihash")?;
    Ok((peer_id, addr))
}

fn handle_event(
    event: SwarmEvent<NodeBehaviourEvent>,
    record_key: &RecordKey,
    args: &Args,
    downloaded_bytes: &mut Option<Vec<u8>>,
) -> Result<bool> {
    match event {
        SwarmEvent::NewListenAddr { address, .. } => {
            println!("Listening on {address}");
        }
        SwarmEvent::Behaviour(NodeBehaviourEvent::Kademlia(kad_event)) => match kad_event {
            kad::Event::OutboundQueryProgressed { result, .. } => match result {
                kad::QueryResult::Bootstrap(Ok(ok)) => {
                    println!(
                        "Bootstrap finished with peer {} ({} remaining)",
                        ok.peer, ok.num_remaining
                    );
                }
                kad::QueryResult::Bootstrap(Err(err)) => {
                    eprintln!("Bootstrap failed: {err}");
                }
                kad::QueryResult::StartProviding(res) => match res {
                    Ok(info) => println!(
                        "Providing record for key: {}",
                        String::from_utf8_lossy(info.key.as_ref())
                    ),
                    Err(err) => eprintln!("Failed to announce provider record: {err}"),
                },
                kad::QueryResult::PutRecord(Ok(ok)) => {
                    println!(
                        "Successfully stored record for key {}",
                        String::from_utf8_lossy(ok.key.as_ref())
                    );
                }
                kad::QueryResult::PutRecord(Err(err)) => {
                    eprintln!("Failed to put record: {err}");
                }
                kad::QueryResult::GetProviders(res) => match res {
                    Ok(kad::GetProvidersOk::FoundProviders { key, providers }) => {
                        println!(
                            "Providers discovered for {}: {:?}",
                            String::from_utf8_lossy(key.as_ref()),
                            providers
                        );
                    }
                    Ok(kad::GetProvidersOk::FinishedWithNoAdditionalRecord { .. }) => {}
                    Err(err) => eprintln!("GetProviders query failed: {err}"),
                },
                kad::QueryResult::GetRecord(Ok(kad::GetRecordOk::FoundRecord(PeerRecord {
                    record,
                    ..
                }))) => {
                    if record.key == *record_key {
                        println!(
                            "Received record for {} ({} bytes)",
                            String::from_utf8_lossy(record.key.as_ref()),
                            record.value.len()
                        );
                        *downloaded_bytes = Some(record.value);
                    }
                }
                kad::QueryResult::GetRecord(Ok(
                    kad::GetRecordOk::FinishedWithNoAdditionalRecord { .. },
                )) => {}
                kad::QueryResult::GetRecord(Err(err)) => {
                    eprintln!("GetRecord query failed: {err}");
                }
                _ => {}
            },
            kad::Event::RoutingUpdated {
                peer, addresses, ..
            } => {
                println!(
                    "Routing table updated with peer {peer}, addresses: {:?}",
                    addresses
                );
            }
            _ => {}
        },
        SwarmEvent::Behaviour(NodeBehaviourEvent::Ping(ping::Event { peer, result, .. })) => {
            if let Err(err) = result {
                eprintln!("Ping to {peer} failed: {err}");
            }
        }
        SwarmEvent::ConnectionEstablished { peer_id, .. } => {
            println!("Connected to {peer_id}");
        }
        SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
            eprintln!("Failed to connect to {:?}: {error}", peer_id);
        }
        _ => {}
    }

    let output = "./output.txt";
    if let Some(bytes) = downloaded_bytes.take() {
        if let Role::Client = args.role {
            fs::write(&output, &bytes)
                .with_context(|| format!("failed to write downloaded data to {}", output))?;
            println!(
                "Downloaded {} bytes for {} to {}. Shutting down.",
                bytes.len(),
                FILE_TO_PROVIDE,
                output
            );
            return Ok(true);
        }
    }

    Ok(false)
}
