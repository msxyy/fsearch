//! fsearch 的核心库。
//!
//! 这个 crate 同时提供：
//!
//! - 一个命令行工具 `fsearch`
//! - 一个可复用的库，包含搜索、输出、索引三部分
//!
//! 使用示例：
//!
//! ```no_run
//! use fsearch::{search, SearchOptions};
//! use regex::Regex;
//! use std::path::Path;
//!
//! let re = Regex::new("fn main").unwrap();
//! let opts = SearchOptions::default();
//! let result = search::search_file(Path::new("src/main.rs"), &re, &opts).unwrap();
//! for line in &result.output {
//!     // 处理每一行
//! }
//! ```

pub mod index;
pub mod pattern;
pub mod printer;
pub mod search;

pub use index::Index;
pub use pattern::{build_combined_pattern, build_pattern, literal_patterns};
pub use printer::{choose_color, JsonPrinter, Output, Printer};
pub use search::{search_file, FileResult, OutputLine, Prefilter, SearchOptions};
