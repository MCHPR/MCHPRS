//! The goal of this module is to make upgrading to newer version of mchprs
//! easier by providing automatic conversion from old world data.
//! 
//! Eventually this module might help recover currupted plot data.
//! 
//! In the future it might be nice to have this as an optional dependency or
//! seperate download. As our save format changes in the future, the fixer
//! module may become quite big.

mod pre_header;

pub enum FixInfo {
    InvalidHeader,
    OldVersion(u32),
}

fn try_fix(path: impl AsRef<Path>) -> Option<PlotData> {

}
