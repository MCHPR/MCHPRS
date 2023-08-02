use super::{Plot, PlotWorld, PLOT_SECTIONS, PLOT_WIDTH};
use anyhow::{Context, Result};
use mchprs_save_data::plot_data::{ChunkData, PlotData, Tps};
use once_cell::sync::Lazy;
use std::path::Path;
use std::time::Duration;

// TODO: where to put this?
pub fn sleep_time_for_tps(tps: Tps) -> Duration {
    match tps {
        Tps::Limited(tps) => {
            if tps > 10 {
                Duration::from_micros(1_000_000 / tps as u64)
            } else {
                Duration::from_millis(50)
            }
        }
        Tps::Unlimited => Duration::ZERO,
    }
}

pub fn load_plot(path: impl AsRef<Path>) -> Result<PlotData<PLOT_SECTIONS>> {
    let path = path.as_ref();
    if path.exists() {
        Ok(PlotData::load_from_file(path)
            .with_context(|| format!("error loading plot save file at {}", path.display()))?)
    } else {
        Ok(EMPTY_PLOT.clone())
    }
}

pub fn empty_plot() -> PlotData<PLOT_SECTIONS> {
    EMPTY_PLOT.clone()
}

static EMPTY_PLOT: Lazy<PlotData<PLOT_SECTIONS>> = Lazy::new(|| {
    let template_path = Path::new("./world/plots/pTEMPLATE");
    if template_path.exists() {
        PlotData::load_from_file(template_path).expect("failed to read template plot")
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
        let chunk_data: Vec<ChunkData<PLOT_SECTIONS>> =
            world.chunks.iter_mut().map(|c| c.save()).collect();
        PlotData {
            tps: Tps::Limited(10),
            chunk_data,
            pending_ticks: Vec::new(),
        }
    }
});
