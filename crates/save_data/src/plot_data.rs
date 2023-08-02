mod fixer;

use self::fixer::FixInfo;
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use mchprs_blocks::block_entities::BlockEntity;
use mchprs_blocks::BlockPos;
use mchprs_world::TickEntry;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
use std::{fmt, io};
use thiserror::Error;

const VERSION: u32 = 0;

#[derive(Error, Debug)]
pub enum PlotLoadError {
    #[error("plot data deserialization error")]
    Deserialize(#[from] bincode::Error),

    #[error("invalid plot data header")]
    InvalidHeader,

    #[error("plot data version {0} too new to be loaded")]
    TooNew(u32),

    #[error("plot data version {0} failed to be converted")]
    ConversionFailed(u32),

    #[error(transparent)]
    Io(#[from] io::Error),
}

impl From<PlotSaveError> for PlotLoadError {
    fn from(e: PlotSaveError) -> Self {
        match e {
            PlotSaveError::Serialize(err) => err.into(),
            PlotSaveError::Io(err) => err.into(),
        }
    }
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
pub struct ChunkData<const NUM_SECTIONS: usize> {
    #[serde(with = "BigArray")]
    pub sections: [Option<ChunkSectionData>; NUM_SECTIONS],
    pub block_entities: FxHashMap<BlockPos, BlockEntity>,
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
pub struct PlotData<const NUM_CHUNK_SECTIONS: usize> {
    pub tps: Tps,
    pub chunk_data: Vec<ChunkData<NUM_CHUNK_SECTIONS>>,
    pub pending_ticks: Vec<TickEntry>,
}

impl<const NUM_CHUNK_SECTIONS: usize> PlotData<NUM_CHUNK_SECTIONS> {
    pub fn load_from_file(
        path: impl AsRef<Path>,
    ) -> Result<PlotData<NUM_CHUNK_SECTIONS>, PlotLoadError> {
        let mut file = File::open(&path)?;

        let mut magic = [0; 8];
        file.read_exact(&mut magic)?;
        if &magic != PLOT_MAGIC {
            return fixer::try_fix(path, FixInfo::InvalidHeader)?
                .ok_or(PlotLoadError::InvalidHeader);
        }

        let version = file.read_u32::<LittleEndian>()?;
        // if version < VERSION {
        //     return fixer::try_fix(path, FixInfo::OldVersion(version))?.ok_or(PlotLoadError::ConversionFailed(version));
        // }
        if version > VERSION {
            return Err(PlotLoadError::TooNew(version));
        }

        Ok(bincode::deserialize_from(file)?)
    }

    pub fn save_to_file(&self, path: impl AsRef<Path>) -> Result<(), PlotSaveError> {
        let mut file = OpenOptions::new().write(true).create(true).open(path)?;

        file.write_all(PLOT_MAGIC)?;
        file.write_u32::<LittleEndian>(VERSION)?;
        bincode::serialize_into(&file, self)?;
        file.sync_data()?;
        Ok(())
    }
}
