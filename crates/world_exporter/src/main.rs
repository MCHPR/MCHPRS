mod level;
mod regions;

use anyhow::Result;
use chrono::{DateTime, Utc};
use clap::Parser;
use std::fs;
use std::path::{Path, PathBuf};

pub const MC_VERSION: &str = "1.20.4";
pub const MC_DATA_VERSION: i32 = 3700;

/// MCHPRS world export tool
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to MCHPRS world directory.
    world_path: PathBuf,

    /// Path to minecraft saves directory. A new save will be created.
    output_path: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let now_utc: DateTime<Utc> = Utc::now();
    let level_name = format!("MCHPRS World Export {}", now_utc);
    let output_path = Path::new(&args.output_path).join(&level_name);
    fs::create_dir(&output_path)?;

    level::write_level_dat(&level_name, &output_path)?;
    regions::generate_regions(&args.world_path, &output_path)?;

    Ok(())
}
