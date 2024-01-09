//! The goal of this module is to make upgrading to newer version of mchprs
//! easier by providing automatic conversion from old world data.
//!
//! Eventually this module might help recover currupted plot data.
//!
//! In the future it might be nice to have this as an optional dependency or
//! seperate download. As our save format changes in the future, the fixer
//! module may become quite big.

use super::{PlotData, PlotLoadError};
use crate::plot_data::VERSION;
use std::fs;
use std::path::Path;
use tracing::debug;

mod pre_header;
mod pre_worldsendrate;

#[derive(Debug)]
pub enum FixInfo {
    InvalidHeader,
    OldVersion { version: u32 },
}

fn make_backup(path: impl AsRef<Path>) -> Result<(), PlotLoadError> {
    let path = path.as_ref();
    let mut backup_path = path.with_extension("bak");
    if backup_path.exists() {
        let num = 1;
        loop {
            backup_path = path.with_extension(format!("bak.{}", num));
            if !backup_path.exists() {
                break;
            }
        }
    }
    fs::rename(path, backup_path)?;
    Ok(())
}

pub fn try_fix<const NUM_SECTIONS: usize>(
    path: impl AsRef<Path>,
    info: FixInfo,
) -> Result<Option<PlotData<NUM_SECTIONS>>, PlotLoadError> {
    debug!("Trying to fix plot with {:?}", info);
    let result = match info {
        FixInfo::InvalidHeader => {
            let data = fs::read(&path)?;
            pre_header::try_fix(&data)
        }
        FixInfo::OldVersion { version: 0 } => {
            let data = fs::read(&path)?;
            pre_worldsendrate::try_fix(&data)
        }
        _ => None,
    };

    Ok(match result {
        Some(data) => {
            make_backup(&path)?;
            data.save_to_file(&path)?;
            debug!("Successfully converted plot to version {}", VERSION);
            Some(data)
        }
        None => None,
    })
}
