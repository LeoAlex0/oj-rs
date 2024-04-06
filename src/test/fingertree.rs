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

    let tree = tree.concat(&tree);
    for i in 0..40 {
        assert_eq!(
            tree.split(|len| len > &Size(i)).map(|it| it.1),
            Some(Value(if i < 20 { i } else { i - 20 }))
        )
    }
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
