mod fixer;
mod v2_to_v3;

use self::fixer::FixInfo;
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use mchprs_blocks::block_entities::BlockEntity;
use mchprs_blocks::BlockPos;
use mchprs_world::storage::{Chunk, ChunkSection};
use mchprs_world::TickEntry;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::{fmt, io};
use thiserror::Error;

/// Version History:
/// 0: Initial plot data file with header (MC 1.18.2)
/// 1: Add world send rate
/// 2: Update to MC 1.20.4
/// 3: Change Tps and WorldSendRate to support real values (f32)
pub const VERSION: u32 = 3;

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

    #[error("conversion from plot data version {0} is unavailable")]
    ConversionUnavailable(u32),
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
    pub data: Vec<u64>,
    pub palette: Vec<u32>,
    pub bits_per_block: u8,
    pub block_count: u32,
}

impl ChunkSectionData {
    fn new(section: &ChunkSection) -> Self {
        Self {
            data: section.data().to_vec(),
            palette: section.palette().to_vec(),
            bits_per_block: section.bits_per_block(),
            block_count: section.block_count(),
        }
    }

    fn load(self) -> ChunkSection {
        ChunkSection::from_raw(
            self.data,
            self.bits_per_block,
            self.palette,
            self.block_count,
        )
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChunkData {
    pub sections: Vec<Option<ChunkSectionData>>,
    pub block_entities: FxHashMap<BlockPos, BlockEntity>,
}

impl ChunkData {
    /// Takes a mutable Chunk to flush it first
    pub fn new(chunk: &mut Chunk) -> Self {
        chunk.flush();
        Self {
            sections: chunk
                .sections
                .iter()
                .map(|section| {
                    if section.block_count() > 0 {
                        Some(ChunkSectionData::new(section))
                    } else {
                        None
                    }
                })
                .collect(),
            block_entities: chunk.block_entities.clone(),
        }
    }

    pub fn load(self, x: i32, z: i32) -> Chunk {
        Chunk {
            x,
            z,
            sections: self
                .sections
                .into_iter()
                .map(|section| match section {
                    Some(section) => section.load(),
                    None => Default::default(),
                })
                .collect(),
            block_entities: self.block_entities,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum Tps {
    Limited(f32),
    Unlimited,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct WorldSendRate(pub f32);

impl Default for WorldSendRate {
    fn default() -> Self {
        Self(60.0)
    }
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
    pub world_send_rate: WorldSendRate,
    pub chunk_data: Vec<ChunkData>,
    pub pending_ticks: Vec<TickEntry>,
}

impl PlotData {
    pub fn load_from_file(path: impl AsRef<Path>) -> Result<PlotData, PlotLoadError> {
        let mut file = File::open(&path)?;

        let mut magic = [0; 8];
        file.read_exact(&mut magic)?;
        if &magic != PLOT_MAGIC {
            return fixer::try_fix(path, FixInfo::InvalidHeader)?
                .ok_or(PlotLoadError::InvalidHeader);
        }

        let version = file.read_u32::<LittleEndian>()?;
        if version < VERSION {
            return fixer::try_fix(path, FixInfo::OldVersion { version })?
                .ok_or(PlotLoadError::ConversionFailed(version));
        }
        if version > VERSION {
            return Err(PlotLoadError::TooNew(version));
        }

        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;
        Ok(bincode::deserialize(&buf)?)
    }

    pub fn save_to_file(&self, path: impl AsRef<Path>) -> Result<(), PlotSaveError> {
        let mut file = File::create(path)?;

        file.write_all(PLOT_MAGIC)?;
        file.write_u32::<LittleEndian>(VERSION)?;
        let data = bincode::serialize(self)?;
        file.write_all(&data)?;
        file.sync_data()?;
        Ok(())
    }
}
