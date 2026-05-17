use solution::data_structure::seg_tree::{Applier, SegTree, SegTreeStore};
use solution::io::Scanner;
use solution::traits::prelude::{Monoid, Semigroup};

static mut P: u32 = 0;

#[derive(Clone, Debug)]
struct Linear {
    k: u32,
    b: u32,
}

impl Semigroup for Linear {
    /// k2 * (k1 * x + b1) + b2 = (k2 * k1) * x + (k2 * b1 + b2)
    fn merge(self, other: Self) -> Self {
        let p = unsafe { P } as u64;
        Self {
            k: ((self.k as u64 * other.k as u64) % p) as u32,
            b: (((self.k as u64 * other.b as u64) % p + self.b as u64) % p) as u32,
        }
    }
}

impl Monoid for Linear {
    /// x = 1 * x + 0
    fn empty() -> Self {
        Self { k: 1, b: 0 }
    }

    fn is_empty(&self) -> bool {
        self.k == 1 && self.b == 0
    }
}

#[derive(Clone, Debug)]
struct Sum(u32);

#[derive(Clone, Debug)]
struct Size(u32);

impl Semigroup for Sum {
    fn merge(self, other: Self) -> Self {
        Self((self.0 + other.0) % unsafe { P })
    }
}

impl Semigroup for Size {
    fn merge(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl Monoid for Sum {
    fn empty() -> Self {
        Self(0)
    }

    fn is_empty(&self) -> bool {
        self.0 == 0
    }
}

impl Monoid for Size {
    fn empty() -> Self {
        Self(0)
    }

    fn is_empty(&self) -> bool {
        self.0 == 0
    }
}

impl Applier<(Sum, Size)> for Linear {
    /// x' = k*x + b*n
    fn apply(&self, (sum, size): &mut (Sum, Size)) {
        let p = unsafe { P } as u64;
        sum.0 = (((self.k as u64 * sum.0 as u64) % p + (self.b as u64 * size.0 as u64)) % p) as u32;
    }
}

fn main() {
    let mut input = Scanner::stdin();
    let len: usize = input.read();
    let num_commands: usize = input.read();
    let mod_by: u32 = input.read();

    // Safe because the tree haven't create yet.
    unsafe {
        P = mod_by;
    }

    let init_value: Vec<u32> = (0..len).map(|_| input.read()).collect();

    let mut store = SegTreeStore::default();
    let mut tree: SegTree<_, Linear> =
        SegTree::build_in(&mut store, len, |i| (Sum(init_value[i]), Size(1)));

    for _ in 0..num_commands {
        let op: u8 = input.read();
        let x: usize = input.read();
        let y: usize = input.read();
        match op {
            1 => {
                let k: u32 = input.read();
                // a[x..y] = k * a[x..y] + 0
                tree = tree.apply(&mut store, x - 1..y, &Linear { k, b: 0 });
            }
            2 => {
                let b: u32 = input.read();
                // a[x..y] = 1 * a[x..y] + b
                tree = tree.apply(&mut store, x - 1..y, &Linear { k: 1, b });
            }
            3 => {
                println!("{}", tree.query(&store, x - 1..y).0 .0);
            }
            _ => unreachable!(),
        }
    }
}
