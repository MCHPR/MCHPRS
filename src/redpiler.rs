use crate::blocks::{Block, BlockPos, RedstoneComparator, BlockFace, ButtonFace, LeverFace};
use crate::world::World;
use crate::plot::Plot;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt::Display;

fn is_wire(world: &dyn World, pos: BlockPos) -> bool {
    matches!(world.get_block(pos), Block::RedstoneWire { .. })
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct NodeId {
    index: usize,
}

#[derive(Debug, Clone, Copy)]
enum LinkType {
    Default,
    Side,
}

#[derive(Debug, Clone)]
struct Link {
    ty: LinkType,
    start: NodeId,
    weight: u32,
    end: NodeId,
}

impl Link {
    fn new(ty: LinkType, start: NodeId, weight: u32, end: NodeId) -> Link {
        Link {
            ty, start, weight, end
        }
    }
}

#[derive(Debug, Clone)]
struct Node {
    pos: BlockPos,
    state: Block,
    inputs: Vec<Link>,
    updates: Vec<NodeId>,
}

impl Node {
    fn new(pos: BlockPos, state: Block) -> Node {
        Node {
            pos,
            state,
            inputs: vec![],
            updates: vec![]
        }
    }

    fn from_block(pos: BlockPos, block: Block) -> Option<Node> {
        let is_node = matches!(block,
            Block::RedstoneComparator { .. }
            | Block::RedstoneTorch { .. }
            | Block::RedstoneWallTorch { .. }
            | Block::RedstoneRepeater { .. }
            | Block::RedstoneWire { .. }
            | Block::Lever { .. }
            | Block::StoneButton { .. }
            | Block::RedstoneBlock { .. }
        );

        if is_node {
            Some(Node::new(pos, block))
        } else {
            None
        }
    }
}

struct InputSearch<'a> {
    plot: &'a mut Plot,
    pos_map: HashMap<BlockPos, NodeId>,
}

impl<'a> InputSearch<'a> {
    fn new(plot: &mut Plot) -> InputSearch<'_> {
        let compiler = &mut plot.redpiler;
        let nodes = &mut compiler.nodes;

        let mut pos_map = HashMap::new();
        for (i, node) in nodes.iter().enumerate() {
            pos_map.insert(node.pos, NodeId { index: i });
        }

