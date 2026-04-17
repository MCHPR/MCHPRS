use std::fmt::Debug;
use std::{iter, slice};

pub trait IdxT: TryFrom<usize> + Into<usize> + Copy + Clone + Debug {}
impl<T> IdxT for T where T: TryFrom<usize> + Into<usize> + Copy + Clone + Debug {}

#[derive(Copy, Clone, Hash, PartialEq, Eq, Debug)]
pub struct NodeIndex<Idx: IdxT = usize>(Idx);

impl<Idx: IdxT> NodeIndex<Idx> {
    pub fn index(self) -> usize {
        self.0.into()
    }

    pub fn new(idx: usize) -> Self {
        let Ok(idx) = idx.try_into() else {
            panic!("limit for index type reached");
        };
        Self(idx)
    }
}

#[derive(Copy, Clone, Hash, PartialEq, Eq, Debug)]
pub struct EdgeIndex<Idx: IdxT>(Idx);

impl<Idx: IdxT> EdgeIndex<Idx> {
    pub fn index(self) -> usize {
        self.0.into()
    }

    pub fn new(idx: usize) -> Self {
        let Ok(idx) = idx.try_into() else {
            panic!("limit for index type reached");
        };
        Self(idx)
    }
}

struct RawNode<Node, Idx: IdxT> {
    value: Option<Node>,

    // [[first_incoming, last_incoming], [first_outgoing, last_outgoing]]
    edge_dirs: [Option<[EdgeIndex<Idx>; 2]>; 2],
}

impl<Node, Idx: IdxT> Default for RawNode<Node, Idx> {
    fn default() -> Self {
        Self {
            value: None,
            edge_dirs: Default::default(),
        }
    }
}

struct EdgeDefinition<Edge, Idx: IdxT> {
    value: Edge,
    endpoints: [NodeIndex<Idx>; 2],
}

const DIR_INCOMING: usize = 0;
const DIR_OUTGOING: usize = 1;
const DIR_PREV: usize = 0;
const DIR_NEXT: usize = 1;

struct RawEdge<Edge, Idx: IdxT> {
    def: Option<EdgeDefinition<Edge, Idx>>,

    /// [[prev_incoming, next_incoming], [prev_outgoing, next_outgoing]]
    dirs: [[Option<EdgeIndex<Idx>>; 2]; 2],
}

impl<Edge, Idx: IdxT> Default for RawEdge<Edge, Idx> {
    fn default() -> Self {
        Self {
            def: None,
            dirs: Default::default(),
        }
    }
}

pub struct StableGraph<Node, Edge, Idx: IdxT = usize> {
    nodes: Vec<RawNode<Node, Idx>>,
    edges: Vec<RawEdge<Edge, Idx>>,

    free_node_stack: Vec<NodeIndex<Idx>>,
    free_edge_stack: Vec<EdgeIndex<Idx>>,

    node_count: usize,
    edge_count: usize,
}

impl<Node, Edge, Idx: IdxT> Default for StableGraph<Node, Edge, Idx> {
    fn default() -> Self {
        Self {
            nodes: Default::default(),
            edges: Default::default(),
            free_node_stack: Default::default(),
            free_edge_stack: Default::default(),
            node_count: Default::default(),
            edge_count: Default::default(),
        }
    }
}

impl<Node, Edge, Idx: IdxT> StableGraph<Node, Edge, Idx> {
    pub fn node_count(&self) -> usize {
        self.node_count
    }

    pub fn edge_count(&self) -> usize {
        self.edge_count
    }

    pub fn add_node(&mut self, value: Node) -> NodeIndex<Idx> {
        let idx = match self.free_node_stack.pop() {
            Some(idx) => idx,
            None => {
                let idx = NodeIndex::new(self.nodes.len());
                self.nodes.push(Default::default());
                idx
            }
        };
        self.nodes[idx.index()].value = Some(value);
        self.node_count += 1;
        idx
    }

    pub fn add_edge(
        &mut self,
        from: NodeIndex<Idx>,
        to: NodeIndex<Idx>,
        value: Edge,
    ) -> EdgeIndex<Idx> {
        let idx = match self.free_edge_stack.pop() {
            Some(idx) => idx,
            None => {
                let idx = EdgeIndex::new(self.edges.len());
                self.edges.push(Default::default());
                idx
            }
        };
        self.edges[idx.index()].def = Some(EdgeDefinition {
            value,
            endpoints: [from, to],
        });

        let mut prevs = [None; 2];
        for (dir, node_idx) in [(DIR_INCOMING, to), (DIR_OUTGOING, from)] {
            let node = &mut self.nodes[node_idx.index()];
            if let Some([_, last]) = &mut node.edge_dirs[dir] {
                self.edges[last.index()].dirs[dir][DIR_NEXT] = Some(idx);
                prevs[dir] = Some(*last);
                *last = idx;
            } else {
                node.edge_dirs[dir] = Some([idx, idx]);
            }
        }
        self.edges[idx.index()].dirs[DIR_INCOMING][DIR_PREV] = prevs[DIR_INCOMING];
        self.edges[idx.index()].dirs[DIR_OUTGOING][DIR_PREV] = prevs[DIR_OUTGOING];

        self.edge_count += 1;
        idx
    }

