pub mod packets;

use crate::server::MinecraftServer;
use packets::{PacketDecoder, PacketEncoder};
use std::io::{self, BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::thread;

#[derive(PartialEq, Clone)]
pub enum NetworkState {
    Handshake,
    Status,
    Login,
    Play,
}

/// This struct represents a TCP Client
pub struct NetworkClient {
    /// All NetworkClients are identified by this id
    pub id: u32,
    reader: BufReader<TcpStream>,
    stream: TcpStream,
    pub state: NetworkState,
    pub packets: Vec<PacketDecoder>,
    pub username: Option<String>,
    pub alive: bool,
    pub compressed: bool,
}

impl NetworkClient {
    pub fn update(&mut self) {
        if !self.alive {
            return;
        };
        let mut would_block = false;
        let incoming_data = Vec::from(match self.reader.fill_buf() {
            Ok(data) => data,
            Err(e) => match e.kind() {
                io::ErrorKind::WouldBlock => {
                    would_block = true;
                    &[]
                }
                _ => {
                    self.alive = false;
                    return;
                }
            },
        });
        let data_length = incoming_data.len();
        let mut incoming_packets = PacketDecoder::decode(false, incoming_data);
        if !incoming_packets.is_empty() {
            self.packets.append(&mut incoming_packets);
        }
        self.reader.consume(data_length);
        if !would_block && data_length == 0 {
            self.alive = false;
        }
    }

    pub fn send_packet(&mut self, data: PacketEncoder) {
        if self.compressed {
            self.stream.write_all(&data.compressed());
        } else {
            self.stream.write_all(&data.uncompressed());
        }
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
            stream.set_nonblocking(true).unwrap();
            sender
                .send(NetworkClient {
                    // The index will increment after each client making it unique. We'll just use this as the id.
                    id: index as u32,
                    reader: BufReader::new(stream.try_clone().unwrap()),
                    stream,
                    state: NetworkState::Handshake,
                    packets: Vec::new(),
                    username: None,
                    alive: true,
                    compressed: false,
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
        for client in self.handshaking_clients.iter_mut() {
            client.update();
        }
    }
}
