use crate::data_structure::ref_store::{
    ArcStoreFactory, ArenaStoreFactory, ConstArenaStoreFactory,
};
use crate::data_structure::seg_tree::prelude::*;
use crate::traits::prelude::*;

#[derive(Clone)]
struct Add(i64);

impl Semigroup for Add {
    fn merge(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl Monoid for Add {
    fn empty() -> Self {
        Self(0)
    }

    fn is_empty(&self) -> bool {
        self.0 == 0
    }
}

impl Applier<(Sum<i64>, Size)> for Add {
    fn apply(&self, (sum, size): &mut (Sum<i64>, Size)) {
        sum.0 += self.0 * size.0 as i64;
    }
}

#[test]
fn in_place_seg_tree_range_update_and_query() {
    ArenaStoreFactory::scoped(128, |factory| {
        let mut arena: SegTreeStore<(Sum<i64>, Size), Add, _> = SegTreeStore::new(factory);
        let mut tree = SegTree::build_in(&mut arena, 8, |i| (Sum(i as i64), Size::default()));

        assert_eq!(tree.query(&arena, 0..8).0 .0, 28);
        assert_eq!(tree.query(&arena, 2..5).0 .0, 9);

        tree.apply_mut(&mut arena, 1..7, &Add(10));

        assert_eq!(tree.query(&arena, 0..8).0 .0, 88);
        assert_eq!(tree.query(&arena, 2..5).0 .0, 39);

        let values = tree.iter(&arena).map(|it| it.0 .0).collect::<Vec<_>>();
        assert_eq!(values, vec![0, 11, 12, 13, 14, 15, 16, 7]);
    });
}

#[test]
fn arena_seg_tree_keeps_persistent_versions() {
    ArenaStoreFactory::scoped(128, |factory| {
        let mut arena: SegTreeStore<(Sum<i64>, Size), Add, _> = SegTreeStore::new(factory);
        let tree = SegTree::build_in(&mut arena, 8, |i| (Sum(i as i64), Size::default()));
        let updated = tree.apply(&mut arena, 1..7, &Add(10));

        assert_eq!(tree.query(&arena, 0..8).0 .0, 28);
        assert_eq!(updated.query(&arena, 0..8).0 .0, 88);
        assert_eq!(updated.query(&arena, 2..5).0 .0, 39);
    });
}

#[test]
fn const_arena_seg_tree_builds_with_static_capacity() {
    ConstArenaStoreFactory::<128>::scoped(|factory| {
        let mut arena: SegTreeStore<(Sum<i64>, Size), Add, _> = SegTreeStore::new(factory);
        let tree = SegTree::build_in(&mut arena, 8, |i| (Sum(i as i64), Size::default()));

        assert_eq!(tree.query(&arena, 0..8).0 .0, 28);
        assert_eq!(tree.query(&arena, 2..5).0 .0, 9);
    });
}

#[test]
fn seg_tree_leaf_block_capacity_is_const_calculated() {
    const BLOCK: usize = seg_leaf_block_capacity_for_bytes::<(Sum<i64>, Size)>(
        SEG_TREE_CACHE_LINE_BYTES + SEG_TREE_CACHE_LINE_BYTES / 2,
    );

    assert_eq!(BLOCK, 5);
    assert_eq!(
        std::mem::size_of::<SegBlock<(Sum<i64>, Size), BLOCK>>(),
        SEG_TREE_CACHE_LINE_BYTES + SEG_TREE_CACHE_LINE_BYTES / 2
    );

    ArenaStoreFactory::scoped(128, |factory| {
        let mut arena: SegTreeStore<(Sum<i64>, Size), Add, _, BLOCK> = SegTreeStore::new(factory);
        let tree: SegTree<_, Add, BLOCK, _> =
            SegTree::build_in(&mut arena, 8, |i| (Sum(i as i64), Size::default()));

        assert_eq!(tree.query(&arena, 0..8).0 .0, 28);
        assert_eq!(tree.query(&arena, 2..5).0 .0, 9);
    });
}

#[test]
fn layered_arena_seg_tree_allocates_updates_in_scratch() {
    ArenaStoreFactory::scoped(128, |factory| {
        let mut base: SegTreeStore<(Sum<i64>, Size), Add, _> = SegTreeStore::new(factory);
        let tree = SegTree::build_in(&mut base, 8, |i| (Sum(i as i64), Size::default()));

        base.layered(64, |mut scratch| {
            let tree = scratch.from_base(&tree);
            let updated = tree.apply(&mut scratch, 1..7, &Add(10));

            assert_eq!(tree.query(&scratch, 0..8).0 .0, 28);
            assert_eq!(updated.query(&scratch, 0..8).0 .0, 88);
            assert_eq!(updated.query(&scratch, 2..5).0 .0, 39);
        });

        assert_eq!(tree.query(&base, 0..8).0 .0, 28);
    });
}

#[test]
fn arc_seg_tree_range_update_and_query() {
    let mut store: SegTreeStore<(Sum<i64>, Size), Add, ArcStoreFactory> =
        SegTreeStore::new(ArcStoreFactory);
    let tree = SegTree::build_in(&mut store, 8, |i| (Sum(i as i64), Size::default()));

    let updated = tree.apply(&mut store, 1..7, &Add(10));

    assert_eq!(tree.query(&store, 0..8).0 .0, 28);
    assert_eq!(updated.query(&store, 0..8).0 .0, 88);
    assert_eq!(updated.query(&store, 2..5).0 .0, 39);
}
