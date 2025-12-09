use crate::compile_graph::{CompileGraph, CompileLink, CompileNode, LinkType, NodeIdx, NodeType};
use mchprs_blocks::blocks::{ComparatorMode, Instrument};
use petgraph::stable_graph::EdgeReference;
use petgraph::visit::EdgeRef;
use petgraph::Direction;
use rustc_hash::FxHashMap;
use std::fmt;
use std::iter::Peekable;
use std::str::Chars;

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
    write!(f, "[")?;
    let mut first = true;
    for edge in edges {
        if !first {
            write!(f, ", ")?;
        } else {
            first = false;
        }
        dump_edge(f, ctx, edge.source(), edge.weight())?;
    }
    write!(f, "]")
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

    write!(f, "  ")?;
    dump_node_name(f, ctx.naming, ctx.node_idx)?;
    write!(f, " = ")?;

    match node.ty {
        NodeType::Repeater {
            delay,
            facing_diode,
        } => write!(
            f,
            "repeater {}, {}, {}, {}, {}, {}",
            delay,
            facing_diode,
            node.state.repeater_locked,
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
            mode,
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
        NodeType::Trapdoor => write!(
            f,
            "trapdoor {}, {}",
            node.state.powered,
            inputs.default_inputs()
        ),
        NodeType::Wire => write!(
            f,
            "wire {}, {}",
            node.state.output_strength,
            inputs.default_inputs()
        ),
        NodeType::Constant => write!(f, "constant {}", node.state.output_strength),
        NodeType::NoteBlock { instrument, note } => {
            write!(
                f,
                "note_block {}, {}, {}",
                instrument,
                note,
                inputs.default_inputs()
            )
        }
    }?;

    if let Some((pos, _)) = node.block {
        write!(f, "  # Loc: {}", pos)?;
    }

    Ok(())
}

