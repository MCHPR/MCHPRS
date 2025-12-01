use mchprs_blocks::{blocks::Block, BlockPos};
use mchprs_core::plot::{PlotWorld, PLOT_WIDTH};
use mchprs_redpiler::{BackendVariant, Compiler, CompilerOptions};
use mchprs_save_data::plot_data::PlotData;
use mchprs_world::World;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};

fn load_plot(path: impl AsRef<std::path::Path>) -> PlotWorld {
    let plot_data = PlotData::load_from_file(path).unwrap();
    let chunks = plot_data
        .chunk_data
        .into_iter()
        .enumerate()
        .map(|(i, c)| c.load(i as i32 / PLOT_WIDTH, i as i32 % PLOT_WIDTH));
    PlotWorld {
        x: 0,
        z: 0,
        chunks: chunks.collect(),
        to_be_ticked: plot_data.pending_ticks,
        packet_senders: Vec::new(),
    }
}

/// Runs the simulation until the system reaches a stable state.
fn run_backend(compiler: &mut Compiler) {
    while compiler.has_pending_ticks() {
        compiler.tick();
    }
}

fn repeater_grid_bench(c: &mut Criterion) {
    let world = load_plot("benches/plots/repeater_grid");

    let start_button_pos = BlockPos::new(16, 1, 16);
    assert!(matches!(
        world.get_block(start_button_pos),
        Block::StoneButton { .. }
    ));

    let setup_and_compile = || {
        let compile_options = CompilerOptions {
            backend_variant: BackendVariant::Direct,
            ..Default::default()
        };
        let mut compiler = Compiler::default();
        compiler.compile(
            &world,
            world.get_corners(),
            compile_options.clone(),
            Default::default(),
            Default::default(),
        );
        compiler.on_use_block(start_button_pos);
        compiler
    };

    c.bench_function("repeater_grid", |b| {
        b.iter_batched_ref(setup_and_compile, run_backend, BatchSize::LargeInput);
    });
}

criterion_group!(benches, repeater_grid_bench);
criterion_main!(benches);
