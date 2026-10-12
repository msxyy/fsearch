mod cli;

use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};

use anyhow::Result;
use clap::Parser;
use ignore::{overrides::OverrideBuilder, WalkBuilder, WalkState};
use regex::{Regex, RegexBuilder};
use std::process::ExitCode;

use fsearch::pattern::{build_combined_pattern, literal_patterns};
use fsearch::search::FileResult;
use fsearch::{choose_color, index, JsonPrinter, Output, Prefilter, Printer, SearchOptions};

use crate::cli::Cli;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Content,
    Name,
    Files,
}

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

    if let Some(shell) = cli.generate_completions.clone() {
        generate_completions(&shell)?;
        return Ok(true);
    }
    if cli.generate_man {
        generate_man()?;
        return Ok(true);
    }

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

    let mode = if cli.files {
        Mode::Files
    } else if cli.name {
        Mode::Name
    } else {
        Mode::Content
    };

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

    // 构建字面量预筛器
    //
    // 只有满足以下条件时才启用：
    // - 有 pattern 列表
    // - 没有 -v（反向匹配时预筛会漏掉应该命中的行）
    // - pattern 全是字面量（含 -F，或本身不含正则元字符）
    // - 没有 -w（\b 边界不能直接用字面量表达）
    let prefilter: Option<Arc<Prefilter>> = if !cli.invert && mode == Mode::Content {
        patterns_opt.as_ref().and_then(|pats| {
            literal_patterns(pats, cli.fixed, cli.word)
                .and_then(|lits| Prefilter::new(&lits, cli.ignore_case))
                .map(Arc::new)
        })
    } else {
        None
    };

    // --use-index：加载或自动重建索引
    let candidate_paths: Option<Vec<String>> = if cli.use_index && mode == Mode::Content {
        let root_path = std::path::Path::new(&path);
        let (idx, rebuilt) = index::load_or_rebuild(root_path)?;
        if rebuilt {
            eprintln!("fsearch: 索引已建立或更新");
        }
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

    // 遍历器（非索引模式使用）
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
        // 索引模式：只遍历候选文件
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
                prefilter: prefilter.clone(),
            };
            if let Ok(r) = fsearch::search_file(p, re_ref, &opts) {
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
            let prefilter = prefilter.clone();
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
                        prefilter: prefilter.clone(),
                    };
                    if let Ok(r) = fsearch::search_file(path, re, &opts) {
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
            let prefilter = prefilter.clone();
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
                        prefilter: prefilter.clone(),
                    };
                    if let Ok(r) = fsearch::search_file(path, re, &opts) {
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
