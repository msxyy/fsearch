//! `fs` —— `fsearch` 的短名入口，与 `src/main.rs` 共用 [`fsearch::app`] 的逻辑。
//!
//! 程序名取自 argv[0]，所以 `fs --help`、`fs --version`
//! 和 `fs --generate-completions` 输出的都是 `fs`。

fn main() -> std::process::ExitCode {
    fsearch::app::run()
}
