use std::{
    array,
    collections::{HashMap, HashSet},
};

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use solution::traits::monoid::Monoid;

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);

#[derive(Clone)]
struct TransposeU8(HashMap<u8, u8>);
impl solution::traits::semigroup::Semigroup for TransposeU8 {
    fn merge(self, other: Self) -> Self {
        // Self(other.0.map(|it| self.0[it as usize]))
        let intrested_keys = self.0.keys().chain(other.0.keys()).collect::<HashSet<_>>();

        Self(
            intrested_keys
                .into_iter()
                .filter_map(|l| {
                    let m = other.0.get(l).unwrap_or(l);
                    let r = self.0.get(m).unwrap_or(m);

                    if *l == *r {
                        None
                    } else {
                        Some((*l, *r))
                    }
                })
                .collect(),
        )
    }
}

impl TransposeU8 {
    fn assign(mut self, x: u8, y: u8) -> Self {
        self.0.insert(x, y);
        self
    }

    fn identity() -> Self {
        Self(HashMap::new()) // identity transpose
    }
}

impl solution::traits::monoid::Monoid for TransposeU8 {
    fn empty() -> Self {
        Self::identity()
    }
}

impl solution::data_structure::seg_tree::Applier<Option<solution::traits::semigroup::Identity<u8>>>
    for TransposeU8
{
    fn apply(
        &self,
        to: Option<solution::traits::semigroup::Identity<u8>>,
    ) -> Option<solution::traits::semigroup::Identity<u8>> {
        to.map(|to| solution::traits::semigroup::Identity(*self.0.get(&to.0).unwrap_or(&to.0)))
    }
}

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("build large segtree", |b| {
        const N: usize = 200000;
        let array: [u8; N] = array::from_fn(|i| i as u8 & 3);
        b.iter(|| {
            let tree: solution::data_structure::seg_tree::SegTree<_, TransposeU8> =
                solution::data_structure::seg_tree::SegTree::build(N, |i| {
                    Some(solution::traits::semigroup::Identity(array[i]))
                });
            black_box(tree);
        })
    });

    c.bench_function("segtree large update", |b| {
        const N: usize = 200000;
        let array: [u8; N] = array::from_fn(|_| 1u8);
        let tree: solution::data_structure::seg_tree::SegTree<_, TransposeU8> =
            solution::data_structure::seg_tree::SegTree::build(N, |i| {
                Some(solution::traits::semigroup::Identity(array[i]))
            });
        b.iter(|| {
            black_box(black_box(tree.clone()).apply(
                black_box(1..N - 1),
                black_box(TransposeU8::empty().assign(1, 2)),
            ))
        })
    });
}
