use crate::data_structure::finger_tree::*;
use crate::traits::monoid::Size;

#[test]
fn build_split() {
    let tree: FingerTree<_> = (0..20).map(Value).collect();
    assert_eq!(tree.measure(), Size(20));
    for i in 0..20 {
        assert_eq!(
            tree.split(|len| len > &Size(i)).map(|it| it.1),
            Some(Value(i))
        )
    }
    assert!(tree.split(|len| len > &Size(20)).is_none());

    let tree = tree.concat(&tree);
    for i in 0..40 {
        assert_eq!(
            tree.split(|len| len > &Size(i)).map(|it| it.1),
            Some(Value(if i < 20 { i } else { i - 20 }))
        )
    }
    assert!(tree.split(|len| len > &Size(40)).is_none());
}

#[test]
fn empty_and_views() {
    let empty = FingerTree::<Value<usize>>::new();
    assert!(empty.is_empty());
    assert!(empty.view_front().is_none());
    assert!(empty.view_back().is_none());
    assert!(empty.split(|_| true).is_none());

    let tree = empty.push_back(Value(1)).push_front(Value(0));
    assert_eq!(tree.measure(), Size(2));
    assert_eq!(tree.view_front().map(|it| it.0), Some(Value(0)));
    assert_eq!(tree.view_back().map(|it| it.1), Some(Value(1)));
}

#[test]
fn views_preserve_order() {
    let tree: FingerTree<_> = (0..64).map(Value).collect();

    let mut cursor = tree.clone();
    for i in 0..64 {
        let (value, rest) = cursor.view_front().unwrap();
        assert_eq!(value, Value(i));
        cursor = rest;
    }
    assert!(cursor.is_empty());

    let mut cursor = tree;
    for i in (0..64).rev() {
        let (rest, value) = cursor.view_back().unwrap();
        assert_eq!(value, Value(i));
        cursor = rest;
    }
    assert!(cursor.is_empty());
}

#[test]
fn arena_family_shares_array_storage() {
    let arena = ArenaFamily::with_capacity(128);
    let mut front = FingerTree::new_in(arena.clone());
    let mut back = FingerTree::new_in(arena);

    for i in 0..32 {
        front.push_back_mut(Value(i));
        back.push_back_mut(Value(i + 32));
    }

    let tree = front.into_concat(back);
    assert_eq!(tree.measure(), Size(64));
    for i in 0..64 {
        assert_eq!(
            tree.split(|len| len > &Size(i)).map(|it| it.1),
            Some(Value(i))
        );
    }
}

#[test]
fn box_family_consuming_operations() {
    let mut tree = BoxFingerTree::new();
    for i in 0..64 {
        tree.push_back_mut(Value(i));
    }

    for i in 0..16 {
        assert_eq!(tree.pop_front(), Some(Value(i)));
    }
    for i in (48..64).rev() {
        assert_eq!(tree.pop_back(), Some(Value(i)));
    }

    let (front, value, back) = tree.into_split(|len| len > &Size(8)).unwrap();
    assert_eq!(front.measure(), Size(8));
    assert_eq!(value, Value(24));
    assert_eq!(back.measure(), Size(23));
}

#[test]
fn concat() {
    for front_size in 0..20 {
        for back_size in 0..20 {
            let front: FingerTree<_> = (0..front_size).map(Value).collect();
            let back: FingerTree<_> = (0..back_size).map(Value).collect();

            let tree = front.concat(&back);
            for i in 0..(front_size + back_size) {
                let value = tree.split(|l| l > &Size(i)).map(|it| it.1);
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
