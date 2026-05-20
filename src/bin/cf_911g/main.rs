use solution::{
    data_structure::{
        ref_store::{AlignedArenaStoreFactory, ArenaRef},
        seg_tree::{
            seg_block_capacity_for_bytes, Applier, SegTree, SegTreeStore, SEG_TREE_CACHE_LINE_BYTES,
        },
    },
    io::{Output, Scanner},
    traits::prelude::{Monoid, Semigroup},
};

#[derive(Clone, Copy)]
struct Mask(u128);

#[derive(Clone)]
struct Replace {
    map: [u8; 101],
    changed: u128,
}

const LEAF_BLOCK: usize =
    seg_block_capacity_for_bytes::<Mask, Replace, ArenaRef<'static>>(SEG_TREE_CACHE_LINE_BYTES);

impl Replace {
    fn new(from: u8, to: u8) -> Self {
        let mut map = [0; 101];
        for (i, value) in map.iter_mut().enumerate() {
            *value = i as u8;
        }
        map[from as usize] = to;
        Self {
            map,
            changed: u128::from(from != to) << from,
        }
    }

    fn apply_to_mask(&self, mask: Mask) -> Mask {
        if self.changed == 0 {
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

    fn is_empty(&self) -> bool {
        self.0 == 0
    }
}

impl Semigroup for Replace {
    fn merge(self, other: Self) -> Self {
        if self.changed == 0 {
            return other;
        }
        if other.changed == 0 {
            return self;
        }

        let mut map = [0; 101];
        let mut changed = 0;
        for (i, value) in map.iter_mut().enumerate() {
            *value = self.map[other.map[i] as usize];
            if *value == i as u8 {
                continue;
            }
            changed |= 1u128 << i;
        }
        Self { map, changed }
    }

    fn prepend_assign(&mut self, other: &Self)
    where
        Self: Clone,
    {
        if other.changed == 0 {
            return;
        }
        if self.changed == 0 {
            *self = other.clone();
            return;
        }

        let mut changed = 0;
        for (i, value) in self.map.iter_mut().enumerate() {
            *value = other.map[*value as usize];
            if *value == i as u8 {
                continue;
            }
            changed |= 1u128 << i;
        }
        self.changed = changed;
    }
}

impl Monoid for Replace {
    fn empty() -> Self {
        let mut map = [0; 101];
        for (i, value) in map.iter_mut().enumerate() {
            *value = i as u8;
        }
        Self { map, changed: 0 }
    }

    fn is_empty(&self) -> bool {
        self.changed == 0
    }
}

impl Applier<Mask> for Replace {
    fn apply(&self, mask: &mut Mask) {
        *mask = self.apply_to_mask(*mask);
    }

    fn affects(&self, value: &Mask) -> bool {
        value.0 & self.changed != 0
    }
}

fn mask(value: u8) -> Mask {
    Mask(1u128 << value)
}

fn main() {
    let mut input = Scanner::stdin();

    let n: usize = input.read();
    let array: Vec<u8> = (0..n).map(|_| input.read()).collect();

    AlignedArenaStoreFactory::scoped(n * 2 + 16, |factory| {
        let mut store: SegTreeStore<Mask, Replace, LEAF_BLOCK, _> = SegTreeStore::new(factory);
        let mut tree = SegTree::build_in(&mut store, n, |i| mask(array[i]));

        let q: usize = input.read();
        for _ in 0..q {
            let l: usize = input.read();
            let r: usize = input.read();
            let from: u8 = input.read();
            let to: u8 = input.read();
            tree.apply_mut(&mut store, l - 1..r, &Replace::new(from, to));
        }

        let mut output = Output::stdout();
        for (i, value) in tree.iter(&store).enumerate() {
            if i > 0 {
                output.print(" ");
            }
            output.print(value.0.trailing_zeros() as u8);
        }
        output.println("");
    });
}
