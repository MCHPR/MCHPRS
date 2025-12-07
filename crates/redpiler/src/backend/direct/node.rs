use mchprs_blocks::blocks::ComparatorMode;
use std::num::NonZeroU8;
use std::ops::{Index, IndexMut};

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct NodeId(u32);

impl NodeId {
    pub fn index(self) -> usize {
        self.0 as usize
    }

    /// Safety: index must be within bounds of nodes array
    /// Safety: index must point to a valid Node and not towards a ForwardLink block
    pub unsafe fn from_index(index: usize) -> NodeId {
        NodeId(index as u32)
    }
}

// This is Pretty Bad:tm: because one can create a NodeId using another instance of Nodes,
// but at least some type system protection is better than none.
#[derive(Default)]
pub struct Nodes {
    nodes: Box<[Node]>,
    valid: Box<[bool]>,
}

impl Nodes {
    pub fn new(nodes: Box<[Node]>) -> Nodes {
        let mut nodes = Nodes {
            nodes,
            valid: Box::new([]),
        };

        let mut valid = vec![false; nodes.nodes.len()].into_boxed_slice();
        for id in nodes.ids() {
            valid[id.index()] = true;
        }
        nodes.valid = valid;

        nodes
    }

    pub fn get(&self, idx: usize) -> NodeId {
        if idx >= self.nodes.len() {
            panic!(
                "node index out of bounds: {}, len={}",
                idx,
                self.nodes.len()
            )
        }
        if !self.valid[idx] {
            panic!("node index invalid: {}", idx)
        }
        NodeId(idx as u32)
    }

    pub fn forward_link(&self, id: NodeId) -> &[ForwardLink] {
        // Safety: Node is followed by correct number of forward links
        unsafe { self[id].forward_links() }
    }

    pub fn ids(&self) -> impl '_ + Iterator<Item = NodeId> {
        self.enumerate().map(|(id, _)| id)
    }

    pub fn enumerate(&self) -> impl Iterator<Item = (NodeId, &Node)> {
        let mut skip = 0;
        self.nodes.iter().enumerate().filter_map(move |(i, node)| {
            if skip > 0 {
                skip -= 1;
                return None;
            }
            skip = node.forward_link_blocks();

            // Safety: Bounds checked and ForwardLinks skipped over
            let id = unsafe { NodeId::from_index(i) };
            Some((id, node))
        })
    }

    pub fn enumerate_mut(&mut self) -> impl Iterator<Item = (NodeId, &mut Node)> {
        let mut skip = 0;
        self.nodes
            .iter_mut()
            .enumerate()
            .filter_map(move |(i, node)| {
                if skip > 0 {
                    skip -= 1;
                    return None;
                }
                skip = node.forward_link_blocks();

                // Safety: Bounds checked and ForwardLinks skipped over
                let id = unsafe { NodeId::from_index(i) };
                Some((id, node))
            })
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
    pub fn new(id: NodeId, side: bool, ss: u8) -> Self {
        assert!(id.index() < (1 << 27));
        // the clamp_weights compile pass should ensure ss < 15
        assert!(ss < 15);
        Self {
            data: (id.index() as u32) << 5 | if side { 1 << 4 } else { 0 } | ss as u32,
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

    pub fn ss(self) -> u8 {
        (self.data & 0b1111) as u8
    }
}

impl std::fmt::Debug for ForwardLink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ForwardLink")
            .field("node", &self.node())
            .field("side", &self.side())
            .field("ss", &self.ss())
            .finish()
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
        far_input: Option<NonMaxU8>,
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
#[derive(Debug, Clone, Default)]
pub struct NodeInput {
    pub ss_counts: [u8; 16],
}

#[derive(Debug, Clone, Copy)]
pub struct NonMaxU8(NonZeroU8);

impl NonMaxU8 {
    pub fn new(value: u8) -> Option<Self> {
        NonZeroU8::new(value + 1).map(Self)
    }

    pub fn get(self) -> u8 {
        self.0.get() - 1
    }
}

// The `Node` struct's size is currently 64 bytes which happens to be the same
// size as an L1 cache line on most modern processors. By forcing a 64-byte
// alignment, we make sure that the entire `Node` can fit on one cache line,
// preventing scenarios where we have to fetch 2 cache lines to read a single `Node`.
#[repr(C, align(64))]
#[derive(Debug, Clone)]
pub struct Node {
    pub default_inputs: NodeInput,
    pub side_inputs: NodeInput,
    /// The index to the first forward link of this node.
    pub ty: NodeType,

    pub fwd_link_len: u16,

    pub output_power: u8,

    pub is_io: bool,

    /// Powered or lit
    pub powered: bool,
    /// Only for repeaters
    pub locked: bool,
    pub changed: bool,
    pub pending_tick: bool,

    // links must be at the very end of the struct
    pub fwd_links: LinkBuffer,
}

const LINKS_IN_NODE: usize = 5;
type LinkBuffer = [ForwardLink; LINKS_IN_NODE];

impl Node {
    pub fn forward_link_blocks_for(fwd_link_len: usize) -> usize {
        const BLOCK_SIZE: usize = size_of::<Node>() / size_of::<ForwardLink>();

        const {
            use std::mem::offset_of;

            assert!(size_of::<Node>() % size_of::<ForwardLink>() == 0);
            assert!(align_of::<Node>() % align_of::<ForwardLink>() == 0);
            assert!(offset_of!(Node, fwd_links) + size_of::<LinkBuffer>() == size_of::<Node>())
        }

        (fwd_link_len as usize + BLOCK_SIZE - 1 - LINKS_IN_NODE) / BLOCK_SIZE
    }

    pub fn forward_link_blocks(&self) -> usize {
        Node::forward_link_blocks_for(self.fwd_link_len as usize)
    }

    /// Safety: self must be followed by correct number of forward links
    pub unsafe fn forward_links(&self) -> &[ForwardLink] {
        std::slice::from_raw_parts(self.fwd_links.as_ptr(), self.fwd_link_len as usize)
    }

    /// Safety: self must be followed by correct number of forward links
    pub unsafe fn forward_links_mut(&mut self) -> &mut [ForwardLink] {
        std::slice::from_raw_parts_mut(self.fwd_links.as_mut_ptr(), self.fwd_link_len as usize)
    }
}
