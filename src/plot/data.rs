use super::{Plot, PlotWorld, PLOT_WIDTH};
use crate::world::storage::ChunkData;
use crate::world::TickEntry;
use serde::{Deserialize, Serialize};
use std::fs;
use std::lazy::SyncLazy;
use std::path::Path;

static EMPTY_PLOT: SyncLazy<PlotData> = SyncLazy::new(|| {
    let template_path = Path::new("./world/plots/pTEMPLATE");
    if template_path.exists() {
        PlotData::read_from_file(template_path)
    } else {
        let mut chunks = Vec::new();
        for chunk_x in 0..PLOT_WIDTH {
            for chunk_z in 0..PLOT_WIDTH {
                chunks.push(Plot::generate_chunk(8, chunk_x, chunk_z));
            }
        }
        let mut world = PlotWorld {
            x: 0,
            z: 0,
            chunks,
            to_be_ticked: Vec::new(),
            packet_senders: Vec::new(),
        };
        let chunk_data: Vec<ChunkData> = world.chunks.iter_mut().map(|c| c.save()).collect();
        PlotData {
            tps: 10,
            show_redstone: true,
            chunk_data,
            pending_ticks: Vec::new(),
        }
    }
});

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PlotData {
    pub tps: u32,
    pub show_redstone: bool,
    pub chunk_data: Vec<ChunkData>,
    pub pending_ticks: Vec<TickEntry>,
}

impl PlotData {
    pub fn read_from_file(path: impl AsRef<Path>) -> PlotData {
        let data = fs::read(path).unwrap();
        bincode::deserialize(&data).unwrap()
    }
}

impl Default for PlotData {
    fn default() -> PlotData {
        EMPTY_PLOT.clone()
    }
}
