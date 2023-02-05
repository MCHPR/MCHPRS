use std::path::Path;
use std::time::Instant;

use criterion::*;
use mchprs_blocks::BlockPos;
use mchprs_core::plot::{PlotWorld, PLOT_WIDTH};
use mchprs_core::redpiler::{Compiler, CompilerOptions};
use mchprs_core::world::storage::Chunk;
use mchprs_save_data::plot_data::PlotData;

const START_BUTTON: BlockPos = BlockPos::new(187, 99, 115);

fn load_world(path: impl AsRef<Path>) -> PlotWorld {
    let data = PlotData::load_from_file(path).unwrap();

    let chunks: Vec<Chunk> = data
        .chunk_data
        .into_iter()
        .enumerate()
        .map(|(i, c)| Chunk::load(i as i32 / PLOT_WIDTH, i as i32 % PLOT_WIDTH, c))
        .collect();
    PlotWorld {
        x: 0,
        z: 0,
        chunks,
        to_be_ticked: data.pending_ticks,
        packet_senders: Vec::new(),
    }
}

fn init_compiler() -> Compiler {
    let mut world = load_world("./benches/chungus_mandelbrot_plot");
    let mut compiler: Compiler = Default::default();

    let options = CompilerOptions::parse("-O");
    let bounds = world.get_corners();
    compiler.compile(&mut world, bounds, options, Vec::new());
    compiler.on_use_block(START_BUTTON);
    compiler
}

fn chungus_mandelbrot(c: &mut Criterion) {
    let mut compiler = init_compiler();

    c.bench_function("chungus-mandelbrot-tick", |b| {
        b.iter(|| compiler.tick());
    });
}

fn mandelbrot_full(_c: &mut Criterion) {
    // HACKKKKKKK, oh how I wish Criterion::filter_matches was public
    let run = std::env::args().any(|arg| "chungus-mandelbrot-full".contains(&arg));
    if !run {
        return;
    }

    println!("Running full chungus mandelbrot, this can take a while!");
    let mut compiler = init_compiler();
    let start = Instant::now();
    for _ in 0..12411975 {
        compiler.tick();
    }
    println!("Mandelbrot benchmark completed in {:?}", start.elapsed());
}

criterion_group!(chungus, chungus_mandelbrot, mandelbrot_full);
criterion_main!(chungus);
