//! `fsearch` 的入口，实现见 [`fsearch::app`]。
//!
//! 短名 `fs`（`src/bin/fs.rs`）与本程序共用同一份逻辑。

fn main() -> std::process::ExitCode {
    fsearch::app::run()
}
