use crate::compile_graph::{CompileGraph, CompileLink, CompileNode, LinkType, NodeIdx, NodeType};
use petgraph::stable_graph::EdgeReference;
use petgraph::visit::EdgeRef;
use petgraph::Direction;
use rustc_hash::FxHashMap;
use std::fmt;

fn dump_node_name(
    f: &mut fmt::Formatter<'_>,
    naming: &FxHashMap<NodeIdx, String>,
    node_idx: NodeIdx,
) -> fmt::Result {
    write!(f, "%")?;
    if let Some(name) = naming.get(&node_idx) {
        write!(f, "{}", name)
    } else {
        write!(f, "{}", node_idx.index())
    }
}

fn dump_edge(
    f: &mut fmt::Formatter<'_>,
    ctx: &FmtContext<'_>,
    src: NodeIdx,
    weight: &CompileLink,
) -> fmt::Result {
    dump_node_name(f, ctx.naming, src)?;
    write!(f, ":{}", weight.ss)
}

fn dump_edges<'a>(
    f: &mut fmt::Formatter<'_>,
    ctx: &FmtContext<'_>,
    edges: impl Iterator<Item = EdgeReference<'a, CompileLink>>,
) -> fmt::Result {
    write!(f, "{{")?;
    let mut first = true;
    for edge in edges {
        if !first {
            write!(f, ", ")?;
        } else {
            first = false;
        }
        dump_edge(f, ctx, edge.source(), edge.weight())?;
    }
    write!(f, "}}")
}

struct FmtContext<'a> {
    graph: &'a CompileGraph,
    node_idx: NodeIdx,
    naming: &'a FxHashMap<NodeIdx, String>,
}

impl<'a> FmtContext<'a> {
    fn node(&self) -> &CompileNode {
        &self.graph[self.node_idx]
    }
}

struct SideInputFormatter<'a> {
    ctx: &'a FmtContext<'a>,
}

impl<'a> fmt::Display for SideInputFormatter<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let default_inputs = self
            .ctx
            .graph
            .edges_directed(self.ctx.node_idx, Direction::Incoming)
            .filter(|edge| edge.weight().ty == LinkType::Side);
        dump_edges(f, self.ctx, default_inputs)
    }
}

struct DefaultInputFormatter<'a> {
    ctx: &'a FmtContext<'a>,
}

impl<'a> fmt::Display for DefaultInputFormatter<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let default_inputs = self
            .ctx
            .graph
            .edges_directed(self.ctx.node_idx, Direction::Incoming)
            .filter(|edge| edge.weight().ty == LinkType::Default);
        dump_edges(f, self.ctx, default_inputs)
    }
}

struct InputFormatter<'a> {
    ctx: &'a FmtContext<'a>,
}

struct FarInputFormatter(Option<u8>);

impl fmt::Display for FarInputFormatter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(ss) => write!(f, "{}", ss),
            None => write!(f, "none"),
        }
    }
}

impl<'a> InputFormatter<'a> {
    fn default_inputs(&self) -> DefaultInputFormatter<'a> {
        DefaultInputFormatter { ctx: self.ctx }
    }

    fn side_inputs(&self) -> SideInputFormatter<'a> {
        SideInputFormatter { ctx: self.ctx }
    }
}

fn dump_node(f: &mut fmt::Formatter<'_>, ctx: &FmtContext<'_>) -> fmt::Result {
    let node = ctx.node();
    let inputs = InputFormatter { ctx };

    match node.ty {
        NodeType::Repeater {
            delay,
            facing_diode,
        } => write!(
            f,
            "repeater {}, {}, {}, {}, {}",
            delay,
            facing_diode,
            node.state.powered,
            inputs.default_inputs(),
            inputs.side_inputs(),
        ),
        NodeType::Torch => write!(
            f,
            "torch {}, {}",
            node.state.powered,
            inputs.default_inputs()
        ),
        NodeType::Comparator {
            mode,
            far_input,
            facing_diode,
        } => write!(
            f,
            "comparator {}, {}, {}, {}, {}, {}",
            mode.to_string(),
            FarInputFormatter(far_input),
            facing_diode,
            node.state.output_strength,
            inputs.default_inputs(),
            inputs.side_inputs(),
        ),
        NodeType::Lamp => write!(
            f,
            "lamp {}, {}",
            node.state.powered,
            inputs.default_inputs()
        ),
        NodeType::Button => write!(f, "button {}", node.state.powered),
        NodeType::Lever => write!(f, "lever {}", node.state.powered),
        NodeType::PressurePlate => write!(f, "pressure_plate {}", node.state.powered),
        NodeType::Trapdoor => write!(f, "trapdoor {}", node.state.powered),
        NodeType::Wire => write!(f, "wire {}", node.state.output_strength),
        NodeType::Constant => write!(f, "constant {}", node.state.output_strength),
        NodeType::NoteBlock { instrument, note } => {
            write!(f, "note_block {}, {}", instrument.to_string(), note)
        }
    }
}

pub fn dump_graph(
    f: &mut fmt::Formatter<'_>,
    graph: &CompileGraph,
    naming: &FxHashMap<NodeIdx, String>,
) -> fmt::Result {
    writeln!(f, "circuit {{")?;
    for node_idx in graph.node_indices() {
        write!(f, "  ")?;
        dump_node_name(f, naming, node_idx)?;
        write!(f, " = ")?;
        let ctx = FmtContext {
            graph,
            node_idx,
            naming,
        };
        dump_node(f, &ctx)?;
        writeln!(f)?;
    }
    write!(f, "}}")
}

struct GraphDumper<'a> {
    graph: &'a CompileGraph,
}

impl<'a> fmt::Display for GraphDumper<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        dump_graph(f, self.graph, &Default::default())
    }
}

pub trait DumpGraph {
    fn dump(&self);
}

impl DumpGraph for CompileGraph {
    fn dump(&self) {
        eprintln!("{}", GraphDumper { graph: self });
    }
}
