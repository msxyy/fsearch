//! fsearch 的核心库。
//!
//! 这个 crate 同时提供：
//!
//! - 一个命令行工具 `fsearch`
//! - 一个可复用的库，包含搜索、输出、索引、配置四部分
//!
//! 使用示例：
//!
//! ```no_run
//! use fsearch::{search_file, SearchOptions};
//! use regex::Regex;
//! use std::path::Path;
//!
//! let re = Regex::new("fn main").unwrap();
//! let opts = SearchOptions::default();
//! let result = search_file(Path::new("src/main.rs"), &re, &opts).unwrap();
//! println!("{} 命中 {} 行", result.path, result.match_count);
//! ```

pub mod config;
pub mod index;
pub mod pattern;
pub mod printer;
pub mod search;

pub use config::{Config, CONFIG_FILENAME};
pub use index::{build_and_save, index_path, load_or_update, Index};
pub use pattern::{build_combined_pattern, build_pattern, literal_patterns};
pub use printer::{choose_color, JsonPrinter, Output, Printer};
pub use search::{search_file, FileResult, OutputLine, Prefilter, SearchOptions};