    pub fn remove_edge(&mut self, idx: EdgeIndex<Idx>) -> Option<Edge> {
        let edge = self.edges.get_mut(idx.index())?;
        let def = edge.def.take()?;
        let dirs = std::mem::take(&mut edge.dirs);

        for (dir, dirs_pn) in dirs.into_iter().enumerate() {
            for dir_pn in [DIR_PREV, DIR_NEXT] {
                // When we have DIR_INCOMING, we want the destination endpoint
                let endpoint = &mut self.nodes[def.endpoints[1 - dir].index()];
                if let Some(opposite) = dirs_pn[1 - dir_pn] {
                    endpoint.edge_dirs[dir].unwrap()[dir_pn] = opposite;
                } else {
                    endpoint.edge_dirs[dir] = None;
                }
            }
        }

        for (dir, dirs_pn) in dirs.into_iter().enumerate() {
            for dir_pn in [DIR_PREV, DIR_NEXT] {
                // If we have a next edge in this direction, set it's prev edge to our prev,
                // and vice-versa
                if let Some(pn_edge) = dirs_pn[dir_pn] {
                    self.edges[pn_edge.index()].dirs[dir][1 - dir_pn] = dirs_pn[1 - dir_pn];
                }
            }
        }

        self.free_edge_stack.push(idx);
        self.edge_count -= 1;
        Some(def.value)
    }

    pub fn remove_node(&mut self, idx: NodeIndex<Idx>) -> Option<Node> {
        let node = self.nodes.get_mut(idx.index())?;
        let value = node.value.take()?;

        for dir in [DIR_INCOMING, DIR_OUTGOING] {
            // TODO: make faster
            loop {
                let node = self.nodes.get_mut(idx.index())?;
                if let Some(range) = node.edge_dirs[dir] {
                    self.remove_edge(range[0]);
                } else {
                    break;
                }
            }
        }

        self.node_count -= 1;
        Some(value)
    }

    pub fn contains_node(&self, node_idx: NodeIndex<Idx>) -> bool {
        self.nodes
            .get(node_idx.index())
            .is_some_and(|node| node.value.is_some())
    }

    pub fn node_bound(&self) -> usize {
        self.nodes.len()
    }

