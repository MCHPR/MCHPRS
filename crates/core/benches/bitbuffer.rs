use criterion::*;
use mchprs_core::world::storage::BitBuffer;

fn bitbuffer_read(c: &mut Criterion) {
    let buffer = BitBuffer::create(black_box(13), 8192);
    c.bench_function("bitbuffer-read", move |b| {
        b.iter(|| buffer.get_entry(black_box(42)))
    });
}

fn bitbuffer_write(c: &mut Criterion) {
    let mut buffer = BitBuffer::create(black_box(13), 8192);
    c.bench_function("bitbuffer-write", move |b| {
        b.iter(|| buffer.set_entry(black_box(42), black_box(13)))
    });
}

criterion_group!(bitbuffer, bitbuffer_write, bitbuffer_read);
criterion_main!(bitbuffer);
