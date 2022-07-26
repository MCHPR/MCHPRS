use mchprs_blocks::BlockPos;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TickPriority {
    Highest,
    Higher,
    High,
    Normal,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TickEntry {
    pub ticks_left: u32,
    pub tick_priority: TickPriority,
    pub pos: BlockPos,
}
