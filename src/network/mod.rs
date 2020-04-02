mod packets;

use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::thread;

/// This struct represents a TCP Client
struct NetworkClient {
    /// All NetworkClients are identified by this id
    id: u32
}

/// This represents the network portion of a minecraft server
pub struct NetworkServer {
    client_receiver: mpsc::Receiver<NetworkClient>
}

impl NetworkServer {

    fn listen(bind_address: &str, sender: mpsc::Sender<NetworkClient>) {
        let listener = TcpListener::bind(bind_address).unwrap();

        for (index, stream) in listener.incoming().enumerate() {
            sender.send(NetworkClient {
                // The index will increment after each client making it unique. We'll just use this as the id.
                id: index as u32
            }).unwrap();
        }
    }

    /// Creates a new NetworkServer. The server will then start accepting TCP clients.
    fn new<'a>(bind_address: String) -> NetworkServer {
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            NetworkServer::listen(&bind_address, sender)
        });
        NetworkServer {
            client_receiver: receiver
        }
    }

}