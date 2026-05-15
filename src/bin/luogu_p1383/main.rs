use solution::data_structure::finger_tree::prelude::*;
use solution::data_structure::ref_store::ArenaStoreFactory;
use solution::io::{Output, Scanner};
use solution::traits::prelude::*;

type TextStore<'text> = FingerTreeStore<Value<u8>, ArenaStoreFactory<'text>>;
type Text<'text> = FingerTree<Value<u8>, TextStore<'text>>;
type Rope<'text> = Value<Text<'text>>;
type HistoryStore<'history, 'text> = FingerTreeStore<Rope<'text>, ArenaStoreFactory<'history>>;
type History<'history, 'text> = FingerTree<Rope<'text>, HistoryStore<'history, 'text>>;

fn main() {
    let mut input = Scanner::stdin();
    let mut output = Output::stdout();
    let n: usize = input.read();
    ArenaStoreFactory::scoped(n * 4 + 1, |text_factory| {
        let mut text_arena: TextStore<'_> = FingerTreeStore::new(text_factory);
        ArenaStoreFactory::scoped(n * 4 + 1, |history_factory| {
            let mut history_arena: HistoryStore<'_, '_> = FingerTreeStore::new(history_factory);
            let mut tree: History<'_, '_> = History::new();
            tree.push_front_mut(&mut history_arena, Value(Text::new()));

            for _ in 0..n {
                let command: String = input.read();
                match command.as_bytes()[0] {
                    b'T' => {
                        let typed = input.read::<String>().as_bytes()[0];
                        let last = tree.front(&history_arena).unwrap().0;
                        tree.push_front_mut(
                            &mut history_arena,
                            Value(last.into_push_back(&mut text_arena, Value(typed))),
                        );
                    }
                    b'U' => {
                        let undo_step: usize = input.read();
                        let status = tree
                            .split(&mut history_arena, |it| it > &Size(undo_step))
                            .map(|it| it.1)
                            .unwrap();
                        tree.push_front_mut(&mut history_arena, status);
                    }
                    b'Q' => {
                        let cursor: usize = input.read();
                        let current = tree.front(&history_arena).unwrap().0;
                        let queried = current
                            .split(&mut text_arena, |it| it > &Size(cursor - 1))
                            .map(|it| it.1)
                            .unwrap()
                            .0;
                        output.bytes(&[queried, b'\n']);
                    }
                    _ => unreachable!("unknown command: {command}"),
                }
            }
        });
    });
}
