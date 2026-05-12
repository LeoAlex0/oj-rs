use criterion::{black_box, BatchSize, BenchmarkId, Criterion};
use solution::{data_structure::finger_tree::prelude::*, traits::prelude::*};

const SMALL: usize = 1_000;
const LARGE: usize = 100_000;

pub fn bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("finger_tree");

    for len in [SMALL, LARGE] {
        group.bench_with_input(BenchmarkId::new("split Rc", len), &len, |b, &len| {
            let tree: FingerTree<_> = (0..len).map(Value).collect();
            b.iter(|| black_box(&tree).split(|measure| measure > &Size(len >> 1)))
        });

        group.bench_with_input(BenchmarkId::new("split arena", len), &len, |b, &len| {
            let mut tree = ArenaFingerTree::with_arena_capacity(len * 2);
            for value in 0..len {
                tree.push_back_mut(Value(value));
            }
            b.iter(|| black_box(&tree).split(|measure| measure > &Size(len >> 1)))
        });

        group.bench_with_input(
            BenchmarkId::new("split Box consuming", len),
            &len,
            |b, &len| {
                let tree: BoxFingerTree<_> = (0..len).map(Value).collect();
                b.iter_batched(
                    || tree.clone(),
                    |tree| black_box(tree).into_split(|measure| measure > &Size(len >> 1)),
                    BatchSize::SmallInput,
                )
            },
        );
    }

    for (left_len, right_len) in [
        (SMALL, SMALL),
        (SMALL, LARGE),
        (LARGE, SMALL),
        (LARGE, LARGE),
    ] {
        group.bench_function(
            BenchmarkId::new("concat Rc", format!("{left_len}<>{right_len}")),
            |b| {
                let tree_l: FingerTree<_> = (0..left_len).map(Value).collect();
                let tree_r: FingerTree<_> = (0..right_len).map(Value).collect();
                b.iter(|| black_box(&tree_l).concat(black_box(&tree_r)))
            },
        );

        group.bench_function(
            BenchmarkId::new("concat arena", format!("{left_len}<>{right_len}")),
            |b| {
                let arena = ArenaFamily::default();
                let mut tree_l = FingerTree::new_in(arena.clone());
                let mut tree_r = FingerTree::new_in(arena);
                for value in 0..left_len {
                    tree_l.push_back_mut(Value(value));
                }
                for value in 0..right_len {
                    tree_r.push_back_mut(Value(value));
                }
                b.iter(|| black_box(&tree_l).concat(black_box(&tree_r)))
            },
        );

        group.bench_function(
            BenchmarkId::new("concat Box consuming", format!("{left_len}<>{right_len}")),
            |b| {
                let tree_l: BoxFingerTree<_> = (0..left_len).map(Value).collect();
                let tree_r: BoxFingerTree<_> = (0..right_len).map(Value).collect();
                b.iter_batched(
                    || (tree_l.clone(), tree_r.clone()),
                    |(tree_l, tree_r)| black_box(tree_l).into_concat(black_box(tree_r)),
                    BatchSize::SmallInput,
                )
            },
        );
    }

    group.finish();
}
