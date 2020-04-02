use crate::network::NetworkServer;
/// This represents a minecraft server
pub struct MinecraftServer {
    network: NetworkServer,
    config: config::Config,
}

impl MinecraftServer {
    pub fn run() {
        println!("Starting server...");
        let mut config = config::Config::default();
        config
            .merge(config::File::with_name("Config"))
            .expect("Error reading config file!");
        let bind_addr = config
            .get_str("bind_address")
            .expect("Bind address not found in config file!");
        MinecraftServer {
            network: NetworkServer::new(bind_addr),
            config,
        };
        loop {
            // temporary
            std::thread::sleep_ms(2);
        }
    }
}
