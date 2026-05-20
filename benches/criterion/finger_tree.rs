use criterion::{black_box, BenchmarkId, Criterion};
use solution::{
    data_structure::{finger_tree::prelude::*, ref_store::ArenaStoreFactory},
    traits::prelude::*,
};

const SMALL: usize = 1_000;
const LARGE: usize = 100_000;
const CHUNK: usize = chunk_capacity_for_bytes::<Value<usize>>(CACHE_LINE_BYTES);

fn arena_capacity(len: usize) -> usize {
    len * 2 + 1024
}

fn chunk_count(len: usize) -> usize {
    len.div_ceil(CHUNK)
}

fn chunk_arena_capacity(len: usize) -> usize {
    arena_capacity(chunk_count(len))
}

fn scratch_capacity(len: usize) -> usize {
    len.checked_ilog2().unwrap_or(0) as usize * 64 + 1024
}

pub fn bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("finger_tree");

    for len in [SMALL, LARGE] {
        group.bench_with_input(BenchmarkId::new("split Rc", len), &len, |b, &len| {
            let mut refs = FingerTreeStore::default();
            let tree: FingerTree<_> = FingerTree::from_iter_in(&mut refs, (0..len).map(Value));
            b.iter(|| black_box(&tree).split(&mut refs, |measure| measure > &Size(len >> 1)))
        });

        group.bench_with_input(BenchmarkId::new("split arena", len), &len, |b, &len| {
            ArenaStoreFactory::scoped(arena_capacity(len), |factory| {
                let mut base: FingerTreeStore<Chunk<Value<usize>, 1>, _> =
                    FingerTreeStore::new(factory);
                let tree: FingerTree<_, 1, _> =
                    FingerTree::from_iter_in(&mut base, (0..len).map(Value));
                b.iter(|| {
                    base.layered(scratch_capacity(len), |mut scratch| {
                        let tree = scratch.from_base(black_box(&tree));
                        let result = black_box(&tree)
                            .split(&mut scratch, |measure| measure > &Size(len >> 1));
                        black_box(result.as_ref().map(|(_, value, _)| value.clone()))
                    })
                })
            })
        });

        group.bench_with_input(
            BenchmarkId::new("split chunked Rc", len),
            &len,
            |b, &len| {
                let mut refs = FingerTreeStore::default();
                let tree: FingerTree<_, CHUNK> =
                    FingerTree::from_iter_in(&mut refs, (0..len).map(Value));
                b.iter(|| black_box(&tree).split(&mut refs, |measure| measure > &Size(len >> 1)))
            },
        );

        group.bench_with_input(
            BenchmarkId::new("split chunked arena", len),
            &len,
            |b, &len| {
                ArenaStoreFactory::scoped(chunk_arena_capacity(len), |factory| {
                    let mut base: FingerTreeStore<Chunk<Value<usize>, CHUNK>, _> =
                        FingerTreeStore::new(factory);
                    let tree: FingerTree<_, CHUNK, _> =
                        FingerTree::from_iter_in(&mut base, (0..len).map(Value));
                    b.iter(|| {
                        base.layered(scratch_capacity(chunk_count(len)), |mut scratch| {
                            let tree = scratch.from_base(black_box(&tree));
                            let result = black_box(&tree)
                                .split(&mut scratch, |measure| measure > &Size(len >> 1));
                            black_box(result.as_ref().map(|(_, value, _)| value.clone()))
                        })
                    })
                })
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
                let mut refs = FingerTreeStore::default();
                let tree_l: FingerTree<_> =
                    FingerTree::from_iter_in(&mut refs, (0..left_len).map(Value));
                let tree_r: FingerTree<_> =
                    FingerTree::from_iter_in(&mut refs, (0..right_len).map(Value));
                b.iter(|| black_box(&tree_l).concat(&mut refs, black_box(&tree_r)))
            },
        );

        group.bench_function(
            BenchmarkId::new("concat arena", format!("{left_len}<>{right_len}")),
            |b| {
                ArenaStoreFactory::scoped(arena_capacity(left_len + right_len), |factory| {
                    let mut base: FingerTreeStore<Chunk<Value<usize>, 1>, _> =
                        FingerTreeStore::new(factory);
                    let tree_l: FingerTree<_, 1, _> =
                        FingerTree::from_iter_in(&mut base, (0..left_len).map(Value));
                    let tree_r: FingerTree<_, 1, _> =
                        FingerTree::from_iter_in(&mut base, (0..right_len).map(Value));
                    b.iter(|| {
                        base.layered(scratch_capacity(left_len + right_len), |mut scratch| {
                            let tree_l = scratch.from_base(black_box(&tree_l));
                            let tree_r = scratch.from_base(black_box(&tree_r));
                            let result =
                                black_box(&tree_l).concat(&mut scratch, black_box(&tree_r));
                            black_box(result.measure())
                        })
                    })
                })
            },
        );

        group.bench_function(
            BenchmarkId::new("concat chunked Rc", format!("{left_len}<>{right_len}")),
            |b| {
                let mut refs = FingerTreeStore::default();
                let tree_l: FingerTree<_, CHUNK> =
                    FingerTree::from_iter_in(&mut refs, (0..left_len).map(Value));
                let tree_r: FingerTree<_, CHUNK> =
                    FingerTree::from_iter_in(&mut refs, (0..right_len).map(Value));
                b.iter(|| black_box(&tree_l).concat(&mut refs, black_box(&tree_r)))
            },
        );

        group.bench_function(
            BenchmarkId::new("concat chunked arena", format!("{left_len}<>{right_len}")),
            |b| {
                ArenaStoreFactory::scoped(chunk_arena_capacity(left_len + right_len), |factory| {
                    let mut base: FingerTreeStore<Chunk<Value<usize>, CHUNK>, _> =
                        FingerTreeStore::new(factory);
                    let tree_l: FingerTree<_, CHUNK, _> =
                        FingerTree::from_iter_in(&mut base, (0..left_len).map(Value));
                    let tree_r: FingerTree<_, CHUNK, _> =
                        FingerTree::from_iter_in(&mut base, (0..right_len).map(Value));
                    b.iter(|| {
                        base.layered(
                            scratch_capacity(chunk_count(left_len + right_len)),
                            |mut scratch| {
                                let tree_l = scratch.from_base(black_box(&tree_l));
                                let tree_r = scratch.from_base(black_box(&tree_r));
                                let result =
                                    black_box(&tree_l).concat(&mut scratch, black_box(&tree_r));
                                black_box(result.measure())
                            },
                        )
                    })
                })
            },
        );
    }

    group.finish();
}
