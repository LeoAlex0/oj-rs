use std::{
    array,
    collections::{HashMap, HashSet},
};

use criterion::{black_box, Criterion};
use solution::{
    data_structure::seg_tree::prelude::*,
    traits::prelude::{Identity, Monoid, Semigroup},
};

type Value = Option<Identity<u8>>;

#[derive(Clone)]
struct TransposeU8(HashMap<u8, u8>);

impl Semigroup for TransposeU8 {
    fn merge(self, other: Self) -> Self {
        let interested_keys = self.0.keys().chain(other.0.keys()).collect::<HashSet<_>>();

        Self(
            interested_keys
                .into_iter()
                .filter_map(|left| {
                    let middle = other.0.get(left).unwrap_or(left);
                    let right = self.0.get(middle).unwrap_or(middle);

                    if *left == *right {
                        None
                    } else {
                        Some((*left, *right))
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
        Self(HashMap::new())
    }
}

impl Monoid for TransposeU8 {
    fn empty() -> Self {
        Self::identity()
    }
}

impl Applier<Value> for TransposeU8 {
    fn apply(&self, to: Value) -> Value {
        to.map(|to| Identity(*self.0.get(&to.0).unwrap_or(&to.0)))
    }
}

pub fn bench(c: &mut Criterion) {
    const N: usize = 200_000;

    let mut group = c.benchmark_group("seg_tree");

    group.bench_function("build large", |b| {
        let array: [u8; N] = array::from_fn(|i| i as u8 & 3);
        b.iter(|| {
            let tree: SegTree<_, TransposeU8> = SegTree::build(N, |i| Some(Identity(array[i])));
            black_box(tree);
        })
    });

    group.bench_function("large update", |b| {
        let array: [u8; N] = array::from_fn(|_| 1u8);
        let tree: SegTree<_, TransposeU8> = SegTree::build(N, |i| Some(Identity(array[i])));
        b.iter(|| {
            black_box(black_box(tree.clone()).apply(
                black_box(1..N - 1),
                black_box(TransposeU8::empty().assign(1, 2)),
            ))
        })
    });

    group.finish();
}
