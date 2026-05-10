use criterion::{black_box, BenchmarkId, Criterion};
use solution::{
    data_structure::finger_tree::{prelude::*, RcRef},
    traits::prelude::*,
};

const SMALL: usize = 1_000;
const LARGE: usize = 100_000;

pub fn bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("finger_tree");

    for len in [SMALL, LARGE] {
        group.bench_with_input(BenchmarkId::new("split RcRef", len), &len, |b, &len| {
            let tree: FingerTree<_, RcRef> = (0..len).map(Value).collect();
            b.iter(|| black_box(&tree).split(|measure| measure > &Size(len >> 1)))
        });
    }

    for (left_len, right_len) in [
        (SMALL, SMALL),
        (SMALL, LARGE),
        (LARGE, SMALL),
        (LARGE, LARGE),
    ] {
        group.bench_function(
            BenchmarkId::new("concat RcRef", format!("{left_len}<>{right_len}")),
            |b| {
                let tree_l: FingerTree<_, RcRef> = (0..left_len).map(Value).collect();
                let tree_r: FingerTree<_, RcRef> = (0..right_len).map(Value).collect();
                b.iter(|| black_box(&tree_l).concat(black_box(&tree_r)))
            },
        );
    }

    group.finish();
}
