pub mod packets;

use packets::serverbound::ServerBoundPacket;
use packets::{read_packet, PacketEncoder};
use std::io::Write;
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;

/// The minecraft protocol has these 4 different states.
#[derive(PartialEq, Clone)]
pub enum NetworkState {
    Handshake,
    Status,
    Login,
    Play,
}

/// This handles the TCP stream.
pub struct NetworkClient {
    /// All NetworkClients are identified by this id.
    /// If the client is a player, the player's entitiy id becomes the same.
    pub id: u32,
    stream: TcpStream,
    pub state: NetworkState,
    packets: mpsc::Receiver<Box<dyn ServerBoundPacket>>,
    pub alive: bool,
    compressed: Arc<AtomicBool>,
    pub username: Option<String>,
    pub uuid: Option<u128>,
}

impl NetworkClient {
    fn listen(
        mut stream: TcpStream,
        sender: mpsc::Sender<Box<dyn ServerBoundPacket>>,
        compressed: Arc<AtomicBool>,
    ) {
        let mut state = NetworkState::Handshake;
        loop {
            let packet = match read_packet(&mut stream, &compressed, &mut state) {
                Ok(packet) => packet,
                // This will cause the client to disconnect
                Err(_) => return,
            };
            if sender.send(packet).is_err() {
                return;
            }
        }
    }

    pub fn receive_packets(&mut self) -> Vec<Box<dyn ServerBoundPacket>> {
        let mut packets = Vec::new();
        loop {
            let packet = self.packets.try_recv();
            match packet {
                Ok(packet) => packets.push(packet),
                Err(mpsc::TryRecvError::Empty) => break,
                _ => {
                    self.alive = false;
                    break;
                }
            }
        }
        packets
    }

    pub fn set_compressed(&mut self, compressed: bool) {
        self.compressed.store(compressed, Ordering::Relaxed);
    }

    pub fn send_packet(&mut self, data: &PacketEncoder) {
        if self.compressed.load(Ordering::Relaxed) {
            let _ = data.write_compressed(&self.stream);
        } else {
            let _ = data.write_uncompressed(&self.stream);
        }
    }

    pub fn close_connection(&mut self) {
        self.alive = false;
        let _ = self.stream.shutdown(Shutdown::Both);
    }
}

/// This represents the network portion of a minecraft server
pub struct NetworkServer {
    client_receiver: mpsc::Receiver<NetworkClient>,
    /// These clients are either in the handshake, login, or ping state, once they shift to play, they will be moved to a plot
    pub handshaking_clients: Vec<NetworkClient>,
}

impl NetworkServer {
    fn listen(bind_address: &str, sender: mpsc::Sender<NetworkClient>) {
        let listener = TcpListener::bind(bind_address).unwrap();

        for (index, stream) in listener.incoming().enumerate() {
            let stream = stream.unwrap();
            let (packet_sender, packet_receiver) = mpsc::channel();
            let compressed = Arc::new(AtomicBool::new(false));
            let client_stream = stream.try_clone().unwrap();
            let client_compressed = compressed.clone();
            thread::spawn(move || {
                NetworkClient::listen(client_stream, packet_sender, client_compressed)
            });
            sender
                .send(NetworkClient {
                    // The index will increment after each client making it unique. We'll just use this as the enitity id.
                    id: index as u32,
                    stream,
                    state: NetworkState::Handshake,
                    packets: packet_receiver,
                    alive: true,
                    compressed,
                    username: None,
                    uuid: None,
                })
                .unwrap();
        }
    }

    /// Creates a new NetworkServer. The server will then start accepting TCP clients.
    pub fn new(bind_address: String) -> NetworkServer {
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || NetworkServer::listen(&bind_address, sender));
        NetworkServer {
            client_receiver: receiver,
            handshaking_clients: Vec::new(),
        }
    }

    pub fn update(&mut self) {
        loop {
            match self.client_receiver.try_recv() {
                Ok(client) => self.handshaking_clients.push(client),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    panic!("Client receiver channel disconnected!")
                }
            }
        }
    }
}
