use crate::data_structure::finger_tree::*;
use crate::data_structure::ref_store::{
    ArcStoreFactory, ArenaStoreFactory, ConstArenaStoreFactory,
};
use crate::traits::monoid::Size;

#[test]
fn build_split() {
    let mut refs = FingerTreeStore::default();
    let tree: FingerTree<_> = FingerTree::from_iter_in(&mut refs, (0..20).map(Value));
    assert_eq!(tree.measure(), Size(20));
    for i in 0..20 {
        assert_eq!(
            tree.split(&mut refs, |len| len > &Size(i)).map(|it| it.1),
            Some(Value(i))
        )
    }
    assert!(tree.split(&mut refs, |len| len > &Size(20)).is_none());

    let tree = tree.concat(&mut refs, &tree);
    for i in 0..40 {
        assert_eq!(
            tree.split(&mut refs, |len| len > &Size(i)).map(|it| it.1),
            Some(Value(if i < 20 { i } else { i - 20 }))
        )
    }
    assert!(tree.split(&mut refs, |len| len > &Size(40)).is_none());
}

#[test]
fn empty_and_views() {
    let mut refs = FingerTreeStore::default();
    let empty = FingerTree::<Value<usize>>::new();
    assert!(empty.is_empty());
    assert!(empty.view_front(&mut refs).is_none());
    assert!(empty.view_back(&mut refs).is_none());
    assert!(empty.split(&mut refs, |_| true).is_none());

    let tree = empty
        .push_back(&mut refs, Value(1))
        .push_front(&mut refs, Value(0));
    assert_eq!(tree.measure(), Size(2));
    assert_eq!(tree.view_front(&mut refs).map(|it| it.0), Some(Value(0)));
    assert_eq!(tree.view_back(&mut refs).map(|it| it.1), Some(Value(1)));
}

#[test]
fn views_preserve_order() {
    let mut refs = FingerTreeStore::default();
    let tree: FingerTree<_> = FingerTree::from_iter_in(&mut refs, (0..64).map(Value));

    let mut cursor = tree.clone();
    for i in 0..64 {
        let (value, rest) = cursor.view_front(&mut refs).unwrap();
        assert_eq!(value, Value(i));
        cursor = rest;
    }
    assert!(cursor.is_empty());

    let mut cursor = tree;
    for i in (0..64).rev() {
        let (rest, value) = cursor.view_back(&mut refs).unwrap();
        assert_eq!(value, Value(i));
        cursor = rest;
    }
    assert!(cursor.is_empty());
}

#[test]
fn arc_store_operations() {
    let mut refs = FingerTreeStore::new(ArcStoreFactory);
    let mut front: FingerTree<Value<usize>, _> = FingerTree::new();
    let mut back: FingerTree<Value<usize>, _> = FingerTree::new();

    for i in 0..16 {
        front.push_back_mut(&mut refs, Value(i));
        back.push_back_mut(&mut refs, Value(i + 16));
    }

    let tree = front.concat(&mut refs, &back);
    assert_eq!(tree.measure(), Size(32));
    for i in 0..32 {
        assert_eq!(
            tree.split(&mut refs, |len| len > &Size(i)).map(|it| it.1),
            Some(Value(i))
        );
    }
}

#[test]
fn arena_store_shares_array_storage() {
    ArenaStoreFactory::scoped(4096, |factory| {
        let mut arena: FingerTreeStore<Value<usize>, _> = FingerTreeStore::new(factory);
        let mut front = FingerTree::new();
        let mut back = FingerTree::new();

        for i in 0..32 {
            front.push_back_mut(&mut arena, Value(i));
            back.push_back_mut(&mut arena, Value(i + 32));
        }

        let tree = front.into_concat(&mut arena, back);
        assert_eq!(tree.measure(), Size(64));
        for i in 0..64 {
            assert_eq!(
                tree.split(&mut arena, |len| len > &Size(i)).map(|it| it.1),
                Some(Value(i))
            );
        }
    });
}

#[test]
fn arena_consuming_operations() {
    ArenaStoreFactory::scoped(4096, |factory| {
        let mut arena: FingerTreeStore<Value<usize>, _> = FingerTreeStore::new(factory);
        let mut tree: FingerTree<_, _> = FingerTree::new();
        for i in 0..64 {
            tree.push_back_mut(&mut arena, Value(i));
        }

        for i in 0..16 {
            assert_eq!(tree.pop_front(&mut arena), Some(Value(i)));
        }
        for i in (48..64).rev() {
            assert_eq!(tree.pop_back(&mut arena), Some(Value(i)));
        }

        let (front, value, back) = tree.into_split(&mut arena, |len| len > &Size(8)).unwrap();
        assert_eq!(front.measure(), Size(8));
        assert_eq!(value, Value(24));
        assert_eq!(back.measure(), Size(23));
    });
}

#[test]
fn const_arena_store_uses_static_capacity() {
    ConstArenaStoreFactory::<4096>::scoped(|factory| {
        let mut arena: FingerTreeStore<Value<usize>, _> = FingerTreeStore::new(factory);
        let tree: FingerTree<_, _> = FingerTree::from_iter_in(&mut arena, (0..64).map(Value));

        assert_eq!(tree.front(&arena), Some(Value(0)));
        assert_eq!(tree.back(&arena), Some(Value(63)));
        assert_eq!(tree.measure(), Size(64));
    });
}

#[test]
#[should_panic(expected = "const arena capacity exceeded")]
fn const_arena_panics_on_overflow() {
    ConstArenaStoreFactory::<1>::scoped(|factory| {
        let mut arena: FingerTreeStore<Value<usize>, _> = FingerTreeStore::new(factory);
        let _tree: FingerTree<_, _> = FingerTree::from_iter_in(&mut arena, [Value(0), Value(1)]);
    });
}

#[test]
fn layered_arena_uses_scratch_for_results() {
    ArenaStoreFactory::scoped(4096, |factory| {
        let mut base: FingerTreeStore<Value<usize>, _> = FingerTreeStore::new(factory);
        let front: FingerTree<_, _> = FingerTree::from_iter_in(&mut base, (0..40).map(Value));
        let back: FingerTree<_, _> = FingerTree::from_iter_in(&mut base, (40..80).map(Value));

        base.layered(1024, |mut scratch| {
            let front = scratch.from_base(&front);
            let back = scratch.from_base(&back);
            let tree = front.concat(&mut scratch, &back);
            assert_eq!(tree.measure(), Size(80));

            for i in 0..80 {
                assert_eq!(
                    tree.split(&mut scratch, |len| len > &Size(i))
                        .map(|it| it.1),
                    Some(Value(i))
                );
            }
        });

        assert_eq!(front.front(&base), Some(Value(0)));
        assert_eq!(back.back(&base), Some(Value(79)));
    });
}

#[test]
fn concat() {
    for front_size in 0..20 {
        for back_size in 0..20 {
            let mut refs = FingerTreeStore::default();
            let front: FingerTree<_> =
                FingerTree::from_iter_in(&mut refs, (0..front_size).map(Value));
            let back: FingerTree<_> =
                FingerTree::from_iter_in(&mut refs, (0..back_size).map(Value));

            let tree = front.concat(&mut refs, &back);
            for i in 0..(front_size + back_size) {
                let value = tree.split(&mut refs, |l| l > &Size(i)).map(|it| it.1);
                assert_eq!(
                    value,
                    Some(if i < front_size {
                        Value(i)
                    } else {
                        Value(i - front_size)
                    })
                )
            }
        }
    }
}
