use crate::permissions::PermissionsConfig;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use toml_edit::{value, DocumentMut};

pub static CONFIG: Lazy<ServerConfig> = Lazy::new(|| ServerConfig::load("Config.toml"));

trait ConfigSerializeDefault {
    fn fix_config(self, name: &str, doc: &mut DocumentMut);
}

macro_rules! impl_simple_default {
    ( $( $type:ty ),* ) => {
        $(
            impl ConfigSerializeDefault for $type {
                fn fix_config(self, name: &str, doc: &mut DocumentMut) {
                    doc.entry(name).or_insert_with(|| value(self));
                }
            }
        )*
    }
}

impl_simple_default!(String, i64, bool);

impl<T> ConfigSerializeDefault for Option<T> {
    fn fix_config(self, _: &str, _: &mut DocumentMut) {
        assert!(matches!(self, None), "`Some` as default is unimplemented");
    }
}

macro_rules! gen_config {
    (
        $( $name:ident: $type:ty = $default:expr),*
    ) => {
        #[derive(Serialize, Deserialize)]
        pub struct ServerConfig {
            $(
                pub $name: $type,
            )*
        }

        impl ServerConfig {
            fn load(config_file: &str) -> ServerConfig {
                let str = fs::read_to_string("Config.toml").unwrap_or_default();
                let mut doc = str.parse::<DocumentMut>().unwrap();

                $(
                    <$type as ConfigSerializeDefault>::fix_config($default, stringify!($name), &mut doc);
                )*

                let patched = doc.to_string();
                if str != patched {
                    let mut file = fs::OpenOptions::new().create(true).write(true).open(&config_file).unwrap();
                    write!(file, "{}", patched).unwrap();
                }

                toml::from_str(&patched).unwrap()
            }
        }
    };
}

gen_config! {
    bind_address: String = "0.0.0.0:25565".to_string(),
    motd: String = "Minecraft High Performance Redstone Server".to_string(),
    chat_format: String = "<{username}> {message}".to_string(),
    max_players: i64 = 99999,
    view_distance: i64 = 8,
    whitelist: bool = false,
    schemati: bool = false,
    luckperms: Option<PermissionsConfig> = None,
    block_in_hitbox: bool = true,
    auto_redpiler: bool = false,
    velocity: Option<VelocityConfig> = None
}

#[derive(Serialize, Deserialize)]
pub struct VelocityConfig {
    pub enabled: bool,
    pub secret: String,
}
