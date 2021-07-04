use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;

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

        fn merge_config(config_file: &str) -> Box<dyn Fn(UnmergedConfig) -> ServerConfig> {
            let config_file = String::from(config_file);
            Box::new(move |config: UnmergedConfig| -> ServerConfig {
                    let default_config = default_config();
                    let mut toml_patch = String::new();
                    let out = ServerConfig {
                        $(
                            $name: match config.$name {
                                Some(entry) => entry,
                                None => {
                                    toml_patch += &format!("{} = {}\n", stringify!($name), default_config.$name);
                                    default_config.$name
                                }
                            },
                        )*
                    };
                    if toml_patch.len() > 0 {
                        let mut file = fs::OpenOptions::new().append(true).open(&config_file).unwrap();
                        write!(file, "\n{}", toml_patch).unwrap();
                    }
                    out
            })
        }
    };
}

gen_config! {
    bind_address: String = "0.0.0.0:25565".to_string(),
    motd: String = "Minecraft High Performance Redstone Server".to_string(),
    chat_format: String = "<{username}> {message}".to_string(),
    max_players: i64 = 99999,
    bungeecord: bool = false,
    whitelist: bool = false
}

fn write_config(config: &ServerConfig) {
    let config_string = toml::to_string(config).unwrap();
    let _ = fs::write("Config.toml", &config_string);
}

fn load_config() -> ServerConfig {
    let config = if let Ok(str) = fs::read_to_string("Config.toml") {
        toml::from_str(&str)
            .map(merge_config("Config.toml"))
            .unwrap_or_else(|_| default_config())
    } else {
        write_config(&default_config());
        default_config()
    };
    config
}
