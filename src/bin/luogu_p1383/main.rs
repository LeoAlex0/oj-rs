use solution::data_structure::finger_tree::prelude::*;
use solution::io::{Output, Scanner};
use solution::traits::prelude::*;

type Text = ArenaFingerTree<Value<u8>>;
type Rope = Value<Text>;
type History = ArenaFingerTree<Rope>;

fn main() {
    let mut input = Scanner::stdin();
    let mut output = Output::stdout();
    let n: usize = input.read();
    let text_arena = ArenaFamily::with_capacity(n * 4 + 1);
    let history_arena = ArenaFamily::with_capacity(n * 4 + 1);
    let mut tree = History::new_in(history_arena);
    tree.push_front_mut(Value(Text::new_in(text_arena)));

    for _ in 0..n {
        let command: String = input.read();
        match command.as_bytes()[0] {
            b'T' => {
                let typed = input.read::<String>().as_bytes()[0];
                let last = tree.view_front().map(|it| it.0 .0).unwrap();
                tree.push_front_mut(Value(last.into_push_back(Value(typed))));
            }
            b'U' => {
                let undo_step: usize = input.read();
                let status = tree
                    .split(|it| it > &Size(undo_step))
                    .map(|it| it.1)
                    .unwrap();
                tree.push_front_mut(status);
            }
            b'Q' => {
                let cursor: usize = input.read();
                let current = tree.view_front().map(|it| it.0 .0).unwrap();
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
