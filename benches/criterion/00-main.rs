mod finger_tree;
mod seg_tree;

use criterion::{criterion_group, criterion_main, Criterion};

fn benchmark(c: &mut Criterion) {
    finger_tree::bench(c);
    seg_tree::bench(c);
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
