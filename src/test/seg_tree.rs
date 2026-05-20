use crate::data_structure::ref_store::{
    AlignedArenaStoreFactory, ArcStoreFactory, ArenaRef, ArenaStoreFactory, ConstArenaStoreFactory,
};
use crate::data_structure::seg_tree::prelude::*;
use crate::traits::prelude::*;

#[derive(Clone)]
struct Add(i64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Mask(u128);

#[derive(Clone)]
struct ByteMap {
    map: [u8; 128],
    identity: bool,
}

impl ByteMap {
    fn identity() -> Self {
        let mut map = [0; 128];
        for (i, value) in map.iter_mut().enumerate() {
            *value = i as u8;
        }
        Self {
            map,
            identity: true,
        }
    }

    fn replace(from: u8, to: u8) -> Self {
        let mut map = Self::identity();
        map.apply_pair(from, to);
        map
    }

    fn apply_pair(&mut self, from: u8, to: u8) {
        if from == to {
            return;
        }

        let mut identity = true;
        for (i, value) in self.map.iter_mut().enumerate() {
            if *value == from {
                *value = to;
            }
            identity &= *value == i as u8;
        }
        self.identity = identity;
    }

    fn apply_to_mask(&self, mask: Mask) -> Mask {
        if self.identity {
            return mask;
        }

        let mut next = 0;
        let mut rest = mask.0;
        while rest != 0 {
            let color = rest.trailing_zeros() as usize;
            next |= 1u128 << self.map[color];
            rest &= rest - 1;
        }
        Mask(next)
    }
}

impl Semigroup for Mask {
    fn merge(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    fn merge_assign(&mut self, other: &Self) {
        self.0 |= other.0;
    }
}

impl Monoid for Mask {
    fn empty() -> Self {
        Self(0)
    }
}

impl Semigroup for Add {
    fn merge(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }

    fn merge_assign(&mut self, other: &Self) {
        self.0 += other.0;
    }

    fn prepend_assign(&mut self, other: &Self) {
        self.merge_assign(other);
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

impl Semigroup for ByteMap {
    fn merge(self, other: Self) -> Self {
        if self.identity {
            return other;
        }
        if other.identity {
            return self;
        }

        let mut map = [0; 128];
        let mut identity = true;
        for (i, value) in map.iter_mut().enumerate() {
            *value = self.map[other.map[i] as usize];
            identity &= *value == i as u8;
        }
        Self { map, identity }
    }

    fn prepend_assign(&mut self, other: &Self)
    where
        Self: Clone,
    {
        if other.identity {
            return;
        }
        if self.identity {
            *self = other.clone();
            return;
        }

        let mut identity = true;
        for (i, value) in self.map.iter_mut().enumerate() {
            *value = other.map[*value as usize];
            identity &= *value == i as u8;
        }
        self.identity = identity;
    }
}

impl Monoid for ByteMap {
    fn empty() -> Self {
        Self::identity()
    }

    fn is_empty(&self) -> bool {
        self.identity
    }
}

impl Applier<Mask> for ByteMap {
    fn apply(&self, mask: &mut Mask) {
        *mask = self.apply_to_mask(*mask);
    }

    fn affects(&self, value: &Mask) -> bool {
        if self.identity {
            return false;
        }

        let mut rest = value.0;
        while rest != 0 {
            let color = rest.trailing_zeros() as usize;
            if self.map[color] != color as u8 {
                return true;
            }
            rest &= rest - 1;
        }
        false
    }

    fn apply_slice(&self, values: &mut [Mask]) {
        for value in values {
            self.apply(value);
        }
    }
}

#[test]
fn in_place_seg_tree_range_update_and_query() {
    ArenaStoreFactory::scoped(128, |factory| {
        let mut arena: SegTreeStore<(Sum<i64>, Size), Add, DEFAULT_SEG_LEAF_BLOCK_CAPACITY, _> =
            SegTreeStore::new(factory);
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
fn seg_tree_updates_blocks_with_custom_batch_apply() {
    const BLOCK: usize = 4;
    let mut store = SegTreeStore::default();
    let mut tree: SegTree<Mask, ByteMap, BLOCK> = SegTree::build_in(&mut store, 9, |i| {
        Mask(1u128 << [1, 2, 3, 2, 1, 2, 4, 2, 5][i])
    });

    tree = tree.apply(&mut store, 1..8, &ByteMap::replace(2, 7));
    let values = tree
        .iter(&store)
        .map(|mask| mask.0.trailing_zeros() as u8)
        .collect::<Vec<_>>();
    assert_eq!(values, vec![1, 7, 3, 7, 1, 7, 4, 7, 5]);

    tree = tree.apply(&mut store, 0..9, &ByteMap::replace(7, 6));
    let values = tree
        .iter(&store)
        .map(|mask| mask.0.trailing_zeros() as u8)
        .collect::<Vec<_>>();
    assert_eq!(values, vec![1, 6, 3, 6, 1, 6, 4, 6, 5]);

    let mut store = SegTreeStore::default();
    let mut tree: SegTree<Mask, ByteMap, BLOCK> = SegTree::build_in(&mut store, 9, |i| {
        Mask(1u128 << [1, 2, 2, 2, 1, 2, 4, 2, 5][i])
    });
    tree = tree.apply(&mut store, 0..9, &ByteMap::replace(2, 7));
    tree = tree.apply(&mut store, 2..4, &ByteMap::replace(7, 8));
    let values = tree
        .iter(&store)
        .map(|mask| mask.0.trailing_zeros() as u8)
        .collect::<Vec<_>>();
    assert_eq!(values, vec![1, 7, 8, 8, 1, 7, 4, 7, 5]);
}

#[test]
fn arena_seg_tree_keeps_persistent_versions() {
    ArenaStoreFactory::scoped(128, |factory| {
        let mut arena: SegTreeStore<(Sum<i64>, Size), Add, DEFAULT_SEG_LEAF_BLOCK_CAPACITY, _> =
            SegTreeStore::new(factory);
        let tree = SegTree::build_in(&mut arena, 8, |i| (Sum(i as i64), Size::default()));
        let updated = tree.apply(&mut arena, 1..7, &Add(10));

        assert_eq!(tree.query(&arena, 0..8).0 .0, 28);
        assert_eq!(updated.query(&arena, 0..8).0 .0, 88);
        assert_eq!(updated.query(&arena, 2..5).0 .0, 39);
    });
}

#[test]
fn aligned_arena_seg_tree_range_update_and_query() {
    AlignedArenaStoreFactory::scoped(128, |factory| {
        let mut arena: SegTreeStore<(Sum<i64>, Size), Add, DEFAULT_SEG_LEAF_BLOCK_CAPACITY, _> =
            SegTreeStore::new(factory);
        let mut tree = SegTree::build_in(&mut arena, 8, |i| (Sum(i as i64), Size::default()));

        assert_eq!(tree.query(&arena, 0..8).0 .0, 28);
        tree.apply_mut(&mut arena, 1..7, &Add(10));
        assert_eq!(tree.query(&arena, 0..8).0 .0, 88);
        assert_eq!(tree.query(&arena, 2..5).0 .0, 39);
    });
}

#[test]
fn const_arena_seg_tree_builds_with_static_capacity() {
    ConstArenaStoreFactory::<128>::scoped(|factory| {
        let mut arena: SegTreeStore<(Sum<i64>, Size), Add, DEFAULT_SEG_LEAF_BLOCK_CAPACITY, _> =
            SegTreeStore::new(factory);
        let tree = SegTree::build_in(&mut arena, 8, |i| (Sum(i as i64), Size::default()));

        assert_eq!(tree.query(&arena, 0..8).0 .0, 28);
        assert_eq!(tree.query(&arena, 2..5).0 .0, 9);
    });
}

#[test]
fn seg_tree_leaf_block_capacity_is_const_calculated() {
    const BLOCK: usize = seg_block_capacity_for_bytes::<(Sum<i64>, Size), Add, ArenaRef<'static>>(
        SEG_TREE_CACHE_LINE_BYTES + SEG_TREE_CACHE_LINE_BYTES / 2,
    );

    ArenaStoreFactory::scoped(128, |factory| {
        let mut arena: SegTreeStore<(Sum<i64>, Size), Add, BLOCK, _> = SegTreeStore::new(factory);
        let tree: SegTree<_, Add, BLOCK, _> =
            SegTree::build_in(&mut arena, 8, |i| (Sum(i as i64), Size::default()));

        assert_eq!(tree.query(&arena, 0..8).0 .0, 28);
        assert_eq!(tree.query(&arena, 2..5).0 .0, 9);
    });
}

#[test]
fn seg_tree_leaf_block_capacity_targets_node_size() {
    const BLOCK: usize = seg_block_capacity_for_bytes::<(Sum<i64>, Size), Add, ArenaRef<'static>>(
        SEG_TREE_CACHE_LINE_BYTES,
    );
    type Store = SegTreeStore<(Sum<i64>, Size), Add, BLOCK, ArenaStoreFactory<'static>>;

    assert!(std::mem::size_of::<SegNode<(Sum<i64>, Size), Add, BLOCK, Store>>() <= 80);
}

#[test]
fn layered_arena_seg_tree_allocates_updates_in_scratch() {
    ArenaStoreFactory::scoped(128, |factory| {
        let mut base: SegTreeStore<(Sum<i64>, Size), Add, DEFAULT_SEG_LEAF_BLOCK_CAPACITY, _> =
            SegTreeStore::new(factory);
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
    let mut store: SegTreeStore<
        (Sum<i64>, Size),
        Add,
        DEFAULT_SEG_LEAF_BLOCK_CAPACITY,
        ArcStoreFactory,
    > = SegTreeStore::new(ArcStoreFactory);
    let tree = SegTree::build_in(&mut store, 8, |i| (Sum(i as i64), Size::default()));

    let updated = tree.apply(&mut store, 1..7, &Add(10));

    assert_eq!(tree.query(&store, 0..8).0 .0, 28);
    assert_eq!(updated.query(&store, 0..8).0 .0, 88);
    assert_eq!(updated.query(&store, 2..5).0 .0, 39);
}
