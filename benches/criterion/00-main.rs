mod cf_911g;
mod finger_tree;
mod seg_tree;

use criterion::{criterion_group, criterion_main, Criterion};

fn benchmark(c: &mut Criterion) {
    cf_911g::bench(c);
    finger_tree::bench(c);
    seg_tree::bench(c);
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
