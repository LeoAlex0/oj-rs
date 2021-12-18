use crate::traits::monoid::Size;

use crate::data_structure::finger_tree::*;
use paste::paste;

extern crate test;

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

macro_rules! bench_split_ref {
    ($len:literal,$ref:ty) => {
        paste! {
            #[bench]
            fn [<split_ $len _ $ref:snake>](b: &mut test::Bencher) {
                let tree: FingerTree<_,$ref> = (0..$len as usize).map(Value).collect();
                b.iter(|| tree.split(|l| l > &Size($len as usize >> 1)));
            }
        }
    };
}

macro_rules! bench_concat_ref {
    ($len:literal,$ref:ty) => {
        paste! {
            #[bench]
            fn [<concat_ $len _ $ref:snake>](b: &mut test::Bencher) {
                let tree: FingerTree<_,$ref> = (0..$len as usize).map(Value).collect();
                b.iter(|| tree.concat(&tree));
            }
        }
    };
}

macro_rules! bench_split {
    ($len :literal) => {
        bench_split_ref!($len, RcRef);
        bench_split_ref!($len, ArcRef);
    };
}

macro_rules! bench_concat {
    ($len:literal) => {
        bench_concat_ref!($len, RcRef);
        bench_concat_ref!($len, ArcRef);
    };
}

macro_rules! benches {
    ($($len:literal),*) => {
        $(
            bench_split!($len);
            bench_concat!($len);
        )*
    };
}

benches![1e4, 1e5, 1e6, 1e7];
