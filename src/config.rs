use serde::{Serialize, Deserialize};
use std::fs;
use lazy_static::lazy_static;

lazy_static! {
    pub static ref CONFIG: ServerConfig = load_config();
}

macro_rules! gen_config {
    (
        $( $name:ident: $type:ty = $default:expr),*
    ) => {
        #[derive(Serialize)]
        pub struct ServerConfig {
            $(
                pub $name: $type,
            )*
        }
        
        #[derive(Deserialize)]
        pub struct UnmergedConfig {
            $(
                $name: Option<$type>,
            )*
        }

        fn default_config() -> ServerConfig {
            ServerConfig {
                $(
                    $name: $default,
                )*
            }
        }

        fn merge_config(config: UnmergedConfig) -> ServerConfig {
            let default_config = default_config();
            ServerConfig {
                $(
                    $name: match config.$name {
                        Some(entry) => entry,
                        None => default_config.$name
                    },
                )*
            }
            
        }
    };
}

gen_config! {
    bind_address: String = "0.0.0.0:25565".to_string(),
    motd: String = "Minecraft High Performace Redstone Server".to_string(),
    chat_format: String = "<{username}> {message}".to_string(),
    max_players: i64 = 99999,
    bungeecord: bool = false
}

fn write_config(config: &ServerConfig) {
    let config_string = toml::to_string(config).unwrap();
    let _ = fs::write("Config.toml", &config_string);
}

fn load_config() -> ServerConfig {
    let config = if let Ok(str) = fs::read_to_string("Config.toml") {
        toml::from_str(&str).map(merge_config).unwrap_or_else(|_| default_config())
    } else {
        default_config()
    };
    write_config(&config);
    config
}