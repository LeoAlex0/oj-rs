use std::{
    array,
    collections::{HashMap, HashSet},
};

use criterion::{black_box, Criterion};
use solution::{
    data_structure::{ref_store::ArenaStoreFactory, seg_tree::prelude::*},
    traits::prelude::{Identity, Monoid, Semigroup},
};

type Value = Option<Identity<u8>>;
const LEAF_BLOCK_BYTES: usize = SEG_TREE_CACHE_LINE_BYTES;
const LEAF_BLOCK: usize = seg_leaf_block_capacity_for_bytes::<Value>(LEAF_BLOCK_BYTES);

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

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Applier<Value> for TransposeU8 {
    fn apply(&self, to: &mut Value) {
        if let Some(to) = to {
            to.0 = *self.0.get(&to.0).unwrap_or(&to.0);
        }
    }
}

fn random_ranges(len: usize, count: usize) -> Vec<std::ops::Range<usize>> {
    let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
    let mut next = || {
        seed ^= seed << 7;
        seed ^= seed >> 9;
        seed ^= seed << 8;
        seed as usize
    };

    (0..count)
        .map(|_| {
            let a = next() % len;
            let b = next() % len;
            let (start, end) = if a <= b { (a, b) } else { (b, a) };
            start..end + 1
        })
        .collect()
}

pub fn bench(c: &mut Criterion) {
    const N: usize = 200_000;
    const QUERY_COUNT: usize = 1 << 15;

    let mut group = c.benchmark_group("seg_tree");

    group.bench_function("build large", |b| {
        let array: [u8; N] = array::from_fn(|i| i as u8 & 3);
        b.iter(|| {
            let tree: SegTree<_, TransposeU8, LEAF_BLOCK> =
                SegTree::build(N, |i| Some(Identity(array[i])));
            black_box(tree);
        })
    });

    group.bench_function("build large arena", |b| {
        let array: [u8; N] = array::from_fn(|i| i as u8 & 3);
        b.iter(|| {
            ArenaStoreFactory::scoped(N * 2, |factory| {
                let mut arena: SegTreeStore<Value, TransposeU8, _, LEAF_BLOCK> =
                    SegTreeStore::new(factory);
                let tree = SegTree::build_in(&mut arena, N, |i| Some(Identity(array[i])));
                black_box(tree);
            })
        })
    });

    group.bench_function("large update", |b| {
        let array: [u8; N] = array::from_fn(|_| 1u8);
        let mut store = SegTreeStore::default();
        let tree: SegTree<_, TransposeU8, LEAF_BLOCK> =
            SegTree::build_in(&mut store, N, |i| Some(Identity(array[i])));
        b.iter(|| {
            let modifier = black_box(TransposeU8::empty().assign(1, 2));
            black_box(black_box(tree.clone()).apply(&mut store, black_box(1..N - 1), &modifier))
        })
    });

    group.bench_function("large query random", |b| {
        let array: [u8; N] = array::from_fn(|i| i as u8 & 3);
        let ranges = random_ranges(N, QUERY_COUNT);
        let mut index = 0;
        let mut store = SegTreeStore::default();
        let tree: SegTree<_, TransposeU8, LEAF_BLOCK> =
            SegTree::build_in(&mut store, N, |i| Some(Identity(array[i])));
        b.iter(|| {
            let range = &ranges[index & (QUERY_COUNT - 1)];
            index += 1;
            black_box(tree.query(&store, black_box(range.clone())))
        })
    });

    group.bench_function("large query random arena", |b| {
        let array: [u8; N] = array::from_fn(|i| i as u8 & 3);
        let ranges = random_ranges(N, QUERY_COUNT);
        ArenaStoreFactory::scoped(N * 2, |factory| {
            let mut index = 0;
            let mut store: SegTreeStore<Value, TransposeU8, _, LEAF_BLOCK> =
                SegTreeStore::new(factory);
            let tree = SegTree::build_in(&mut store, N, |i| Some(Identity(array[i])));
            b.iter(|| {
                let range = &ranges[index & (QUERY_COUNT - 1)];
                index += 1;
                black_box(tree.query(&store, black_box(range.clone())))
            })
        })
    });

    group.bench_function("large update mut arena", |b| {
        let array: [u8; N] = array::from_fn(|_| 1u8);
        ArenaStoreFactory::scoped(N * 2, |factory| {
            let mut store: SegTreeStore<Value, TransposeU8, _, LEAF_BLOCK> =
                SegTreeStore::new(factory);
            let mut tree = SegTree::build_in(&mut store, N, |i| Some(Identity(array[i])));
            b.iter(|| {
                let modifier = black_box(TransposeU8::empty().assign(1, 2));
                tree.apply_mut(&mut store, black_box(1..N - 1), &modifier);
                black_box(&tree);
            })
        })
    });

    group.finish();
}
