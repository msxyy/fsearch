mod cli;
mod index;
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
use crate::search::{FileResult, SearchOptions};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Content,
    Name,
    Files,
}

/// 排序模式下收集的每个文件的搜索结果
type CollectedResults = Arc<Mutex<Vec<(String, Option<FileResult>)>>>;

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

fn generate_completions(shell: &str) -> Result<()> {
    use clap::CommandFactory;
    use clap_complete::{generate, shells};
    use std::io;

    let mut cmd = Cli::command();
    match shell.to_lowercase().as_str() {
        "bash" => generate(shells::Bash, &mut cmd, "fsearch", &mut io::stdout()),
        "zsh" => generate(shells::Zsh, &mut cmd, "fsearch", &mut io::stdout()),
        "fish" => generate(shells::Fish, &mut cmd, "fsearch", &mut io::stdout()),
        "powershell" | "pwsh" => {
            generate(shells::PowerShell, &mut cmd, "fsearch", &mut io::stdout())
        }
        "elvish" => generate(shells::Elvish, &mut cmd, "fsearch", &mut io::stdout()),
        other => anyhow::bail!(
            "不支持的 shell: {}（支持 bash, zsh, fish, powershell, elvish）",
            other
        ),
    }
    Ok(())
}

fn generate_man() -> Result<()> {
    use clap::CommandFactory;
    use clap_mangen::Man;
    use std::io::Write;

    let cmd = Cli::command();
    let man = Man::new(cmd);
    let mut buf = Vec::new();
    man.render(&mut buf)?;
    std::io::stdout().write_all(&buf)?;
    Ok(())
}

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

pub fn build_combined_pattern(patterns: &[String], fixed: bool, word: bool) -> String {
    if patterns.is_empty() {
        return String::new();
    }
    if patterns.len() == 1 {
        return build_pattern(&patterns[0], fixed, word);
    }
    let escaped: Vec<String> = patterns
        .iter()
        .map(|p| if fixed { regex::escape(p) } else { p.clone() })
        .collect();
    let joined = escaped.join("|");
    if word {
        format!(r"\b(?:{})\b", joined)
    } else {
        format!(r"(?:{})", joined)
    }
}

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

fn parse_encoding(name: Option<&str>) -> Result<Option<&'static encoding_rs::Encoding>> {
    let name = match name {
        None => return Ok(None),
        Some(n) => n,
    };
    match encoding_rs::Encoding::for_label(name.as_bytes()) {
        Some(enc) if std::ptr::eq(enc, encoding_rs::UTF_8) => Ok(None),
        Some(enc) => Ok(Some(enc)),
        None => anyhow::bail!(
            "未知编码: {}（可用：gbk, gb18030, big5, shift_jis, euc-kr, windows-1252 等）",
            name
        ),
    }
}

fn parse_size(s: &str) -> Result<u64> {
    let s = s.trim();
    let (num_str, mult) = if let Some(n) = s.strip_suffix(['K', 'k']) {
        (n, 1024u64)
    } else if let Some(n) = s.strip_suffix(['M', 'm']) {
        (n, 1024u64 * 1024)
    } else if let Some(n) = s.strip_suffix(['G', 'g']) {
        (n, 1024u64 * 1024 * 1024)
    } else if let Some(n) = s.strip_suffix(['B', 'b']) {
        (n, 1u64)
    } else {
        (s, 1u64)
    };
    let n: f64 = num_str
        .parse()
        .map_err(|_| anyhow::anyhow!("无效的文件大小: {}（例如 10M、1G、500K）", s))?;
    Ok((n * mult as f64) as u64)
}

