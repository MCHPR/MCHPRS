use anstream::println;
use clap::{Parser, Subcommand};
use line_index::{LineIndex, TextSize};
use mchprs_blocks::BlockPos;
use mchprs_redpiler::{
    passes::{build_pass_pipeline, PassPipelineBuilder, PassRegistry},
    ril::{self, RILModule, RILTest},
    CompilerInput, TaskMonitor,
};
use mchprs_schematic::{load_schematic, paste_clipboard};
use mchprs_world::testing::TestWorld;
use owo_colors::OwoColorize as _;
use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

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
}

/// Recursively search for ril files starting from `path` and collect into `paths`.
fn search_path(path: PathBuf, paths: &mut Vec<PathBuf>) {
    if path.is_dir() {
        for dir_entry in path.read_dir().unwrap() {
            let dir_entry = dir_entry.unwrap();
            search_path(dir_entry.path(), paths);
        }
    } else {
        if path.extension() == Some(OsStr::new("ril")) {
            paths.push(path);
        }
    }
}

fn find_test_root(path: &Path) -> Option<&Path> {
    if path.is_file() {
        return find_test_root(path.parent()?);
    }

    if path.join(".ril_test_root").exists() {
        Some(path)
    } else {
        find_test_root(path.parent()?)
    }
}

/// Returns true if a test was updated
fn run_test(
    test_root: &Option<&Path>,
    test_path: &Path,
    module: &RILModule,
    test: RILTest,
    update: bool,
    test_src: &mut String,
) -> bool {
    let (world, bounds) = if let Some(schem_path) = test.schematic_path {
        let schem_path = test_path.parent().unwrap().join(schem_path);
        let Ok(schematic) = load_schematic(&schem_path) else {
            eprintln!("error: failed to load schematic at path: {:?}", schem_path);
            return false;
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
            TestWorld::new(0, 0, 0),
            (BlockPos::zero(), BlockPos::zero()),
        )
    };

    let input = CompilerInput {
        world: &world,
        bounds,
    };

    let registry = PassRegistry::default();
    let pass_pipeline = match &test.options.passes {
        Some(passes) => {
            let mut builder = PassPipelineBuilder::new(&registry);
            for driver_key in passes.split(',') {
                if !builder.add_pass_by_driver_key(driver_key) {
                    eprintln!("error: failed to add pass with key: {}", driver_key);
                    return false;
                }
            }
            builder.build()
        }
        None => build_pass_pipeline(&registry, &test.options),
    };
    let monitor = Arc::new(TaskMonitor::default());
    let result_graph = pass_pipeline.run_passes(&test.options, &input, test.graph, monitor);
    let test_path = match test_root {
        Some(test_root) => test_path.strip_prefix(test_root).unwrap(),
        None => test_path,
    };
    let full_name = format!("{}:{}", test_path.with_extension("").display(), test.name);
    if !module.compare_test_result(&test.name, &result_graph) {
        let mut result_ril = String::new();
        ril::dump_graph(&mut result_ril, &result_graph, &test.name).unwrap();
        if update {
            println!("{} {}", "[UPDATED]".blue(), full_name);
            module.update_test(test_src, &test.name, &result_ril);
            return true;
        } else {
            println!("{} {}", "[FAIL]".red(), full_name);
            println!("Expected RIL:");
            println!("{}", result_ril);
        }
    } else {
        println!("{} {}", "[PASS]".green(), full_name);
    }

    false
}

fn run_tests(path: PathBuf, update: bool) {
    let mut ril_paths = Vec::new();
    let test_root = find_test_root(&path);
    if test_root.is_none() {
        eprintln!("warning: failed to find .ril_test_root");
    }
    search_path(path.clone(), &mut ril_paths);
    for path in ril_paths {
        let mut src = fs::read_to_string(&path).unwrap();
        let module = match RILModule::parse_from_string(&src) {
            Ok(module) => module,
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
                continue;
            }
        };
        let tests = module.get_tests();
        println!("Found {} RIL test modules.", tests.len());
        let mut updated = false;
        for test in tests {
            updated |= run_test(&test_root, &path, &module, test, update, &mut src);
        }
        if updated {
            fs::write(&path, src).unwrap();
        }
    }
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Command::Test { path, update } => {
            run_tests(path, update);
        }
    }
}
