use std::{cmp::max, rc::Rc, usize};

fn main() {}

mod TreeAgent {
    #[derive(Default)]
    struct NodeRecord {
        ord: usize,
        subtree_size: usize,
        group_head: usize,
    }

    struct TreeAgent {
        len: usize,
        nodes: Vec<NodeRecord>,
    }
}
