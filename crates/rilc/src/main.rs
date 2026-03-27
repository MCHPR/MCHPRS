use clap::{Parser, Subcommand};
use line_index::{LineIndex, TextSize};
use mchprs_blocks::BlockPos;
use mchprs_redpiler::{
    passes::{PassPipeline, PassPipelineBuilder, PassRegistry},
    ril::RILModule,
    CompilerOptions,
};
use mchprs_schematic::{load_schematic, paste_clipboard};
use mchprs_world::testing::TestWorld;
use std::path::{Path, PathBuf};

mod compile;
mod test;

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Test {
        /// Path to the test or test directory
        path: PathBuf,

        #[arg(long)]
        update: bool,
    },
    Compile {
        /// Path to the input file
        input_path: PathBuf,

        /// Path to the output file
        #[arg(short = 'o')]
        output_path: Option<PathBuf>,

        /// Enable optimization passes which may significantly increase compile times.
        #[arg(long, short = 'O')]
        optimize: bool,
        /// Export the graph to a binary format. See the [`redpiler_graph`] crate.
        #[arg(long, short)]
        export: bool,
        /// Only flush lamp, button, lever, pressure plate, or trapdoor updates.
        #[arg(long, short)]
        io_only: bool,
        /// Consider a redstone dot to be an output block (for color screens)
        #[arg(long, short = 'd')]
        wire_dot_out: bool,
        /// Print out the RIL circuit after every redpiler pass
        #[arg(long)]
        print_after_all: bool,
        /// A comma seperated list of passes to run. This can only be used by the rilc driver.
        #[arg(long)]
        passes: Option<String>,
    },
}

fn load_world(
    ril_file_path: &Path,
    schem_path: &Option<String>,
) -> Option<(TestWorld, (BlockPos, BlockPos))> {
    Some(if let Some(schem_path) = schem_path {
        let schem_path = ril_file_path.parent().unwrap().join(schem_path);
        let Ok(schematic) = load_schematic(&schem_path) else {
            eprintln!("error: failed to load schematic at path: {:?}", schem_path);
            return None;
        };
        let x_size = schematic.size_x.div_ceil(16) as i32;
        let y_size = schematic.size_y.div_ceil(16) as i32;
        let z_size = schematic.size_z.div_ceil(16) as i32;
        let mut world = TestWorld::new(x_size, y_size, z_size);
        paste_clipboard(&mut world, &schematic, BlockPos::zero(), true);

        let bounds = (
            BlockPos::zero(),
            BlockPos::new(
                schematic.size_x as i32 - 1,
                schematic.size_y as i32 - 1,
                schematic.size_z as i32 - 1,
            ),
        );

        (world, bounds)
    } else {
        (
            TestWorld::new(1, 1, 1),
            (BlockPos::zero(), BlockPos::zero()),
        )
    })
}

fn load_ril(path: &Path, src: &str) -> Option<RILModule> {
    match RILModule::parse_from_string(&src) {
        Ok(module) => Some(module),
        Err(err) => {
            eprintln!("error: failed to load RIL module at path: {:?}", &path);
            let file_name = path.file_name().unwrap();
            let line_index = LineIndex::new(&src);
            let pos = TextSize::new(err.pos as u32);
            let line_col = line_index.line_col(pos);
            eprintln!(
                "{}:{}:{} {}",
                file_name.display(),
                line_col.line + 1,
                line_col.col + 1,
                err.message
            );
            None
        }
    }
}

fn parse_pass_pipeline<'p>(
    registry: &'p PassRegistry<TestWorld>,
    passes: &str,
) -> Option<PassPipeline<'p, TestWorld>> {
    let mut builder = PassPipelineBuilder::new(&registry);
    for driver_key in passes.split(',') {
        if !builder.add_pass_by_driver_key(driver_key) {
            eprintln!("error: failed to add pass with key: {}", driver_key);
            return None;
        }
    }
    Some(builder.build())
}

fn main() {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match cli.command {
        Command::Test { path, update } => {
            test::run_tests(path, update);
        }
        Command::Compile {
            input_path,
            output_path,
            optimize,
            export,
            io_only,
            wire_dot_out,
            print_after_all,
            passes,
        } => {
            let options = CompilerOptions {
                optimize,
                export,
                io_only,
                update: false,
                export_dot_graph: false,
                wire_dot_out,
                print_after_all,
                print_before_backend: false,
                backend_variant: Default::default(),
                passes,
            };
            compile::compile(&input_path, &output_path, &options);
        }
    }
}
