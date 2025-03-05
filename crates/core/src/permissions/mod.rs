use crate::config::CONFIG;
use crate::utils::HyphenatedUUID;
use anyhow::{anyhow, Context, Result};
use mysql::prelude::*;
use mysql::{OptsBuilder, Pool, PooledConn, Row};
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};

static POOL: OnceCell<Pool> = OnceCell::new();

fn conn() -> Result<PooledConn> {
    Ok(POOL
        .get()
        .context("Tried to get conn before permissions init")?
        .get_conn()?)
}

fn config() -> &'static PermissionsConfig {
    CONFIG.luckperms.as_ref().unwrap()
}

#[derive(Debug)]
enum PathSegment {
    WildCard,
    Named(String),
}

#[derive(Debug)]
struct PermissionNode {
    path: Vec<PathSegment>,
    value: bool,
}

impl PermissionNode {
    fn matches(&self, str: &str) -> bool {
        for (i, segment) in str.split('.').enumerate() {
            match &self.path[i] {
                PathSegment::WildCard => return true,
                PathSegment::Named(name) => {
                    if name != segment {
                        return false;
                    }
                }
            }
        }
        true
    }
}

#[derive(Debug, Default)]
pub struct PlayerPermissionsCache {
    nodes: Vec<PermissionNode>,
}

impl PlayerPermissionsCache {
    pub fn get_node_val(&self, name: &str) -> Option<bool> {
        for node in &self.nodes {
            if node.matches(name) {
                return Some(node.value);
            }
        }
        None
    }

    fn insert(&mut self, name: &str, value: bool) {
        let path = name
            .split('.')
            .map(|s| match s {
                "*" => PathSegment::WildCard,
                s => PathSegment::Named(s.to_owned()),
            })
            .collect();
        self.nodes.push(PermissionNode { path, value });
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct PermissionsConfig {
    host: String,
    db_name: String,
    username: String,
    password: String,
    server_context: String,
}

pub fn init(config: PermissionsConfig) -> Result<()> {
    let opts = OptsBuilder::new()
        .ip_or_hostname(Some(config.host))
        .db_name(Some(config.db_name))
        .user(Some(config.username))
        .pass(Some(config.password));
    let pool = Pool::new(opts)?;
    POOL.set(pool)
        .map_err(|_| anyhow!("Tried to init permissions more than once"))?;

    Ok(())
}

fn load_group(cache: &mut PlayerPermissionsCache, name: &str, server_context: &str) -> Result<()> {
    let mut conn = conn()?;

    let rows: Vec<Row> = conn.exec(
        r#"
            SELECT permission, value
            FROM luckperms_group_permissions
            WHERE name=? AND (server="global" OR server=?);
        "#,
        (&name, server_context),
    )?;
    for row in rows {
        let path_str = String::from_value(row[0].clone());
        let value = FromValue::from_value(row[1].clone());
        cache.insert(&path_str, value);

        if let Some(group_name) = path_str.strip_prefix("group.") {
            load_group(cache, group_name, server_context)?;
        }
    }
    Ok(())
}

pub fn load_player_cache(uuid: u128, config: &PermissionsConfig) -> Result<PlayerPermissionsCache> {
    let uuid = HyphenatedUUID(uuid).to_string();
    let mut conn = conn()?;

    let mut cache: PlayerPermissionsCache = Default::default();

    let user_rows: Vec<Row> = conn.exec(
        r#"
            SELECT permission, value
            FROM luckperms_user_permissions
            WHERE uuid=? AND (server="global" OR server=?);
        "#,
        (&uuid, &config.server_context),
    )?;
    for row in user_rows {
        let path_str = String::from_value(row[0].clone());
        let value = FromValue::from_value(row[1].clone());
        cache.insert(&path_str, value);

        if let Some(group_name) = path_str.strip_prefix("group.") {
            load_group(&mut cache, group_name, &config.server_context)?;
        }
    }

    Ok(cache)
}
