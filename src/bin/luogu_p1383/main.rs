use solution::data_structure::finger_tree::prelude::*;
use solution::io::{Output, Scanner};
use solution::traits::prelude::*;

type Rope = Value<FingerTree<Value<u8>>>;

fn main() {
    let mut input = Scanner::stdin();
    let mut output = Output::stdout();
    let n: usize = input.read();
    let mut tree: FingerTree<Rope> = vec![Value(FingerTree::new())].into_iter().collect();

    for _ in 0..n {
        let command: String = input.read();
        match command.as_bytes()[0] {
            b'T' => {
                let typed = input.read::<String>().as_bytes()[0];
                let last = tree.view_l().map(|it| it.0 .0).unwrap();
                tree = FingerTree::push_l(Value(FingerTree::push_r(&last, Value(typed))), &tree);
            }
            b'U' => {
                let undo_step: usize = input.read();
                let status = tree
                    .split(|it| it > &Size(undo_step))
                    .map(|it| it.1)
                    .unwrap();
                tree = FingerTree::push_l(status, &tree);
            }
            b'Q' => {
                let cursor: usize = input.read();
                let current = tree.view_l().map(|it| it.0 .0).unwrap();
                let queried = current
                    .split(|it| it > &Size(cursor - 1))
                    .map(|it| it.1)
                    .unwrap()
                    .0;
                output.bytes(&[queried, b'\n']);
            }
            _ => panic!("unknown command: {command}"),
        }
    }
}
