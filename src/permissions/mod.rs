use anyhow::{anyhow, Result};
use mysql::prelude::*;
use mysql::{OptsBuilder, Pool};
use serde::{Deserialize, Serialize};
use std::lazy::SyncOnceCell;

static POOL: SyncOnceCell<Pool> = SyncOnceCell::new();

#[derive(Serialize, Deserialize)]
pub struct PermissionsConfig {
    host: String,
    db_name: String,
    username: String,
    password: String,
    server_context: String,
}

fn init(config: PermissionsConfig) -> Result<()> {
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
