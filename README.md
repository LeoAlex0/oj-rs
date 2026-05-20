# oj-rs

这是一个用于 Online Judge 题目提交的 Rust 工作区。每道题作为一个 Cargo
binary 放在 `src/bin/<problem>/main.rs` 下，复用的数据结构和算法放在
`src/` 下的 `solution` library crate 中。

## 题解代码组织

题解推荐只导入自己需要的主题级入口，避免把整个 library 都带进打包根集合：

```rust
use solution::data_structure::seg_tree::prelude::*;
use solution::io::{Output, Scanner};
use solution::traits::prelude::*;
```

`solution::io` 提供轻量的 `Scanner` 和 `Output`，用于 OJ 常见的
whitespace token 输入和缓冲输出。数据结构模块按需提供自己的 `prelude`，
例如 `seg_tree::prelude`、`finger_tree::prelude`；代数抽象放在
`traits::prelude`。

## 打包提交

多数 OJ 只能提交单个源文件。本仓库提供了 `oj-pack`，用于把某个 Cargo
binary 打包成一个可提交的 `.rs` 单文件。

```sh
cargo oj-pack luogu_p3372 > out.rs
```

生成结果写入 stdout；诊断信息、警告和 `--list` 输出写入 stderr。因此使用
重定向时，`out.rs` 会保持为干净的 Rust 源文件。

常用参数：

```sh
cargo oj-pack --list
cargo oj-pack --minify luogu_p3372 > out.rs
cargo oj-pack --no-check luogu_p3372
cargo oj-pack --no-prune-impl-items luogu_p3372
cargo oj-pack --max-bytes 65536 luogu_p3372 > out.rs
cargo oj-pack completions zsh > _oj-pack
```

默认情况下，`oj-pack` 会：

- 通过 `cargo metadata` 发现 binary 和 library crate；
- 展开目标 binary 实际用到的本地 `solution` library 模块；
- 将 `solution::...` 路径改写为单文件内可用的 `crate::...` 路径；
- 移除测试专用 item 和文档属性；
- 执行保守的 item-level dead code elimination；
- 自动裁剪保留 impl block 中未实际使用的固有方法；
- 默认输出经过格式化的可读源码；
- 使用 `rustc --emit=metadata` 校验生成源码。

如果需要尽可能缩小提交体积，可以添加 `--minify`。该模式会去掉非必要的空格
和换行，只保留 Rust token 之间必须存在的分隔。

## 开发

检查所有题目 binary：

```sh
cargo check -p solution --bins
```

运行打包器测试：

```sh
cargo test -p oj-pack
```

对所有可打包 binary 做一次 smoke test：

```sh
for bin in $(cargo oj-pack --list 2>&1); do
  cargo oj-pack "$bin" > "/tmp/$bin.rs"
done
```

## 打包器限制

`oj-pack` 是针对本仓库代码组织方式编写的轻量打包器，不是通用 Rust bundler。
当前支持文件式模块和本地 library 代码；不打包 crates.io 依赖、build script、
proc macro 或由宏生成的模块。对于宏生成的 impl，会在需要时保守保留，以优先
保证生成代码可编译。