    pub fn node_indices(&self) -> NodeIndices<'_, Node, Idx> {
        NodeIndices {
            iter: self.nodes.iter().enumerate(),
        }
    }

    pub fn neighbors(&self, node_idx: NodeIndex<Idx>, dir: Direction) -> Neighbors<'_, Edge, Idx> {
        let node = &self.nodes[node_idx.index()];
        if node.value.is_none() {
            panic!("node not present");
        }
        let first = node.edge_dirs[dir.dir()].map(|[first, _]| first);
        Neighbors {
            edges: &self.edges,
            dir: dir.dir(),
            next: first,
        }
    }

    pub fn edges(&self, node_idx: NodeIndex<Idx>, dir: Direction) -> Edges<'_, Edge, Idx> {
        let node = &self.nodes[node_idx.index()];
        if node.value.is_none() {
            panic!("node not present");
        }
        let first = node.edge_dirs[dir.dir()].map(|[first, _]| first);
        Edges {
            edges: &self.edges,
            dir: dir.dir(),
            next: first,
        }
    }

    pub fn edge_endpoints(
        &self,
        edge_idx: EdgeIndex<Idx>,
    ) -> Option<(NodeIndex<Idx>, NodeIndex<Idx>)> {
        let edge = self.edges[edge_idx.index()].def.as_ref()?;
        Some((edge.endpoints[0], edge.endpoints[1]))
    }

    pub fn all_node_weights(&self) -> impl Iterator<Item = &Node> {
        self.nodes.iter().filter_map(|node| node.value.as_ref())
    }

    pub fn all_edges(&self) -> impl Iterator<Item = EdgeRef<'_, Edge, Idx>> {
        self.edges.iter().enumerate().filter_map(|(idx, edge)| {
            edge.def.as_ref().map(|def| EdgeRef {
                idx: EdgeIndex::new(idx),
                edge: def,
            })
        })
    }

    pub fn retain_edges<F>(&mut self, mut f: F)
    where
        F: FnMut(&'_ Self, EdgeIndex<Idx>) -> bool,
    {
        for idx in 0..self.edges.len() {
            if self.edges[idx].def.is_none() {
                continue;
            }
            let idx = EdgeIndex::new(idx);
            let keep = f(self, idx);
            if !keep {
                self.remove_edge(idx);
            }
        }
    }

    pub fn retain_nodes<F>(&mut self, mut f: F)
    where
        F: FnMut(&'_ Self, NodeIndex<Idx>) -> bool,
    {
        for idx in 0..self.nodes.len() {
            if self.nodes[idx].value.is_none() {
                continue;
            }
            let idx = NodeIndex::new(idx);
            let keep = f(self, idx);
            if !keep {
                self.remove_node(idx);
            }
        }
    }
}

impl<Node, Edge, Idx: IdxT> std::ops::Index<NodeIndex<Idx>> for StableGraph<Node, Edge, Idx> {
    type Output = Node;

    fn index(&self, index: NodeIndex<Idx>) -> &Self::Output {
        self.nodes[index.index()].value.as_ref().unwrap()
    }
}

impl<Node, Edge, Idx: IdxT> std::ops::IndexMut<NodeIndex<Idx>> for StableGraph<Node, Edge, Idx> {
    fn index_mut(&mut self, index: NodeIndex<Idx>) -> &mut Self::Output {
        self.nodes[index.index()].value.as_mut().unwrap()
    }
}

impl<Node, Edge, Idx: IdxT> std::ops::Index<EdgeIndex<Idx>> for StableGraph<Node, Edge, Idx> {
    type Output = Edge;

    fn index(&self, index: EdgeIndex<Idx>) -> &Self::Output {
        &self.edges[index.index()].def.as_ref().unwrap().value
    }
}

impl<Node, Edge, Idx: IdxT> std::ops::IndexMut<EdgeIndex<Idx>> for StableGraph<Node, Edge, Idx> {
    fn index_mut(&mut self, index: EdgeIndex<Idx>) -> &mut Self::Output {
        &mut self.edges[index.index()].def.as_mut().unwrap().value
    }
}

#[derive(Copy, Clone, Debug)]
pub enum Direction {
    Incoming,
    Outgoing,
}

impl Direction {
    fn dir(self) -> usize {
        match self {
            Direction::Incoming => DIR_INCOMING,
            Direction::Outgoing => DIR_OUTGOING,
        }
    }
}

pub struct NodeIndices<'a, Node: 'a, Idx: IdxT> {
    iter: iter::Enumerate<slice::Iter<'a, RawNode<Node, Idx>>>,
}

impl<Node, Idx: IdxT> Iterator for NodeIndices<'_, Node, Idx> {
    type Item = NodeIndex<Idx>;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter
            .find_map(|(idx, node)| node.value.as_ref().map(|_| NodeIndex::new(idx)))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (lower, _) = self.iter.size_hint();
        (lower, None)
    }
}

pub struct Neighbors<'a, Edge, Idx: IdxT> {
    edges: &'a [RawEdge<Edge, Idx>],
    dir: usize,
    next: Option<EdgeIndex<Idx>>,
}

impl<Edge, Idx: IdxT> Iterator for Neighbors<'_, Edge, Idx> {
    type Item = NodeIndex<Idx>;

    fn next(&mut self) -> Option<Self::Item> {
        let edge = &self.edges[self.next?.index()];
        let def = edge.def.as_ref().unwrap();

        self.next = edge.dirs[self.dir][DIR_NEXT];
        Some(def.endpoints[1 - self.dir])
    }
}

impl<Edge, Idx: IdxT> Neighbors<'_, Edge, Idx> {
    pub fn detach(&self) -> NeighborsDetached<Idx> {
        NeighborsDetached {
            dir: self.dir,
            next: self.next,
        }
    }
}

pub struct NeighborsDetached<Idx: IdxT> {
    dir: usize,
    next: Option<EdgeIndex<Idx>>,
}

impl<Idx: IdxT> NeighborsDetached<Idx> {
    pub fn next<Node, Edge>(
        &mut self,
        graph: &StableGraph<Node, Edge, Idx>,
    ) -> Option<(EdgeIndex<Idx>, NodeIndex<Idx>)> {
        let edge_idx = self.next?;
        let edge = &graph.edges[edge_idx.index()];
        let def = edge.def.as_ref().unwrap();

        self.next = edge.dirs[self.dir][DIR_NEXT];
        Some((edge_idx, def.endpoints[1 - self.dir]))
    }

    pub fn next_node<Node, Edge>(
        &mut self,
        graph: &StableGraph<Node, Edge, Idx>,
    ) -> Option<NodeIndex<Idx>> {
        self.next(graph).map(|(_, node_idx)| node_idx)
    }

    pub fn next_edge<Node, Edge>(
        &mut self,
        graph: &StableGraph<Node, Edge, Idx>,
    ) -> Option<EdgeIndex<Idx>> {
        self.next(graph).map(|(edge_idx, _)| edge_idx)
    }
}

