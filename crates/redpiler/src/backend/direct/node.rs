use std::{
    num::NonZeroU128,
    ops::{Index, IndexMut},
};

use mchprs_blocks::blocks::ComparatorMode;

use crate::compile_graph::SignalStrength;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct NodeId(u32);

impl NodeId {
    pub fn index(self) -> usize {
        self.0 as usize
    }

    /// Safety: index must be within bounds of nodes array
    pub unsafe fn from_index(index: usize) -> NodeId {
        NodeId(index as u32)
    }
}

// This is Pretty Bad:tm: because one can create a NodeId using another instance of Nodes,
// but at least some type system protection is better than none.
#[derive(Default)]
pub struct Nodes {
    pub nodes: Box<[Node]>,
}

impl Nodes {
    pub fn new(nodes: Box<[Node]>) -> Nodes {
        Nodes { nodes }
    }

    pub fn get(&self, idx: usize) -> NodeId {
        if self.nodes.get(idx).is_some() {
            NodeId(idx as u32)
        } else {
            panic!("node index out of bounds: {}", idx)
        }
    }

    pub fn inner(&self) -> &[Node] {
        &self.nodes
    }

    pub fn inner_mut(&mut self) -> &mut [Node] {
        &mut self.nodes
    }
}

impl Index<NodeId> for Nodes {
    type Output = Node;

    // The index here MUST have been created by this instance, otherwise scary things will happen !
    fn index(&self, index: NodeId) -> &Self::Output {
        unsafe { self.nodes.get_unchecked(index.0 as usize) }
    }
}

impl IndexMut<NodeId> for Nodes {
    fn index_mut(&mut self, index: NodeId) -> &mut Self::Output {
        unsafe { self.nodes.get_unchecked_mut(index.0 as usize) }
    }
}

#[derive(Clone, Copy)]
pub struct ForwardLink {
    data: u32,
}

impl ForwardLink {
    pub fn new(id: NodeId, side: bool, weight: u8) -> Self {
        assert!(id.index() < (1 << 27));
        assert!(weight < 15);
        Self {
            data: (id.index() as u32) << 5 | if side { 1 << 4 } else { 0 } | weight as u32,
        }
    }

    pub fn node(self) -> NodeId {
        unsafe {
            // safety: ForwardLink is constructed using a NodeId
            NodeId::from_index((self.data >> 5) as usize)
        }
    }

    pub fn side(self) -> bool {
        self.data & (1 << 4) != 0
    }

    pub fn weight(self) -> u8 {
        (self.data & 0b1111) as u8
    }
}

impl std::fmt::Debug for ForwardLink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ForwardLink")
            .field("node", &self.node())
            .field("side", &self.side())
            .field("weight", &self.weight())
            .finish()
    }
}

#[derive(Clone, Debug, Default)]
pub struct ForwardLinkRange(std::ops::Range<usize>);

impl ForwardLinkRange {
    pub fn len(&self) -> usize {
        self.0.end - self.0.start
    }
}

#[derive(Default)]
pub struct ForwardLinks {
    links: Vec<ForwardLink>,
}

impl ForwardLinks {
    pub fn extend(&mut self, iter: impl IntoIterator<Item = ForwardLink>) -> ForwardLinkRange {
        let start = self.links.len();
        self.links.extend(iter);
        let end = self.links.len();

        ForwardLinkRange(start..end)
    }

    /// The `range` MUST have been created by this instance of ForwardLinks, otherwise this is UB.
    pub fn get(&self, range: &ForwardLinkRange) -> &[ForwardLink] {
        // Safety: there's only one instance of ForwardLinks in the backend
        unsafe { self.links.get_unchecked(range.0.clone()) }
    }

    /// After this point, all existing `ForwardLinkRange`s are invalidated.
    pub fn clear(&mut self) {
        self.links.clear();
    }
}

#[derive(Debug, Clone, Copy)]
pub enum NodeType {
    Repeater {
        delay: u8,
        facing_diode: bool,
    },
    Torch,
    Comparator {
        mode: ComparatorMode,
        far_input: Option<SignalStrength>,
        facing_diode: bool,
    },
    Lamp,
    Button,
    Lever,
    PressurePlate,
    Trapdoor,
    Wire,
    Constant,
    NoteBlock {
        noteblock_id: u16,
    },
}

#[repr(align(16))]
#[derive(Debug, Clone)]
pub struct NodeInput {
    power_counts: [u8; 16],
}

impl NodeInput {
    pub const fn new() -> Self {
        let mut power_counts = [0; 16];
        power_counts[0] = u8::MAX;
        Self { power_counts }
    }

    #[inline]
    pub fn update_power(&mut self, old_power: SignalStrength, new_power: SignalStrength) {
        self.power_counts[old_power.get() as usize] -= 1;
        self.power_counts[new_power.get() as usize] += 1;
    }

    pub fn is_powered(&self) -> bool {
        self.power_counts[0] != u8::MAX
    }

    pub fn power(&self) -> SignalStrength {
        let counts = u128::from_le_bytes(self.power_counts);
        // Safety: construction and updates preserve a total count of 255.
        let counts = unsafe { NonZeroU128::new_unchecked(counts) };
        // A nonzero u128 has at most 127 leading zeros, so the strength is 0..=15.
        SignalStrength::try_from(15 - (counts.leading_zeros() >> 3) as u8).unwrap()
    }
}

impl FromIterator<SignalStrength> for NodeInput {
    fn from_iter<T: IntoIterator<Item = SignalStrength>>(powers: T) -> Self {
        let mut inputs = Self::new();
        for (index, power) in powers.into_iter().enumerate() {
            assert!(
                index < u8::MAX as usize,
                "Exceeded the maximum number of inputs {}",
                u8::MAX
            );
            inputs.update_power(SignalStrength::ZERO, power);
        }
        inputs
    }
}

// The `Node` struct's size is currently 64 bytes which happens to be the same
// size as an L1 cache line on most modern processors. By forcing a 64-byte
// alignment, we make sure that the entire `Node` can fit on one cache line,
// preventing scenarios where we have to fetch 2 cache lines to read a single `Node`.
#[repr(align(64))]
#[derive(Debug, Clone)]
pub struct Node {
    pub ty: NodeType,
    pub default_inputs: NodeInput,
    pub side_inputs: NodeInput,

    pub fwd_link_range: ForwardLinkRange,

    pub is_io: bool,

    pub power: SignalStrength,
    pub repeater_locked: bool,
    pub changed: bool,
    pub pending_tick: bool,
}

impl Node {
    pub fn is_powered(&self) -> bool {
        !self.power.is_zero()
    }

    pub fn set_power(&mut self, power: SignalStrength) {
        self.power = power;
        self.changed = true;
    }

    pub fn set_powered(&mut self, powered: bool) {
        self.set_power(powered.into());
    }

    pub fn set_repeater_locked(&mut self, locked: bool) {
        self.repeater_locked = locked;
        self.changed = true;
    }
}
