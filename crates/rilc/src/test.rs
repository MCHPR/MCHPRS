use anstream::println;
use mchprs_redpiler::{
    passes::{build_pass_pipeline, PassRegistry},
    ril::{self, RILModule, RILTest},
    string_replacer::StringReplacer,
    CompilerInput, TaskMonitor,
};
use owo_colors::OwoColorize as _;
use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

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
    test_src: &mut StringReplacer,
) -> bool {
    let Some((world, bounds)) = crate::load_world(test_path, &test.schematic_path) else {
        return false;
    };

    let input = CompilerInput {
        world: &world,
        bounds,
        pending_ticks: &[]
    };

    let registry = PassRegistry::default();
    let pass_pipeline = match &test.options.passes {
        Some(passes) => match crate::parse_pass_pipeline(&registry, passes) {
            Some(pipeline) => pipeline,
            None => return false,
        },
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
        } else {
            println!("{} {}", "[FAIL]".red(), full_name);
            println!("Expected RIL:");
            println!("{}", result_ril);
        }
        false
    } else {
        println!("{} {}", "[PASS]".green(), full_name);
        true
    }
}

pub fn run_tests(path: PathBuf, update: bool) {
    let mut ril_paths = Vec::new();
    let test_root = find_test_root(&path);
    if test_root.is_none() {
        eprintln!("warning: failed to find .ril_test_root");
    }
    search_path(path.clone(), &mut ril_paths);
    println!("Found {} RIL test modules.", ril_paths.len());

    let mut num_passed = 0;
    let mut num_failed = 0;
    for path in ril_paths {
        let src = fs::read_to_string(&path).unwrap();
        let Some(module) = crate::load_ril(&path, &src) else {
            num_failed += 1;
            continue;
        };
        let tests = module.get_tests();
        let mut updated = false;

        let mut src = StringReplacer::new(&src);

        for test in tests {
            let result = run_test(&test_root, &path, &module, test, update, &mut src);
            if result {
                num_passed += 1;
            } else {
                num_failed += 1;
                if update {
                    updated = true;
                }
            }
        }
        if updated {
            fs::write(&path, src.finish().as_ref()).unwrap();
        }
    }

    if update {
        println!("{} tests updated.", num_failed);
    } else {
        println!("{}/{} tests passed.", num_passed, num_passed + num_failed)
    }
}
