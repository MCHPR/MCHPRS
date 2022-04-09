use std::path::Path;
use std::time::Instant;

use criterion::*;
use mchprs::plot::data::PlotData;
use mchprs::plot::{PLOT_WIDTH, PlotWorld};
use mchprs::blocks::BlockPos;
use mchprs::world::storage::Chunk;
use mchprs::redpiler::{Compiler, CompilerOptions};

const START_BUTTON: BlockPos = BlockPos::new(187, 99, 115);

fn load_world(path: impl AsRef<Path>) -> PlotWorld {
    let data = PlotData::read_from_file(path);

    let chunks: Vec<Chunk> = data
        .chunk_data
        .into_iter()
        .enumerate()
        .map(|(i, c)| {
            Chunk::load(
                i as i32 / PLOT_WIDTH,
                i as i32 % PLOT_WIDTH,
                c,
            )
        })
        .collect();
    PlotWorld {
        x: 0,
        z: 0,
        chunks,
        to_be_ticked: data.pending_ticks,
        packet_senders: Vec::new(),
    }
}

fn init_compiler() -> (PlotWorld, Compiler) {
    let mut world = load_world("./benches/chungus_mandelbrot_plot");
    let mut compiler: Compiler = Default::default();

    let options = CompilerOptions::parse("-O");
    compiler.compile(&mut world, options, None, None, Vec::new());
    compiler.on_use_block(&mut world, START_BUTTON);
    (world, compiler)
}

fn chungus_mandelbrot(c: &mut Criterion) {
    let (mut world, mut compiler) = init_compiler();

    c.bench_function("chungus-mandelbrot-tick", |b| {
        b.iter(|| compiler.tick(&mut world));
    });
}

fn mandelbrot_full(_c: &mut Criterion) {
    // HACKKKKKKK, oh how I wish Criterion::filter_matches was public
    let run = std::env::args().any(|arg| "chungus-mandelbrot-full".contains(&arg));
    if !run {
        return;
    }
    
    println!("Running full chungus mandelbrot, this can take a while!");
    let (mut world, mut compiler) = init_compiler();
    let start = Instant::now();
    for _ in 0..12411975 {
        compiler.tick(&mut world);
    }
    println!("Mandelbrot benchmark completed in {:?}", start.elapsed());
}

criterion_group!(chungus, chungus_mandelbrot, mandelbrot_full);
criterion_main!(chungus);
