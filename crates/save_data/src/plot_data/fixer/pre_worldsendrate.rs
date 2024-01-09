use crate::plot_data::{ChunkData, PlotData, Tps, WorldSendRate};
use mchprs_world::TickEntry;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct PreSendratePlotData<const NUM_CHUNK_SECTIONS: usize> {
    pub tps: Tps,
    pub chunk_data: Vec<ChunkData<NUM_CHUNK_SECTIONS>>,
    pub pending_ticks: Vec<TickEntry>,
}

pub fn try_fix<const NUM_SECTIONS: usize>(data: &[u8]) -> Option<PlotData<NUM_SECTIONS>> {
    // Skip magic and version header
    let data = &data[12..data.len()];
    let old_data: PreSendratePlotData<NUM_SECTIONS> = bincode::deserialize(data).ok()?;

    let data = PlotData {
        tps: old_data.tps,
        world_send_rate: WorldSendRate::default(),
        chunk_data: old_data.chunk_data,
        pending_ticks: old_data.pending_ticks,
    };
    Some(data)
}
