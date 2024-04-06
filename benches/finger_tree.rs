use criterion::{black_box, criterion_group, criterion_main, Criterion};
use solution::{
    data_structure::finger_tree::{FingerTree, PersistMonoidIndexDeque, RcRef, Value},
    traits::monoid::Size,
};

macro_rules! splits {
    ($CRITERION_MUT_REF:expr ,$LEN:expr, $REF:ty) => {
        $CRITERION_MUT_REF.bench_function(
            format!("split {} {}", stringify!($REF), stringify!($LEN)).as_ref(),
            |b| {
                let tree: FingerTree<_, $REF> = (0..$LEN as usize).map(Value).collect();
                b.iter(|| black_box(&tree).split(|l| l > &Size($LEN as usize >> 1)))
            },
        );
    };
}

macro_rules! concat {
    ($CRITERION_MUT_REF:expr, $L_LEN:expr, $R_LEN: expr, $REF:ty) => {
        $CRITERION_MUT_REF.bench_function(
            format!(
                "concat {} {}<>{}",
                stringify!($REF),
                stringify!($L_LEN),
                stringify!($R_LEN)
            )
            .as_ref(),
            |b| {
                let tree_l: FingerTree<_, RcRef> = (0..$L_LEN as usize).map(Value).collect();
                let tree_r: FingerTree<_, RcRef> = (0..$R_LEN as usize).map(Value).collect();
                b.iter(|| black_box(&tree_l).concat(black_box(&tree_r)))
            },
        );
    };
}

fn criterion_benchmark(c: &mut Criterion) {
    splits!(c, 1e3, RcRef);
    splits!(c, 1e5, RcRef);

    concat!(c, 1e3, 1e3, RcRef);
    concat!(c, 1e3, 1e5, RcRef);
    concat!(c, 1e5, 1e3, RcRef);
    concat!(c, 1e5, 1e5, RcRef);
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