fn run() -> Result<bool> {
    let cli = Cli::parse();

    // 隐藏 flag：生成补全 / man
    if let Some(shell) = cli.generate_completions.clone() {
        generate_completions(&shell)?;
        return Ok(true);
    }
    if cli.generate_man {
        generate_man()?;
        return Ok(true);
    }

    // --build-index <dir>：建立索引后退出
    if let Some(root) = cli.build_index.as_deref() {
        let root_path = std::path::Path::new(root);
        let n = index::build_and_save(root_path)?;
        let idx_path = index::index_path(root_path);
        eprintln!(
            "fsearch: 已索引 {} 个文件，保存到 {}",
            n,
            idx_path.display()
        );
        return Ok(true);
    }

    let start = std::time::Instant::now();

    // 模式选择
    let mode = if cli.files {
        Mode::Files
    } else if cli.name {
        Mode::Name
    } else {
        Mode::Content
    };

    // 位置参数处理
    let (patterns_opt, path): (Option<Vec<String>>, String) = if mode == Mode::Files {
        let p = if cli.path == "." {
            cli.pattern.clone().unwrap_or_else(|| ".".to_string())
        } else {
            cli.path.clone()
        };
        (None, p)
    } else if !cli.patterns.is_empty() {
        let search_path = if cli.path != "." {
            cli.path.clone()
        } else {
            cli.pattern.clone().unwrap_or_else(|| ".".to_string())
        };
        (Some(cli.patterns.clone()), search_path)
    } else {
        let mut pats = Vec::new();
        if let Some(p) = &cli.pattern {
            pats.push(p.clone());
        }
        (Some(pats), cli.path.clone())
    };

    // 构建正则
    let re: Option<Arc<Regex>> = match mode {
        Mode::Files => None,
        _ => {
            let pats = patterns_opt
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("缺少搜索模式"))?;
            if pats.is_empty() {
                anyhow::bail!("缺少搜索模式（请用位置参数或 -e 提供）");
            }
            let built = build_combined_pattern(pats, cli.fixed, cli.word);
            Some(Arc::new(
                RegexBuilder::new(&built)
                    .case_insensitive(cli.ignore_case)
                    .build()?,
            ))
        }
    };

    let before = cli.before.unwrap_or(cli.context);
    let after = cli.after.unwrap_or(cli.context);

    let no_ignore = cli.no_ignore || cli.unrestricted >= 1;
    let want_hidden = cli.hidden || cli.unrestricted >= 2;
    let search_binary = cli.unrestricted >= 3;

    let encoding = parse_encoding(cli.encoding.as_deref())?;
    let max_filesize = match cli.max_filesize.as_deref() {
        Some(s) => Some(parse_size(s)?),
        None => None,
    };

    let sort_output = cli.sort;
    let quiet = cli.quiet;

    let count = cli.count || cli.count_matches;
    let count_matches = cli.count_matches;
    let files_without_match = cli.files_without_match;

    let replace: Option<Arc<String>> = if cli.json
        || cli.files_with_matches
        || files_without_match
        || count
        || cli.only_matching
    {
        None
    } else {
        cli.replace.clone().map(Arc::new)
    };

    // --use-index：加载索引，得到候选文件列表
    let candidate_paths: Option<Vec<String>> = if cli.use_index && mode == Mode::Content {
        let root_path = std::path::Path::new(&path);
        let idx_path = index::index_path(root_path);
        if !idx_path.exists() {
            anyhow::bail!(
                "索引不存在: {}。请先运行 `fsearch --build-index {}`",
                idx_path.display(),
                path
            );
        }
        let idx = index::Index::load(&idx_path)?;
        match patterns_opt.as_ref().and_then(|v| v.first()) {
            Some(first) => match idx.candidates(first) {
                Some(c) => Some(c),
                None => {
                    eprintln!(
                        "fsearch: pattern 含正则元字符或长度不足 3，索引模式仅支持字面量搜索，回退到全量扫描"
                    );
                    None
                }
            },
            None => None,
        }
    } else {
        None
    };

    // 遍历器（仅在非索引模式下使用）
    let mut builder = WalkBuilder::new(&path);
    builder
        .hidden(!want_hidden)
        .follow_links(cli.follow)
        .require_git(false);
    if let Some(d) = cli.max_depth {
        builder.max_depth(Some(d));
    }
    if no_ignore {
        builder
            .ignore(false)
            .git_ignore(false)
            .git_global(false)
            .git_exclude(false);
    }

    let has_overrides =
        cli.glob.is_some() || !cli.type_filter.is_empty() || !cli.type_not_filter.is_empty();
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
        for t in &cli.type_not_filter {
            let exts =
                extensions_for_type(t).ok_or_else(|| anyhow::anyhow!("未知的文件类型: {}", t))?;
            for ext in exts {
                ob.add(&format!("!*.{}", ext))?;
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
    let mut output = if quiet {
        Output::Null
    } else if cli.json {
        Output::Json(JsonPrinter::new())
    } else {
        Output::Plain(Printer::new(color_choice, cli.line_number))
    };

    let any_found = Arc::new(AtomicBool::new(false));
    let files_scanned = Arc::new(AtomicUsize::new(0));
    let files_matched = Arc::new(AtomicUsize::new(0));
    let total_matches = Arc::new(AtomicUsize::new(0));

    let files_with_matches = cli.files_with_matches;
    let invert = cli.invert;
    let only_matching = cli.only_matching;
    let max_count = cli.max_count;
    let name_mode = mode == Mode::Name;
    let files_mode = mode == Mode::Files;

    if let Some(paths) = candidate_paths {
        // 索引模式：直接遍历候选文件
        let re_ref = re
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("内部错误：索引模式下正则缺失"))?;
        let shared = Arc::new(Mutex::new(output));
        for path_str in &paths {
            files_scanned.fetch_add(1, Ordering::Relaxed);
            let p = std::path::Path::new(path_str);
            if let Some(max) = max_filesize {
                if let Ok(meta) = std::fs::metadata(p) {
                    if meta.len() > max {
                        continue;
                    }
                }
            }
            let opts = SearchOptions {
                files_with_matches,
                files_without_match,
                count,
                count_matches,
                before,
                after,
                invert,
                only_matching,
                max_count,
                search_binary,
                encoding,
                replace: replace.as_ref().map(|s| (**s).clone()),
            };
            if let Ok(r) = search::search_file(p, re_ref, &opts) {
                let interesting = if files_without_match {
                    !r.matched && !r.skipped
                } else {
                    r.matched
                };
                if interesting {
                    any_found.store(true, Ordering::Relaxed);
                    files_matched.fetch_add(1, Ordering::Relaxed);
                    if r.matched {
                        total_matches.fetch_add(r.match_count, Ordering::Relaxed);
                    }
                    let mut out = shared.lock().unwrap();
                    if files_without_match {
                        let _ = out.print_filename_only(path_str);
                    } else {
                        let _ = out.print_file_result(&r, files_with_matches, count, count_matches);
                    }
                }
            }
        }
        output = Arc::try_unwrap(shared)
            .map_err(|_| anyhow::anyhow!("内部错误：输出锁未释放"))?
            .into_inner()
            .unwrap();
    } else if sort_output {
        let collected: CollectedResults = Arc::new(Mutex::new(Vec::new()));

        let walker = builder.build_parallel();
        walker.run(|| {
            let re = re.clone();
            let replace = replace.clone();
            let collected = Arc::clone(&collected);
            let files_scanned = Arc::clone(&files_scanned);
            let any_found = Arc::clone(&any_found);
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

                if files_mode {
                    any_found.store(true, Ordering::Relaxed);
                    collected.lock().unwrap().push((path_str, None));
                } else if name_mode {
                    if let Some(re) = &re {
                        let basename = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                        if re.is_match(basename) {
                            any_found.store(true, Ordering::Relaxed);
                            files_matched.fetch_add(1, Ordering::Relaxed);
                            collected.lock().unwrap().push((path_str, None));
                        }
                    }
                } else if let Some(re) = &re {
                    if let Some(max) = max_filesize {
                        if let Ok(meta) = std::fs::metadata(path) {
                            if meta.len() > max {
                                return WalkState::Continue;
                            }
                        }
                    }
                    let opts = SearchOptions {
                        files_with_matches,
                        files_without_match,
                        count,
                        count_matches,
                        before,
                        after,
                        invert,
                        only_matching,
                        max_count,
                        search_binary,
                        encoding,
                        replace: replace.as_ref().map(|s| (**s).clone()),
                    };
                    if let Ok(r) = search::search_file(path, re, &opts) {
                        let interesting = if files_without_match {
                            !r.matched && !r.skipped
                        } else {
                            r.matched
                        };
                        if interesting {
                            any_found.store(true, Ordering::Relaxed);
                            files_matched.fetch_add(1, Ordering::Relaxed);
                            if r.matched {
                                total_matches.fetch_add(r.match_count, Ordering::Relaxed);
                            }
                            collected.lock().unwrap().push((path_str, Some(r)));
                        }
                    }
                }
                WalkState::Continue
            })
        });

        let mut list = Arc::try_unwrap(collected)
            .map_err(|_| anyhow::anyhow!("内部错误：结果锁未释放"))?
            .into_inner()
            .unwrap();
        list.sort_by(|a, b| a.0.cmp(&b.0));
        for (path_str, fr) in list {
            match fr {
                None => {
                    let _ = output.print_filename_only(&path_str);
                }
                Some(r) => {
                    if files_without_match {
                        let _ = output.print_filename_only(&path_str);
                    } else {
                        let _ =
                            output.print_file_result(&r, files_with_matches, count, count_matches);
                    }
                }
            }
        }
    } else {
        let shared = Arc::new(Mutex::new(output));

        let walker = builder.build_parallel();
        walker.run(|| {
            let re = re.clone();
            let replace = replace.clone();
            let shared = Arc::clone(&shared);
            let files_scanned = Arc::clone(&files_scanned);
            let any_found = Arc::clone(&any_found);
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

                if files_mode {
                    any_found.store(true, Ordering::Relaxed);
                    if let Ok(mut out) = shared.lock() {
                        let _ = out.print_filename_only(&path_str);
                    }
                } else if name_mode {
                    if let Some(re) = &re {
                        let basename = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                        if re.is_match(basename) {
                            any_found.store(true, Ordering::Relaxed);
                            files_matched.fetch_add(1, Ordering::Relaxed);
                            if let Ok(mut out) = shared.lock() {
                                let _ = out.print_filename_only(&path_str);
                            }
                        }
                    }
                } else if let Some(re) = &re {
                    if let Some(max) = max_filesize {
                        if let Ok(meta) = std::fs::metadata(path) {
                            if meta.len() > max {
                                return WalkState::Continue;
                            }
                        }
                    }
                    let opts = SearchOptions {
                        files_with_matches,
                        files_without_match,
                        count,
                        count_matches,
                        before,
                        after,
                        invert,
                        only_matching,
                        max_count,
                        search_binary,
                        encoding,
                        replace: replace.as_ref().map(|s| (**s).clone()),
                    };
                    if let Ok(r) = search::search_file(path, re, &opts) {
                        let interesting = if files_without_match {
                            !r.matched && !r.skipped
                        } else {
                            r.matched
                        };
                        if interesting {
                            any_found.store(true, Ordering::Relaxed);
                            files_matched.fetch_add(1, Ordering::Relaxed);
                            if r.matched {
                                total_matches.fetch_add(r.match_count, Ordering::Relaxed);
                            }
                            if let Ok(mut out) = shared.lock() {
                                if files_without_match {
                                    let _ = out.print_filename_only(&path_str);
                                } else {
                                    let _ = out.print_file_result(
                                        &r,
                                        files_with_matches,
                                        count,
                                        count_matches,
                                    );
                                }
                            }
                        }
                    }
                }
                WalkState::Continue
            })
        });

        output = Arc::try_unwrap(shared)
            .map_err(|_| anyhow::anyhow!("内部错误：输出锁未释放"))?
            .into_inner()
            .unwrap();
    }

    let found = any_found.load(Ordering::Relaxed);
    let scanned = files_scanned.load(Ordering::Relaxed);
    let matched = files_matched.load(Ordering::Relaxed);
    let m_count = total_matches.load(Ordering::Relaxed);
    let elapsed = start.elapsed();

    if cli.stats {
        if cli.json {
            let _ = output.print_summary(scanned, matched, m_count, elapsed.as_nanos());
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

    #[test]
    fn encoding_none_returns_none() {
        assert!(parse_encoding(None).unwrap().is_none());
    }

    #[test]
    fn encoding_utf8_label_returns_none() {
        assert!(parse_encoding(Some("utf-8")).unwrap().is_none());
        assert!(parse_encoding(Some("UTF-8")).unwrap().is_none());
    }

    #[test]
    fn encoding_gbk_returns_some() {
        let enc = parse_encoding(Some("gbk")).unwrap();
        assert!(enc.is_some());
        assert_eq!(enc.unwrap().name(), "GBK");
    }

    #[test]
    fn encoding_big5_returns_some() {
        let enc = parse_encoding(Some("big5")).unwrap();
        assert!(enc.is_some());
        assert_eq!(enc.unwrap().name(), "Big5");
    }

    #[test]
    fn encoding_unknown_errors() {
        assert!(parse_encoding(Some("not-a-real-encoding")).is_err());
    }

    #[test]
    fn size_bytes() {
        assert_eq!(parse_size("1024").unwrap(), 1024);
        assert_eq!(parse_size("1024B").unwrap(), 1024);
    }

    #[test]
    fn size_kilo_mega_giga() {
        assert_eq!(parse_size("10K").unwrap(), 10 * 1024);
        assert_eq!(parse_size("10k").unwrap(), 10 * 1024);
        assert_eq!(parse_size("10M").unwrap(), 10 * 1024 * 1024);
        assert_eq!(parse_size("1G").unwrap(), 1024 * 1024 * 1024);
        assert_eq!(parse_size("1.5M").unwrap(), (1.5 * 1024.0 * 1024.0) as u64);
    }

    #[test]
    fn size_invalid_errors() {
        assert!(parse_size("abc").is_err());
    }

    #[test]
    fn combined_single_pattern() {
        let pats = vec!["fn".to_string()];
        assert_eq!(build_combined_pattern(&pats, false, false), "fn");
        assert_eq!(build_combined_pattern(&pats, false, true), r"\b(?:fn)\b");
    }

    #[test]
    fn combined_multiple_patterns() {
        let pats = vec!["foo".to_string(), "bar".to_string()];
        assert_eq!(build_combined_pattern(&pats, false, false), r"(?:foo|bar)");
        assert_eq!(
            build_combined_pattern(&pats, false, true),
            r"\b(?:foo|bar)\b"
        );
    }

    #[test]
    fn combined_fixed_escapes_each() {
        let pats = vec!["a.b".to_string(), "c*d".to_string()];
        assert_eq!(build_combined_pattern(&pats, true, false), r"(?:a\.b|c\*d)");
    }
}