pub fn dump_graph(
    f: &mut fmt::Formatter<'_>,
    graph: &CompileGraph,
    naming: &FxHashMap<NodeIdx, String>,
) -> fmt::Result {
    writeln!(f, "circuit {{")?;
    for node_idx in graph.node_indices() {
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
    fn dump_to_stderr(&self);
    fn dump_to_string(&self) -> String;
}

impl DumpGraph for CompileGraph {
    fn dump_to_stderr(&self) {
        eprintln!("{}", GraphDumper { graph: self });
    }

    fn dump_to_string(&self) -> String {
        format!("{}", GraphDumper { graph: self })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ComponentType {
    Repeater,
    Torch,
    Comparator,
    Lamp,
    Button,
    Lever,
    PressurePlate,
    Trapdoor,
    Wire,
    Constant,
    NoteBlock,
}

#[derive(PartialEq, Debug)]
enum Token {
    /// The circuit keyword
    Circuit,
    ComponentType(ComponentType),
    Instrument(Instrument),
    ComparatorMode(ComparatorMode),
    Bool(bool),
    Int(u32),
    String(String),
    /// %name
    Value(String),
    /// The `none` keyword
    None,
    Colon,
    RightBracket,
    LeftBracket,
    RightCurlyBrace,
    LeftCurlyBrace,
    Comma,
    Equals,
}

struct Lexer<'a> {
    src_iter: Peekable<Chars<'a>>,
    // src: &'a str,
    pos: usize,
    tokens: Vec<Token>,
}

impl<'a> Lexer<'a> {
    fn lex(input: &str) -> Vec<Token> {
        let mut lexer = Lexer {
            src_iter: input.chars().peekable(),
            pos: 0,
            tokens: Vec::new(),
        };
        while let Some(c) = lexer.src_iter.peek() {
            let token = match *c {
                ':' => Token::Colon,
                '[' => Token::LeftBracket,
                ']' => Token::RightBracket,
                '{' => Token::LeftCurlyBrace,
                '}' => Token::RightCurlyBrace,
                ',' => Token::Comma,
                '=' => Token::Equals,
                '#' => {
                    lexer.skip_line();
                    continue;
                }
                '%' => {
                    lexer.src_iter.next();
                    let name = lexer.read_value_ident();
                    lexer.tokens.push(Token::Value(name));
                    continue;
                }
                _ if c.is_whitespace() => {
                    lexer.src_iter.next();
                    continue;
                }
                _ if c.is_ascii_digit() => {
                    lexer.read_int();
                    continue;
                }
                _ if c.is_ascii_alphabetic() => {
                    lexer.read_keyword();
                    continue;
                }
                _ => panic!("Unexpected character: {}", c),
            };
            lexer.tokens.push(token);
            lexer.src_iter.next();
        }
        lexer.tokens
    }

    fn read_value_ident(&mut self) -> String {
        let mut str = String::new();
        while let Some(c) = self.src_iter.peek() {
            if !c.is_ascii_digit() && !c.is_ascii_alphabetic() {
                break;
            }
            str.push(*c);
            self.src_iter.next();
        }
        str
    }

    fn read_int(&mut self) {
        let mut num_string = String::new();
        while let Some(c) = self.src_iter.peek() {
            if !c.is_ascii_digit() {
                break;
            }
            num_string.push(*c);
            self.src_iter.next();
        }
        let num: u32 = num_string.parse().unwrap();
        self.tokens.push(Token::Int(num));
    }

    fn read_keyword(&mut self) {
        let mut word = String::new();
        while let Some(c) = self.src_iter.peek() {
            if !c.is_ascii_alphabetic() {
                break;
            }
            word.push(*c);
            self.src_iter.next();
        }

        let token = match word.as_str() {
            "circuit" => Token::Circuit,
            "none" => Token::None,
            "repeater" => Token::ComponentType(ComponentType::Repeater),
            "torch" => Token::ComponentType(ComponentType::Torch),
            "comparator" => Token::ComponentType(ComponentType::Comparator),
            "lamp" => Token::ComponentType(ComponentType::Lamp),
            "button" => Token::ComponentType(ComponentType::Button),
            "lever" => Token::ComponentType(ComponentType::Lever),
            "pressure_plate" => Token::ComponentType(ComponentType::PressurePlate),
            "trapdoor" => Token::ComponentType(ComponentType::Trapdoor),
            "wire" => Token::ComponentType(ComponentType::Wire),
            "constant" => Token::ComponentType(ComponentType::Constant),
            "note_block" => Token::ComponentType(ComponentType::NoteBlock),
            // Note Block Instruments
            "harp" => Token::Instrument(Instrument::Harp),
            "basedrum" => Token::Instrument(Instrument::Basedrum),
            "snare" => Token::Instrument(Instrument::Snare),
            "hat" => Token::Instrument(Instrument::Hat),
            "bass" => Token::Instrument(Instrument::Bass),
            "flute" => Token::Instrument(Instrument::Flute),
            "bell" => Token::Instrument(Instrument::Bell),
            "guitar" => Token::Instrument(Instrument::Guitar),
            "chime" => Token::Instrument(Instrument::Chime),
            "xylophone" => Token::Instrument(Instrument::Xylophone),
            "iron_xylophone" => Token::Instrument(Instrument::IronXylophone),
            "cow_bell" => Token::Instrument(Instrument::CowBell),
            "didgeridoo" => Token::Instrument(Instrument::Didgeridoo),
            "bit" => Token::Instrument(Instrument::Bit),
            "banjo" => Token::Instrument(Instrument::Banjo),
            "pling" => Token::Instrument(Instrument::Pling),
            "zombie" => Token::Instrument(Instrument::Zombie),
            "skeleton" => Token::Instrument(Instrument::Skeleton),
            "creeper" => Token::Instrument(Instrument::Creeper),
            "dragon" => Token::Instrument(Instrument::Dragon),
            "wither_skeleton" => Token::Instrument(Instrument::WitherSkeleton),
            "piglin" => Token::Instrument(Instrument::Piglin),
            // Bool
            "false" => Token::Bool(false),
            "true" => Token::Bool(true),
            // Comparator Mode
            "compare" => Token::ComparatorMode(ComparatorMode::Compare),
            "subtract" => Token::ComparatorMode(ComparatorMode::Subtract),
            _ => panic!("Unknown keyword: {}", word),
        };
        self.tokens.push(token);
    }

    fn skip_line(&mut self) {
        while let Some(c) = self.src_iter.next() {
            if c == '\n' {
                break;
            }
        }
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.src_iter.peek() {
            if !c.is_whitespace() {
                break;
            }
            self.src_iter.next();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lexer_comparator() {
        use Token::*;
        let src = r"
            circuit {
              %comp = comparator subtract, none, false, 0, [%10:2, %11:1], [%10:4, %11:1]  # Loc: (126, 8, 118)
            }
        ";
        let actual_tokens = Lexer::lex(src);
        let expected_tokens = &[
            Circuit,
            LeftCurlyBrace,
            Value("comp".to_string()),
            Equals,
            ComponentType(super::ComponentType::Comparator),
            ComparatorMode(super::ComparatorMode::Subtract),
            Comma,
            None,
            Comma,
            Bool(false),
            Comma,
            Int(0),
            Comma,
            LeftBracket,
            Value("10".to_string()),
            Colon,
            Int(2),
            Comma,
            Value("11".to_string()),
            Colon,
            Int(1),
            RightBracket,
            Comma,
            LeftBracket,
            Value("10".to_string()),
            Colon,
            Int(4),
            Comma,
            Value("11".to_string()),
            Colon,
            Int(1),
            RightBracket,
            RightCurlyBrace,
        ];
        assert_eq!(&actual_tokens, expected_tokens);
    }
}