        InputSearch {
            plot,
            pos_map,
        }
    }

    fn provides_weak_power(&self,
        block: Block,
        side: BlockFace
    ) -> bool {
        match block {
            Block::RedstoneTorch { .. } => true,
            Block::RedstoneWallTorch { facing, .. } if facing.block_face() != side => true,
            Block::RedstoneBlock {} => true,
            Block::Lever { .. }  => true,
            Block::StoneButton { .. }  => true,
            Block::RedstoneRepeater { repeater }
                if repeater.facing.block_face() == side => true,
            Block::RedstoneComparator { comparator } if comparator.facing.block_face() == side => true,
            _ => false,
        }
    }

    fn provides_strong_power(
        &self, 
        block: Block,
        side: BlockFace
    ) -> bool {
        match block {
            Block::RedstoneTorch { lit: true } if side == BlockFace::Bottom => true,
            Block::RedstoneWallTorch { lit: true, .. } if side == BlockFace::Bottom => true,
            Block::Lever { lever } => match side {
                BlockFace::Top if lever.face == LeverFace::Floor => true,
                BlockFace::Bottom if lever.face == LeverFace::Ceiling  => true,
                _ if lever.facing == side.to_direction() => true,
                _ => false,
            },
            Block::StoneButton { button } => match side {
                BlockFace::Top if button.face == ButtonFace::Floor && button.powered => true,
                BlockFace::Bottom if button.face == ButtonFace::Ceiling && button.powered => true,
                _ if button.facing == side.to_direction() => true,
                _ => false,
            },
            Block::RedstoneRepeater { .. } => self.provides_weak_power(block, side),
            Block::RedstoneComparator { .. } => self.provides_weak_power(block, side),
            _ => false,
        }
    }

    fn get_redstone_links(&self, block: Block, side: BlockFace, pos: BlockPos, link_ty: LinkType, distance: u32, start_node: NodeId, search_wire: bool) -> Vec<Link> {
        let mut res = Vec::new();
        if block.is_solid() {
            for side in &BlockFace::values() {
                let block = self.plot.get_block(pos.offset(*side));
                let pos = pos.offset(*side);
                if self.provides_strong_power(block, *side) {
                    res.push(Link::new(link_ty, start_node, distance, self.pos_map[&pos]));
                }

                if let Block::RedstoneWire { wire } = block {
                    let direction = side.to_direction();
                    if search_wire && !wire
                        .get_regulated_sides(self.plot, pos)
                        .get_current_side(direction.opposite())
                        .is_none() 
                    {
                        res.append(&mut self.search_wire(start_node, block, pos, link_ty, distance));
                    }
                }
            }
        } else if self.provides_weak_power(block, side) {
            res.push(Link::new(link_ty, start_node, distance, self.pos_map[&pos]));
        } else if let Block::RedstoneWire { wire } = block {
            let direction = side.to_direction();
            if search_wire && !wire
                .get_regulated_sides(self.plot, pos)
                .get_current_side(direction.opposite())
                .is_none() 
            {
                res.append(&mut self.search_wire(start_node, block, pos, link_ty, distance));
            }
        }
        res
    }

    fn search_wire(&self, start_node: NodeId, root_block: Block, root_pos: BlockPos, link_ty: LinkType, mut distance: u32) -> Vec<Link> {
        let mut res = Vec::new();

        let mut queue: VecDeque<BlockPos> = VecDeque::new();
        let mut discovered = HashSet::new();

        discovered.insert(root_pos);
        queue.push_back(root_pos);

        while !queue.is_empty() {
            let pos = queue.pop_front().unwrap();
            dbg!(pos);

            let up_pos = pos.offset(BlockFace::Top);
            let up_block = self.plot.get_block(up_pos);

            for side in &BlockFace::values() {
                let neighbor_pos = pos.offset(*side);
                let neighbor = self.plot.get_block(neighbor_pos);

                res.append(&mut self.get_redstone_links(neighbor, *side, neighbor_pos, link_ty, distance, start_node, false));

                if is_wire(self.plot, neighbor_pos) && !discovered.contains(&neighbor_pos) {
                    queue.push_back(neighbor_pos);
                    discovered.insert(neighbor_pos);
                }

                if side.is_horizontal() {
                    if !up_block.is_solid() && !neighbor.is_transparent() {
                        let neighbor_up_pos = neighbor_pos.offset(BlockFace::Top);
                        if is_wire(self.plot, neighbor_up_pos) && !discovered.contains(&neighbor_up_pos) {
                            queue.push_back(neighbor_up_pos);
                            discovered.insert(neighbor_up_pos);
                        }
                    }
    
                    if !neighbor.is_solid() {
                        let neighbor_down_pos = neighbor_pos.offset(BlockFace::Top);
                        if is_wire(self.plot, neighbor_down_pos) && !discovered.contains(&neighbor_down_pos) {
                            queue.push_back(neighbor_down_pos);
                            discovered.insert(neighbor_down_pos);
                        }
                    }
                }
            }
        }  

        res
    }

    fn search(&mut self) {
        let nodes = self.plot.redpiler.nodes.clone();
        for (i, node) in nodes.into_iter().enumerate() {
            let id = NodeId { index: i };
            match node.state {
                Block::RedstoneRepeater { repeater } => {
                    let facing = repeater.facing;

                    let input_pos = node.pos.offset(facing.block_face());
                    let input_block = self.plot.get_block(input_pos);
                    let inputs = self.get_redstone_links(input_block, facing.block_face(), input_pos, LinkType::Default, 0, NodeId { index: i }, true);
                    self.plot.redpiler.nodes[i].inputs = inputs;
                }
                Block::RedstoneWire { .. } => {
                    let inputs = self.search_wire(id, node.state, node.pos, LinkType::Default, 0);
                    self.plot.redpiler.nodes[i].inputs = inputs;
                }
                _ => {} // TODO: The other nodes
            }
        }
    }
}

#[derive(Default)]
pub struct Compiler {
    nodes: Vec<Node>
}

impl Compiler {
    pub fn compile(plot: &mut Plot, first_pos: BlockPos, second_pos: BlockPos) {
        Compiler::identify_nodes(plot, first_pos, second_pos);
        InputSearch::new(plot).search();
        let compiler = &mut plot.redpiler;
        // dbg!(&compiler.nodes);
        println!("{}", compiler);

        // TODO: Everything else
    }

    pub fn reset(&mut self) {
        self.nodes.clear();
    }

    fn identify_node(&mut self, pos: BlockPos, block: Block) {
        if let Some(node) = Node::from_block(pos, block) {
            self.nodes.push(node);
        }
    }

    fn identify_nodes(plot: &mut Plot, first_pos: BlockPos, second_pos: BlockPos) {
        let start_pos = first_pos.min(second_pos);
        let end_pos = first_pos.max(second_pos);
        for y in start_pos.y..=end_pos.y {
            for z in start_pos.z..=end_pos.z {
                for x in start_pos.x..=end_pos.x {
                    let pos = BlockPos::new(x, y, z);
                    plot.redpiler.identify_node(pos, plot.get_block(pos));
                }
            }
        }
    }
}

impl Display for Compiler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("digraph{")?;
        for (id, node) in self.nodes.iter().enumerate() {
            write!(f, "n{}[label=\"({}, {}, {})\"];", id, node.pos.x, node.pos.y, node.pos.z)?;
            for link in &node.inputs {
                write!(f, "n{}->n{}[label=\"{}\"];", link.end.index, link.start.index, link.weight)?;
            }
        }
        f.write_str("}\n")
    }
}