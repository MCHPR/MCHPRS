//! If the mchprs plot header is missing, it's possible it can still be a
//! plot save file. For mchprs versions targetting 1.17.1 and below, we did
//! not have a file header.

use crate::plot_data::{ChunkData, ChunkSectionData, PlotData, Tps};
use mchprs_blocks::block_entities::BlockEntity;
use mchprs_blocks::BlockPos;
use mchprs_world::TickEntry;
use rustc_hash::FxHashMap;
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
pub struct PreHeaderChunkData {
    sections: BTreeMap<u8, ChunkSectionData>,
    block_entities: FxHashMap<BlockPos, BlockEntity>,
}

#[derive(Deserialize)]
pub struct PreHeaderPlotData {
    pub tps: u32,
    pub show_redstone: bool,
    pub chunk_data: Vec<PreHeaderChunkData>,
    pub pending_ticks: Vec<TickEntry>,
}

pub fn try_fix<const NUM_SECTIONS: usize>(data: &[u8]) -> Option<PlotData<NUM_SECTIONS>> {
    let old_data: PreHeaderPlotData = bincode::deserialize(data).ok()?;

    let data = PlotData {
        tps: match old_data.tps {
            u32::MAX => Tps::Unlimited,
            limit => Tps::Limited(limit),
        },
        chunk_data: old_data
            .chunk_data
            .into_iter()
            .map(|chunk| {
                const INIT: Option<ChunkSectionData> = None;
                let mut sections = [INIT; NUM_SECTIONS];
                for (y, section) in chunk.sections.into_iter() {
                    if (y as usize) < sections.len() {
                        sections[y as usize] = Some(section);
                    }
                }
                ChunkData {
                    sections,
                    block_entities: chunk.block_entities,
                }
            })
            .collect(),
        pending_ticks: old_data.pending_ticks,
    };
    Some(data)
}
