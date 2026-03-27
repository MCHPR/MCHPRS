use mchprs_redpiler::passes::{build_pass_pipeline, PassRegistry};
use mchprs_redpiler::ril::ast::Global;
use mchprs_redpiler::ril::dump_graph;
use mchprs_redpiler::{CompilerInput, CompilerOptions, TaskMonitor};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{fs, process};

pub fn compile(input_path: &Path, output_path: &Option<PathBuf>, options: &CompilerOptions) {
    let ril_src = match fs::read_to_string(input_path) {
        Ok(src) => src,
        Err(err) => {
            eprintln!("error: couldn't read `{}`: {}", input_path.display(), err);
            process::exit(1);
        }
    };
    let Some(module) = crate::load_ril(input_path, &ril_src) else {
        process::exit(1);
    };

    let registry = PassRegistry::default();
    let pass_pipeline = match &options.passes {
        Some(passes) => match crate::parse_pass_pipeline(&registry, passes) {
            Some(pipeline) => pipeline,
            None => process::exit(1),
        },
        None => build_pass_pipeline(&registry, &options),
    };

    let mut result = String::new();
    for (name, global) in &module.globals {
        let (graph, schem_path) = match global {
            Global::Circuit(circuit) => (module.get_graph(circuit), None),
            Global::Schematic(schem) => (Default::default(), Some(schem.path.clone())),
            Global::Test(_) => {
                eprintln!("error: tests are not supported in this context");
                process::exit(1);
            }
            Global::BackendCircuit(_) => todo!("backend circuit compilation"),
        };

        let Some((world, bounds)) = crate::load_world(input_path, &schem_path) else {
            process::exit(1);
        };
        let input = CompilerInput {
            world: &world,
            bounds,
        };

        let monitor = Arc::new(TaskMonitor::default());
        let result_graph = pass_pipeline.run_passes(options, &input, graph, monitor);
        result.push('\n');
        dump_graph(&mut result, &result_graph, name).unwrap();
        result.push('\n');
    }

    let output_path = if let Some(path) = output_path {
        path.clone()
    } else {
        input_path.with_extension("out.ril")
    };

    if let Err(err) = fs::write(output_path, result) {
        println!("error: failed to write output file: {}", err);
        process::exit(1);
    }
}
