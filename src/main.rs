mod cli;
mod printer;
mod search;

use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};

use anyhow::Result;
use clap::Parser;
use ignore::{overrides::OverrideBuilder, WalkBuilder, WalkState};
use regex::{Regex, RegexBuilder};
use std::process::ExitCode;

use crate::cli::Cli;
use crate::printer::{choose_color, JsonPrinter, Output, Printer};
use crate::search::SearchOptions;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Content,
    Name,
    Files,
}

fn main() -> ExitCode {
    match run() {
        Ok(found) => {
            if found {
                ExitCode::from(0)
            } else {
                ExitCode::from(1)
            }
        }
        Err(err) => {
            eprintln!("fsearch: {err:#}");
            ExitCode::from(2)
        }
    }
}

/// 根据 -F / -w 组装最终正则模式字符串
pub fn build_pattern(pattern: &str, fixed: bool, word: bool) -> String {
    let mut p = if fixed {
        regex::escape(pattern)
    } else {
        pattern.to_string()
    };
    if word {
        p = format!(r"\b(?:{})\b", p);
    }
    p
}

/// 把 --type 映射到扩展名列表
fn extensions_for_type(t: &str) -> Option<&'static [&'static str]> {
    Some(match t {
        "rust" | "rs" => &["rs"],
        "python" | "py" => &["py", "pyi"],
        "javascript" | "js" => &["js", "mjs", "cjs", "jsx"],
        "typescript" | "ts" => &["ts", "tsx"],
        "go" => &["go"],
        "c" => &["c", "h"],
        "cpp" | "c++" | "cc" => &["cpp", "cc", "cxx", "hpp", "hh", "hxx"],
        "java" => &["java"],
        "ruby" | "rb" => &["rb"],
        "php" => &["php"],
        "perl" => &["pl", "pm"],
        "markdown" | "md" => &["md", "markdown"],
        "toml" => &["toml"],
        "yaml" | "yml" => &["yaml", "yml"],
        "json" => &["json"],
        "xml" => &["xml"],
        "html" => &["htm", "html"],
        "css" => &["css"],
        "scss" => &["scss"],
        "shell" | "sh" | "bash" => &["sh", "bash", "zsh", "fish"],
        "sql" => &["sql"],
        "lua" => &["lua"],
        "swift" => &["swift"],
        "kotlin" | "kt" => &["kt", "kts"],
        "scala" => &["scala"],
        "haskell" | "hs" => &["hs", "lhs"],
        _ => return None,
    })
}

