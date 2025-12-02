use mchprs_blocks::{blocks::Block, BlockPos};
use mchprs_core::plot::{PlotWorld, PLOT_WIDTH};
use mchprs_redpiler::{Compiler, CompilerOptions};
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

const PLOTS: [(&str, BlockPos); 4] = [
    ("adder", BlockPos::new(30, 3, 46)),
    ("call_stack", BlockPos::new(29, 0, 10)),
    ("long_wire", BlockPos::new(31, 1, 16)),
    ("repeater_grid", BlockPos::new(16, 1, 16)),
];

fn bench_compile(c: &mut Criterion) {
    let compile_options = CompilerOptions::default();
    let mut group = c.benchmark_group("compile_unoptimized");
    for (plot_name, _) in PLOTS {
        let world = load_plot(format!("benches/plots/{plot_name}"));
        let bounds = world.get_corners();
        group.bench_function(plot_name, |bencher| {
            bencher.iter_batched_ref(
                Compiler::default,
                |compiler: &mut Compiler| {
                    compiler.compile(
                        &world,
                        bounds,
                        compile_options,
                        Default::default(),
                        Default::default(),
                    );
                },
                BatchSize::LargeInput,
            );
        });
    }
}

fn bench_tick(c: &mut Criterion) {
    let compile_options = CompilerOptions::default();
    let mut group = c.benchmark_group("tick_unoptimized");
    for (plot_name, start_trigger_pos) in PLOTS {
        let world = load_plot(format!("benches/plots/{plot_name}"));
        let bounds = world.get_corners();
        assert!(matches!(
            world.get_block(start_trigger_pos),
            Block::StoneButton { .. } | Block::Lever { .. }
        ));
        let setup = || {
            let mut compiler = Compiler::default();
            compiler.compile(
                &world,
                bounds,
                compile_options,
                Default::default(),
                Default::default(),
            );
            compiler.on_use_block(start_trigger_pos);
            compiler
        };
        group.bench_function(plot_name, |bencher| {
            bencher.iter_batched_ref(
                setup,
                |compiler: &mut Compiler| {
                    while compiler.has_pending_ticks() {
                        compiler.tick();
                    }
                },
                BatchSize::LargeInput,
            );
        });
    }
}

criterion_group!(benches, bench_compile, bench_tick);
criterion_main!(benches);