pub struct EdgeRef<'a, Edge, Idx: IdxT> {
    idx: EdgeIndex<Idx>,
    edge: &'a EdgeDefinition<Edge, Idx>,
}

impl<Edge, Idx: IdxT> EdgeRef<'_, Edge, Idx> {
    pub fn id(&self) -> EdgeIndex<Idx> {
        self.idx
    }

    pub fn weight(&self) -> &'_ Edge {
        &self.edge.value
    }

    pub fn source(&self) -> NodeIndex<Idx> {
        self.edge.endpoints[0]
    }

    pub fn target(&self) -> NodeIndex<Idx> {
        self.edge.endpoints[0]
    }
}

impl<'a, Edge, Idx: IdxT> Iterator for Edges<'a, Edge, Idx> {
    type Item = EdgeRef<'a, Edge, Idx>;

    fn next(&mut self) -> Option<Self::Item> {
        let idx = self.next?;
        let edge = &self.edges[idx.index()];
        let def = edge.def.as_ref().unwrap();

        self.next = edge.dirs[self.dir][DIR_NEXT];
        Some(EdgeRef { idx, edge: def })
    }
}

pub struct Edges<'a, Edge, Idx: IdxT> {
    edges: &'a [RawEdge<Edge, Idx>],
    dir: usize,
    next: Option<EdgeIndex<Idx>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_graph() {
        let g: StableGraph<usize, usize, usize> = StableGraph::default();
        assert_eq!(g.node_count(), 0);
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn test_add_nodes() {
        let mut g: StableGraph<usize, usize, usize> = StableGraph::default();

        let n1 = g.add_node(10);
        let n2 = g.add_node(20);

        assert_ne!(n1, n2);
        assert_eq!(g.node_count(), 2);
    }

    #[test]
    fn test_add_edges() {
        let mut g: StableGraph<usize, usize, usize> = StableGraph::default();

        let n1 = g.add_node(1);
        let n2 = g.add_node(2);

        let e = g.add_edge(n1, n2, 100);

        assert_eq!(g.edge_count(), 1);
        // sanity: edge index exists (not much else we can assert without getters)
        let removed = g.remove_edge(e);
        assert_eq!(removed, Some(100));
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn test_remove_node() {
        let mut g: StableGraph<usize, usize, usize> = StableGraph::default();

        let n1 = g.add_node(42);
        let n2 = g.add_node(99);

        assert_eq!(g.node_count(), 2);

        let removed = g.remove_node(n1);
        assert_eq!(removed, Some(42));
        assert_eq!(g.node_count(), 1);

        let removed_again = g.remove_node(n1);
        assert_eq!(removed_again, None);

        // ensure other node still exists
        let removed_n2 = g.remove_node(n2);
        assert_eq!(removed_n2, Some(99));
        assert_eq!(g.node_count(), 0);
    }

    #[test]
    fn test_remove_edge() {
        let mut g: StableGraph<usize, usize, usize> = StableGraph::default();

        let n1 = g.add_node(1);
        let n2 = g.add_node(2);

        let e1 = g.add_edge(n1, n2, 5);
        let e2 = g.add_edge(n2, n1, 10);

        assert_eq!(g.edge_count(), 2);

        assert_eq!(g.remove_edge(e1), Some(5));
        assert_eq!(g.edge_count(), 1);

        assert_eq!(g.remove_edge(e1), None); // already removed

        assert_eq!(g.remove_edge(e2), Some(10));
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn test_remove_node_removes_edges() {
        let mut g: StableGraph<usize, usize, usize> = StableGraph::default();

        let n1 = g.add_node(1);
        let n2 = g.add_node(2);
        let n3 = g.add_node(3);

        let e1 = g.add_edge(n1, n2, 10);
        let e2 = g.add_edge(n2, n3, 20);
        let e3 = g.add_edge(n1, n3, 30);

        assert_eq!(g.edge_count(), 3);

        // removing n2 should remove edges involving it
        g.remove_node(n2);

        // e1 and e2 should be gone
        assert_eq!(g.remove_edge(e1), None);
        assert_eq!(g.remove_edge(e2), None);

        // e3 should still exist
        assert_eq!(g.remove_edge(e3), Some(30));
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn test_multiple_add_remove_cycles() {
        let mut g: StableGraph<usize, usize, usize> = StableGraph::default();

        let n1 = g.add_node(1);
        let n2 = g.add_node(2);

        let e1 = g.add_edge(n1, n2, 100);
        assert_eq!(g.edge_count(), 1);

        g.remove_edge(e1);
        assert_eq!(g.edge_count(), 0);

        let e2 = g.add_edge(n2, n1, 200);
        assert_eq!(g.edge_count(), 1);

        assert_eq!(g.remove_edge(e2), Some(200));
        assert_eq!(g.edge_count(), 0);
    }
}
