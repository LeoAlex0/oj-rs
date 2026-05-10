# oj-pack 开发说明

`oj-pack` 是本仓库的提交源码打包器。它不是通用 Rust bundler，而是面向
`solution` library + `src/bin/<problem>/main.rs` 这种 OJ 仓库布局的专用工具。

日常入口：

```sh
cargo oj-pack luogu_p3372 > out.rs
cargo oj-pack --minify luogu_p3372 > out.rs
cargo test -p oj-pack
```

## 依赖选型

- `cargo_metadata`：读取 workspace/package/target 信息，避免手写
  `Cargo.toml` 解析逻辑。代码里调用的是 `MetadataCommand::no_deps()`，等价于
  `cargo metadata --no-deps`；这里的 `--no-deps` 不是 `oj-pack` 的 CLI 参数，
  只表示当前实现不解析或打包 crates.io 依赖。
- `syn`：用 `full` 解析完整 Rust item，用 `visit-mut` 改写路径。打包器依赖
  AST 结构处理 `use`、`mod`、`impl`、`macro_rules!` 等 item，避免用正则修改
  Rust 源码。
- `quote` / `proc-macro2`：把筛选后的 `syn::Item` 重新转为 token stream，并在
  minify 时按 token 粒度处理空格，避免破坏字符串、生命周期、宏调用等语法。
- `prettyplease`：默认输出格式化后的可读源码。`--minify` 才走自带的 token
  压缩逻辑。
- `clap`：用 derive 定义 CLI，提供标准化 `--help`、usage 和错误提示。当前保留
  `cargo oj-pack <bin>` / `--list` 旧入口，同时提供 `pack`、`list`、
  `completions` 子命令。
- `clap_complete`：基于同一份 clap command 定义生成 shell completion，避免手写
  completion 脚本。

## 代码结构

入口在 `src/main.rs`：

- `Cli` / `Command` / `PackFlags` 定义 clap CLI。默认输出格式化代码，`--minify`
  输出压缩代码。
- `run` 负责把 clap 解析出的 action 分发到 `pack_binary`、`list_binaries` 或
  completion 生成。
- `pack_binary` 把 CLI 参数翻译成 `PackOptions`，再调用 `pack_project`。

核心逻辑在 `src/lib.rs`：

- `PackageInfo::load`：通过 `cargo metadata` 找到 root package、library target
  和所有 binary target。
- `pack_project`：串联完整流程：解析 binary、收集 library 根引用、路径改写、
  DCE、渲染、格式化或压缩、`rustc --emit=metadata` 校验。
- `clean_file` / `clean_item`：删除 `#[cfg(test)]` item 和 doc attribute。这里不
  做业务裁剪，只做“提交时肯定不需要”的清理。
- `collect_solution_roots`：从 binary 中的 `use solution::...` 和直接
  `solution::...` 路径收集 DCE 初始根。
- `Library` / `Module` / `Record`：内存中的源码索引。`Module` 保留模块树顺序，
  `Record` 存 item 名称、引用、impl 信息和可见性。
- `Library::eliminate_dead_items`：item-level DCE。它从根 item 出发，反复扩展
  本地引用、相关 impl block 和必要宏调用，直到可达集合稳定。
- `Library::render`：按原模块树顺序输出仍然可达的模块和 item。
- `RewriteCrateName`：把 binary 里的 `solution::...` 改为单文件内的
  `crate::...`。
- `minify`：只在 `--minify` 时使用，按 token 判断是否必须保留空格。
- `check_rustc`：将生成源码写入临时文件，用 `rustc --emit=metadata` 做语法和
  类型层面的快速校验。

## DCE 策略

DCE 是保守的近似名称解析，不是完整 Rust resolver。

根集合来自：

- 显式导入：`use solution::data_structure::seg_tree::*;`
- 直接路径：`solution::traits::monoid::Size`

保留规则：

- public item 被 binary 引用或通过 glob import 实际使用时进入初始集合。
- 已保留 item 中出现的本地名称会继续保留对应 item。
- `impl LocalType` 或 `impl Trait for LocalType` 随 self type 保留。
- `impl LocalTrait for ExternalType` 随 trait 保留。
- 宏定义和宏调用按 item 粒度保留；宏生成 impl 时会保守保留必要的宏调用。

已知会偏保守的情况：

- 一个宏调用同时生成多个 impl 时，DCE 不能拆开宏展开结果，只能保留整个宏调用。
- glob import 会根据 binary 中出现过的标识符收窄，但不做完整作用域解析。
- 对 trait method、associated item、泛型 bound 的解析是基于 token 中的本地
  ident，因此可能多保留，不应少保留。

## 模块解析边界

当前支持：

- `src/lib.rs` 作为 library root；
- `mod foo;` 对应 `foo.rs` 或 `foo/mod.rs`；
- inline module；
- `pub use module::*` 这类 re-export。

当前不支持：

- 打包 crates.io 依赖；
- build script 或 proc macro；
- `#[path = "..."] mod foo;`；
- 由宏生成的模块；
- 多个本地 library crate 互相打包。

如果以后要支持这些能力，优先扩展 `load_module` / `find_module_file` 和
`collect_solution_roots`，不要在输出字符串上做后处理。

## 修改建议

- 新增 CLI 参数时，先改 `Cli` / `PackFlags` 和 `PackOptions`，再补
  `src/main.rs` 的 clap 解析单元测试。会影响 shell completion 的参数必须通过
  clap 定义表达，不要绕过 clap 手写解析。
- 改 DCE 规则时，优先添加一个能体现“少保留/不误删”的测试。现有
  `item_level_dce_excludes_unreachable_library_items` 可以作为模板。
- 改输出格式时，同时测试默认格式化输出和 `--minify` 输出。
- 保持 stdout 只输出最终源码；所有诊断、列表和 warning 走 stderr。

常用验证：

```sh
cargo fmt --all --check
cargo check -p solution --bins
cargo test -p oj-pack
cargo oj-pack --help
cargo oj-pack completions zsh > /tmp/_oj-pack
for bin in $(cargo oj-pack --list 2>&1); do cargo oj-pack "$bin" > "/tmp/$bin.rs"; done
```
