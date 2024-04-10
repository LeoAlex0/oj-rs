extern crate solution;

use solution::{data_structure::finger_tree::*, traits::monoid::Size};
use std::io::{stdin, stdout, Write};

type Rope = Value<FingerTree<Value<u8>>>;

fn main() {
    let mut buf = String::new();
    stdin().read_line(&mut buf).unwrap();
    let n = buf.trim().parse().unwrap();
    let mut tree: FingerTree<Rope> = vec![Value(FingerTree::new())].into_iter().collect();

    for _ in 0..n {
        buf.clear();
        stdin().read_line(&mut buf).unwrap();
        buf = buf.trim().to_string();
        if buf.starts_with('T') {
            let typed = buf.as_bytes()[2];
            let last = tree.view_l().map(|it| it.0 .0).unwrap();
            tree = FingerTree::push_l(Value(FingerTree::push_r(&last, Value(typed))), &tree);
        } else if buf.starts_with('U') {
            let words: Vec<_> = buf.split_whitespace().collect();
            let undo_step: usize = words[1].parse().unwrap();
            let status = tree
                .split(|it| it > &Size(undo_step))
                .map(|it| it.1)
                .unwrap();
            tree = FingerTree::push_l(status, &tree);
        } else if buf.starts_with('Q') {
            let words: Vec<_> = buf.split_whitespace().collect();
            let cursor: usize = words[1].parse().unwrap();
            let current = tree.view_l().map(|it| it.0 .0).unwrap();
            let queried = current
                .split(|it| it > &Size(cursor - 1))
                .map(|it| it.1)
                .unwrap()
                .0;
            stdout().write_all(&[queried]).unwrap();
            println!();
        } else {
            panic!("unknown command: {}", buf);
        }
    }
}
