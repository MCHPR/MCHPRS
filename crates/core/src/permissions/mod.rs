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
    value: i32,
    server_context: String,
}

impl PermissionNode {
    fn matches(&self, str: &str) -> bool {
        if self.server_context != "global" && self.server_context != config().server_context {
            return false;
        }

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

#[derive(Debug)]
pub struct PlayerPermissionsCache {
    nodes: Vec<PermissionNode>,
}

impl PlayerPermissionsCache {
    pub fn get_node_val(&self, name: &str) -> Option<i32> {
        for node in &self.nodes {
            if node.matches(name) {
                return Some(node.value);
            }
        }
        None
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

pub fn load_player_cache(uuid: u128) -> Result<PlayerPermissionsCache> {
    let uuid = HyphenatedUUID(uuid).to_string();
    let mut conn = conn()?;
    let res: Vec<Row> = conn.exec(
        "
        WITH RECURSIVE groups_inherited AS (
            SELECT *
            FROM luckperms_user_permissions
            WHERE uuid LIKE ?
            UNION
            SELECT luckperms_group_permissions.*
            FROM groups_inherited, luckperms_group_permissions
            WHERE luckperms_group_permissions.name = SUBSTR(groups_inherited.permission, 7)
        )
        SELECT *
        FROM groups_inherited;
    ",
        (&uuid,),
    )?;

    let mut nodes = Vec::new();
    for row in res {
        let path_str = String::from_value(row[2].clone());
        let path = path_str
            .split('.')
            .map(|s| match s {
                "*" => PathSegment::WildCard,
                s => PathSegment::Named(s.to_owned()),
            })
            .collect();
        let node = PermissionNode {
            path,
            server_context: FromValue::from_value(row[4].clone()),
            value: FromValue::from_value(row[3].clone()),
        };
        nodes.push(node);
    }

    Ok(PlayerPermissionsCache { nodes })
}