fn run() -> Result<bool> {
    let cli = Cli::parse();
    let start = std::time::Instant::now();

    // 选择模式
    let mode = if cli.files {
        Mode::Files
    } else if cli.name {
        Mode::Name
    } else {
        Mode::Content
    };

    // 处理位置参数歧义：--files 模式下唯一位置参数当作 path
    let (pattern_opt, path) = if mode == Mode::Files {
        let p = if cli.path == "." {
            cli.pattern.clone().unwrap_or_else(|| ".".to_string())
        } else {
            cli.path.clone()
        };
        (None, p)
    } else {
        (cli.pattern.clone(), cli.path.clone())
    };

    // 校验并构建正则
    let re: Option<Arc<Regex>> = match mode {
        Mode::Files => None,
        _ => {
            let pat = pattern_opt
                .clone()
                .ok_or_else(|| anyhow::anyhow!("缺少搜索模式"))?;
            let built = build_pattern(&pat, cli.fixed, cli.word);
            Some(Arc::new(
                RegexBuilder::new(&built)
                    .case_insensitive(cli.ignore_case)
                    .build()?,
            ))
        }
    };

    // before / after 优先级：-A/-B 覆盖 -C
    let before = cli.before.unwrap_or(cli.context);
    let after = cli.after.unwrap_or(cli.context);

    // 遍历器
    let mut builder = WalkBuilder::new(&path);
    builder
        .hidden(!cli.hidden)
        .follow_links(cli.follow)
        // 即使不在 git 仓库里，也读取 .gitignore
        .require_git(false);
    if cli.no_ignore {
        builder
            .ignore(false)
            .git_ignore(false)
            .git_global(false)
            .git_exclude(false);
    }

    // --glob 与 --type 通过 OverrideBuilder 白名单
    let has_overrides = cli.glob.is_some() || !cli.type_filter.is_empty();
    if has_overrides {
        let mut ob = OverrideBuilder::new(&path);
        if let Some(glob) = &cli.glob {
            ob.add(glob)?;
        }
        for t in &cli.type_filter {
            let exts = extensions_for_type(t).ok_or_else(|| {
                anyhow::anyhow!(
                    "未知的文件类型: {}（可用：rust, python, js, ts, go, c, cpp, markdown, toml, yaml, json 等）",
                    t
                )
            })?;
            for ext in exts {
                ob.add(&format!("*.{}", ext))?;
            }
        }
        builder.overrides(ob.build()?);
    }

    // 输出
    let color_choice = if cli.json {
        termcolor::ColorChoice::Never
    } else {
        choose_color(&cli.color)
    };
    let output = if cli.json {
        Output::Json(JsonPrinter::new())
    } else {
        Output::Plain(Printer::new(color_choice, cli.line_number))
    };
    let output = Arc::new(Mutex::new(output));

    // 共享统计
    let any_found = Arc::new(AtomicBool::new(false));
    let files_scanned = Arc::new(AtomicUsize::new(0));
    let files_matched = Arc::new(AtomicUsize::new(0));
    let total_matches = Arc::new(AtomicUsize::new(0));

    let files_with_matches = cli.files_with_matches;
    let count = cli.count;
    let invert = cli.invert;
    let only_matching = cli.only_matching;
    let max_count = cli.max_count;

    let walker = builder.build_parallel();

    walker.run(|| {
        let re = re.clone();
        let output = Arc::clone(&output);
        let any_found = Arc::clone(&any_found);
        let files_scanned = Arc::clone(&files_scanned);
        let files_matched = Arc::clone(&files_matched);
        let total_matches = Arc::clone(&total_matches);

        Box::new(move |result| {
            let entry = match result {
                Ok(e) => e,
                Err(_) => return WalkState::Continue,
            };

            if !entry.file_type().is_some_and(|ft| ft.is_file()) {
                return WalkState::Continue;
            }

            files_scanned.fetch_add(1, Ordering::Relaxed);
            let path = entry.path();
            let path_str = path.display().to_string();

            match mode {
                Mode::Files => {
                    any_found.store(true, Ordering::Relaxed);
                    if let Ok(mut out) = output.lock() {
                        let _ = out.print_filename_only(&path_str);
                    }
                }
                Mode::Name => {
                    if let Some(re) = &re {
                        let basename = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                        if re.is_match(basename) {
                            any_found.store(true, Ordering::Relaxed);
                            files_matched.fetch_add(1, Ordering::Relaxed);
                            if let Ok(mut out) = output.lock() {
                                let _ = out.print_filename_only(&path_str);
                            }
                        }
                    }
                }
                Mode::Content => {
                    if let Some(re) = &re {
                        let opts = SearchOptions {
                            files_with_matches,
                            count,
                            before,
                            after,
                            invert,
                            only_matching,
                            max_count,
                        };
                        if let Ok(file_result) = search::search_file(path, re, &opts) {
                            if file_result.matched {
                                any_found.store(true, Ordering::Relaxed);
                                files_matched.fetch_add(1, Ordering::Relaxed);
                                total_matches.fetch_add(file_result.match_count, Ordering::Relaxed);
                                if let Ok(mut out) = output.lock() {
                                    let _ = out.print_file_result(
                                        &file_result,
                                        files_with_matches,
                                        count,
                                    );
                                }
                            }
                        }
                    }
                }
            }

            WalkState::Continue
        })
    });

    let found = any_found.load(Ordering::Relaxed);
    let scanned = files_scanned.load(Ordering::Relaxed);
    let matched = files_matched.load(Ordering::Relaxed);
    let m_count = total_matches.load(Ordering::Relaxed);
    let elapsed = start.elapsed();

    if cli.stats {
        if cli.json {
            if let Ok(mut out) = output.lock() {
                let _ = out.print_summary(scanned, matched, m_count, elapsed.as_nanos());
            }
        } else {
            eprintln!(
                "fsearch: {} files scanned, {} files matched, {} matches, {:.2?}",
                scanned, matched, m_count, elapsed
            );
        }
    }

    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_regex_unchanged() {
        assert_eq!(build_pattern(r"fn\s+main", false, false), r"fn\s+main");
        assert_eq!(build_pattern(r"\d+", false, false), r"\d+");
    }

    #[test]
    fn fixed_escapes_special_chars() {
        assert_eq!(build_pattern("fn(", true, false), r"fn\(");
        assert_eq!(build_pattern("a.b*c", true, false), r"a\.b\*c");
        assert_eq!(build_pattern("a+b?c", true, false), r"a\+b\?c");
        assert_eq!(build_pattern("[]{}()", true, false), r"\[\]\{\}\(\)");
    }

    #[test]
    fn fixed_preserves_normal_chars() {
        assert_eq!(build_pattern("hello", true, false), "hello");
        assert_eq!(build_pattern("fn main", true, false), "fn main");
    }

    #[test]
    fn word_wraps_with_boundaries() {
        assert_eq!(build_pattern("fn", false, true), r"\b(?:fn)\b");
        assert_eq!(build_pattern(r"\d+", false, true), r"\b(?:\d+)\b");
    }

    #[test]
    fn fixed_and_word_combined() {
        assert_eq!(build_pattern("fn(", true, true), r"\b(?:fn\()\b");
    }

    #[test]
    fn word_match_whole_word_only() {
        let p = build_pattern("fn", false, true);
        let re = Regex::new(&p).unwrap();
        assert!(re.is_match("fn main"));
        assert!(re.is_match("let x = fn();"));
        assert!(!re.is_match("define"));
        assert!(!re.is_match("fnord"));
        assert!(!re.is_match("cthulhu_fn"));
    }

    #[test]
    fn fixed_matches_literal_paren() {
        let p = build_pattern("fn(", true, false);
        let re = Regex::new(&p).unwrap();
        assert!(re.is_match("fn(x)"));
        assert!(re.is_match("call fn( now"));
        assert!(!re.is_match("fn x"));
    }

    #[test]
    fn fixed_prevents_regex_interpretation() {
        let re_plain = Regex::new("a.c").unwrap();
        assert!(re_plain.is_match("abc"));

        let p = build_pattern("a.c", true, false);
        let re_fixed = Regex::new(&p).unwrap();
        assert!(re_fixed.is_match("a.c"));
        assert!(!re_fixed.is_match("abc"));
    }

    #[test]
    fn plain_regex_alternation_still_works() {
        let p = build_pattern("foo|bar", false, false);
        assert_eq!(p, "foo|bar");
        let re = Regex::new(&p).unwrap();
        assert!(re.is_match("foo"));
        assert!(re.is_match("bar"));
    }

    #[test]
    fn word_with_alternation() {
        let p = build_pattern("foo|bar", false, true);
        assert_eq!(p, r"\b(?:foo|bar)\b");
        let re = Regex::new(&p).unwrap();
        assert!(re.is_match("foo bar"));
        assert!(!re.is_match("foobar"));
    }

    #[test]
    fn type_extension_lookup() {
        assert_eq!(extensions_for_type("rust"), Some(&["rs"][..]));
        assert_eq!(extensions_for_type("py"), Some(&["py", "pyi"][..]));
        assert_eq!(
            extensions_for_type("markdown"),
            Some(&["md", "markdown"][..])
        );
        assert!(extensions_for_type("unknown").is_none());
    }
}
