//! Migration from version 2 to version 3
//!
//! Version 2: Tps and WorldSendRate used u32 values
//! Version 3: Tps and WorldSendRate use f32 values

use super::{ChunkData, PlotData, PlotLoadError, Tps, WorldSendRate, PLOT_MAGIC};
use byteorder::{LittleEndian, ReadBytesExt};
use mchprs_world::TickEntry;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use tracing::debug;

// Version 2 data structures (with u32 values)
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum TpsV2 {
    Limited(u32),
    Unlimited,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldSendRateV2(pub u32);

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PlotDataV2 {
    pub tps: TpsV2,
    pub world_send_rate: WorldSendRateV2,
    pub chunk_data: Vec<ChunkData>,
    pub pending_ticks: Vec<TickEntry>,
}

pub fn convert_v2_to_v3(path: impl AsRef<Path>) -> Result<Option<PlotData>, PlotLoadError> {
    let mut file = File::open(&path)?;

    let mut magic = [0; 8];
    file.read_exact(&mut magic)?;
    if &magic != PLOT_MAGIC {
        return Ok(None);
    }

    let version = file.read_u32::<LittleEndian>()?;
    if version != 2 {
        return Ok(None);
    }

    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;
    let old_data: PlotDataV2 = bincode::deserialize(&buf)?;

    // Convert old data to new format
    let new_tps = match old_data.tps {
        TpsV2::Limited(val) => Tps::Limited(val as f32),
        TpsV2::Unlimited => Tps::Unlimited,
    };

    let new_world_send_rate = WorldSendRate(old_data.world_send_rate.0 as f32);

    let new_data = PlotData {
        tps: new_tps,
        world_send_rate: new_world_send_rate,
        chunk_data: old_data.chunk_data,
        pending_ticks: old_data.pending_ticks,
    };

    debug!("Successfully converted plot data from version 2 to version 3");
    Ok(Some(new_data))
}
