use mchprs_blocks::block_entities::BlockEntity;
use mchprs_blocks::BlockPos;
use mchprs_world::TickEntry;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
use std::{fmt, io};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PlotLoadError {
    #[error("plot data deserialization error")]
    Deserialize(#[from] bincode::Error),

    #[error("invalid plot data header")]
    InvalidHeader,

    #[error("plot data version {0} too new too be loaded")]
    TooNew(u32),

    #[error(transparent)]
    Io(#[from] io::Error),
}

#[derive(Error, Debug)]
pub enum PlotSaveError {
    #[error("plot data serialization error")]
    Serialize(#[from] bincode::Error),

    #[error(transparent)]
    Io(#[from] io::Error),
}

static PLOT_MAGIC: &[u8; 8] = b"\x86MCHPRS\x00";

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct ChunkSectionData {
    pub data: Vec<i64>,
    pub palette: Vec<i32>,
    pub bits_per_block: i8,
    pub block_count: i32,
    pub entries: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChunkData {
    pub sections: [ChunkSectionData; 16],
    pub block_entities: HashMap<BlockPos, BlockEntity>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tps {
    Limited(u32),
    Unlimited,
}

impl fmt::Display for Tps {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Tps::Limited(tps) => write!(f, "{}", tps),
            Tps::Unlimited => write!(f, "unlimited"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PlotData {
    pub tps: Tps,
    pub show_redstone: bool,
    pub chunk_data: Vec<ChunkData>,
    pub pending_ticks: Vec<TickEntry>,
}

impl PlotData {
    pub fn load_from_file(path: impl AsRef<Path>) -> Result<PlotData, PlotLoadError> {
        let mut file = File::open(path)?;

        let mut magic = [0; 8];
        file.read_exact(&mut magic)?;
        if &magic != PLOT_MAGIC {
            // TODO: convert plot data
            return Err(PlotLoadError::InvalidHeader);
        }

        Ok(bincode::deserialize_from(file)?)
    }

    pub fn save_to_file(&self, path: impl AsRef<Path>) -> Result<(), PlotSaveError> {
        let mut file = OpenOptions::new().write(true).create(true).open(path)?;

        file.write_all(PLOT_MAGIC)?;
        bincode::serialize_into(&file, self)?;
        file.sync_data()?;
        Ok(())
    }
}
