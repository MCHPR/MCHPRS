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

pub fn try_fix(path: impl AsRef<Path>, info: FixInfo) -> Result<Option<PlotData>, PlotLoadError> {
    debug!("Trying to fix plot with {:?}", info);
    let result: Option<PlotData> = match info {
        FixInfo::OldVersion {
            version: version @ 0..=1,
        } => return Err(PlotLoadError::ConversionUnavailable(version)),
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
