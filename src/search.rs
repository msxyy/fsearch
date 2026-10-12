use std::{
    collections::VecDeque,
    fs::{self, File},
    io::{BufRead, BufReader, Read},
    path::Path,
    sync::Arc,
};

use aho_corasick::{AhoCorasick, AhoCorasickBuilder};
use anyhow::Result;
use encoding_rs::Encoding;
use regex::Regex;

/// 字面量预筛器
///
/// 比正则快得多，用来快速排除不可能命中的行。
/// **只用于快速排除**：`might_match*` 返回 false 的行一定不命中；
/// 返回 true 的行还需要交给 regex 复核。
pub struct Prefilter {
    ac: AhoCorasick,
}

impl Prefilter {
    /// 用字面量列表构建预筛器
    ///
    /// `case_insensitive=true` 时做 ASCII 级别的大小写不敏感匹配。
    /// 非 ASCII 字符的大小写仍按原样匹配，这与 ripgrep 的默认行为一致。
    pub fn new(literals: &[String], case_insensitive: bool) -> Option<Self> {
        if literals.is_empty() {
            return None;
        }
        let ac = AhoCorasickBuilder::new()
            .ascii_case_insensitive(case_insensitive)
            .build(literals)
            .ok()?;
        Some(Self { ac })
    }

    /// 这一行（UTF-8 字符串）是否可能命中？
    pub fn might_match(&self, line: &str) -> bool {
        self.ac.is_match(line)
    }

    /// 这一行（原始字节）是否可能命中？
    ///
    /// 比 `might_match(&str)` 快，因为跳过了 UTF-8 解码。
    /// 不命中的行可以直接在字节层面丢弃。
    pub fn might_match_bytes(&self, bytes: &[u8]) -> bool {
        self.ac.is_match(bytes)
    }
}

/// 打印用的输出行
pub enum OutputLine {
    Match {
        line_no: usize,
        line: String,
        matches: Vec<(usize, usize)>,
    },
    MatchOnly {
        line_no: usize,
        text: String,
    },
    Context {
        line_no: usize,
        line: String,
    },
    Separator,
}

/// 单文件搜索的完整结果
pub struct FileResult {
    pub path: String,
    pub matched: bool,
    pub match_count: usize,
    pub occurrence_count: usize,
    pub skipped: bool,
    pub output: Vec<OutputLine>,
}

/// 搜索选项
#[derive(Default)]
pub struct SearchOptions {
    pub files_with_matches: bool,
    pub files_without_match: bool,
    pub count: bool,
    pub count_matches: bool,
    pub before: usize,
    pub after: usize,
    pub invert: bool,
    pub only_matching: bool,
    pub max_count: Option<usize>,
    pub search_binary: bool,
    pub encoding: Option<&'static Encoding>,
    pub replace: Option<String>,
    /// 可选的预筛器；调用方负责只在合适的场景下传入
    pub prefilter: Option<Arc<Prefilter>>,
}

/// 判断是否是二进制文件：读取前 8KB，包含 \0 视为二进制
fn is_binary(path: &Path) -> bool {
    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return true,
    };
    let mut buf = [0u8; 8192];
    match file.read(&mut buf) {
        Ok(n) => buf[..n].contains(&0),
        Err(_) => true,
    }
}

/// 找到一行中所有匹配的字节区间
fn find_matches(re: &Regex, line: &str) -> Vec<(usize, usize)> {
    re.find_iter(line).map(|m| (m.start(), m.end())).collect()
}

/// 把一行中的所有匹配替换成 replacement
fn apply_replace(re: &Regex, line: &str, replacement: &str) -> String {
    re.replace_all(line, replacement).into_owned()
}

/// 流式读取来源
///
/// Stream 分支支持**字节级预筛**：当不需要上下文时，先把行当成字节
/// 交给预筛器，不命中的行直接跳过，省掉 UTF-8 解码。
///
/// 两种变体都自带 `line_no` 计数，`next_line` 返回真实行号，
/// 保证预筛跳过的行也计入行号。
enum LineSource {
    Stream {
        reader: BufReader<File>,
        buf: Vec<u8>,
        prefilter: Option<Arc<Prefilter>>,
        /// 是否跳过预筛（有上下文时不能跳过，因为上下文行要原样输出）
        skip_prefilter: bool,
        /// 文件里的真实行号（每读一行 +1，不管是否被预筛跳过）
        line_no: usize,
    },
    Memory {
        lines: std::vec::IntoIter<String>,
        line_no: usize,
    },
}

impl LineSource {
    /// 返回 `(真实行号, 行内容)`；到文件结尾返回 `None`
    fn next_line(&mut self) -> Option<(usize, String)> {
        match self {
            LineSource::Stream {
                reader,
                buf,
                prefilter,
                skip_prefilter,
                line_no,
            } => loop {
                buf.clear();
                let n = match reader.read_until(b'\n', buf) {
                    Ok(0) => return None,
                    Ok(n) => n,
                    Err(_) => return None,
                };
                // 每读一行，行号 +1，即使这行会被预筛跳过
                *line_no += 1;

                let mut end = n;
                if end > 0 && buf[end - 1] == b'\n' {
                    end -= 1;
                }
                if end > 0 && buf[end - 1] == b'\r' {
                    end -= 1;
                }

                // 字节级预筛：不命中的行直接跳过解码
                if !*skip_prefilter {
                    if let Some(pf) = prefilter {
                        if !pf.might_match_bytes(&buf[..end]) {
                            continue;
                        }
                    }
                }

                return Some((*line_no, String::from_utf8_lossy(&buf[..end]).into_owned()));
            },
            LineSource::Memory { lines, line_no } => {
                let line = lines.next()?;
                *line_no += 1;
                Some((*line_no, line))
            }
        }
    }
}

/// 对单个文件执行搜索，返回结果，不负责打印
///
/// 内存占用与文件大小无关：只保留 `opts.before` 行环形缓冲。
/// 若 `opts.encoding` 指定非 UTF-8，则整个文件会被解码到内存。
pub fn search_file(path: &Path, re: &Regex, opts: &SearchOptions) -> Result<FileResult> {
    let path_str = path.display().to_string();
    let mut result = FileResult {
        path: path_str,
        matched: false,
        match_count: 0,
        occurrence_count: 0,
        skipped: false,
        output: Vec::new(),
    };

    if matches!(opts.max_count, Some(0)) {
        return Ok(result);
    }

    if !opts.search_binary && is_binary(path) {
        result.skipped = true;
        return Ok(result);
    }

    let mut source = match opts.encoding {
        Some(enc) if !std::ptr::eq(enc, encoding_rs::UTF_8) => {
            let bytes = match fs::read(path) {
                Ok(b) => b,
                Err(_) => {
                    result.skipped = true;
                    return Ok(result);
                }
            };
            let (cow, _, _) = enc.decode(&bytes);
            let lines: Vec<String> = cow.lines().map(String::from).collect();
            LineSource::Memory {
                lines: lines.into_iter(),
                line_no: 0,
            }
        }
        _ => {
            let file = match File::open(path) {
                Ok(f) => f,
                Err(_) => {
                    result.skipped = true;
                    return Ok(result);
                }
            };
            // 有上下文时不能用字节级预筛（上下文行必须原样输出）
            let skip_prefilter = opts.before > 0 || opts.after > 0;
            LineSource::Stream {
                reader: BufReader::new(file),
                buf: Vec::new(),
                prefilter: opts.prefilter.clone(),
                skip_prefilter,
                line_no: 0,
            }
        }
    };

    let build_output = !opts.files_with_matches && !opts.files_without_match && !opts.count;
    let use_context = !opts.only_matching && (opts.before > 0 || opts.after > 0);
    let use_before = if opts.only_matching { 0 } else { opts.before };
    let use_after = if opts.only_matching { 0 } else { opts.after };

    let mut ring: VecDeque<(usize, String)> = VecDeque::new();
    let mut after_remaining: usize = 0;
    let mut last_printed: Option<usize> = None;
    let mut hit_limit = false;

    while let Some((line_no, line)) = source.next_line() {
        if hit_limit {
            if build_output && after_remaining > 0 {
                result.output.push(OutputLine::Context {
                    line_no,
                    line: line.clone(),
                });
                last_printed = Some(line_no);
                after_remaining -= 1;
                continue;
            }
            break;
        }

        // 字符串级预筛：只在 Stream 分支 skip_prefilter=false 时才会走到这里
        // （即：有上下文，字节级预筛被跳过，需要在 String 上再筛一次）
        let candidate = match (&opts.prefilter, use_context) {
            (Some(pf), true) => pf.might_match(&line),
            _ => true,
        };
        let is_match = if candidate { re.is_match(&line) } else { false };
        let hit = if opts.invert { !is_match } else { is_match };

        if hit {
            result.match_count += 1;
            result.matched = true;

            if opts.count_matches && !opts.invert {
                result.occurrence_count += re.find_iter(&line).count();
            } else {
                result.occurrence_count += 1;
            }

            if build_output {
                if use_context {
                    let before_start = line_no.saturating_sub(use_before).max(1);
                    for (lno, txt) in ring.iter() {
                        if *lno < before_start {
                            continue;
                        }
                        if let Some(lp) = last_printed {
                            if *lno <= lp {
                                continue;
                            }
                        }
                        if let Some(lp) = last_printed {
                            if *lno > lp + 1 {
                                result.output.push(OutputLine::Separator);
                            }
                        }
                        result.output.push(OutputLine::Context {
                            line_no: *lno,
                            line: txt.clone(),
                        });
                        last_printed = Some(*lno);
                    }
                }

                if opts.only_matching && !opts.invert {
                    let ms = find_matches(re, &line);
                    for (s, e) in ms {
                        result.output.push(OutputLine::MatchOnly {
                            line_no,
                            text: line[s..e].to_string(),
                        });
                    }
                } else {
                    if use_context {
                        if let Some(lp) = last_printed {
                            if line_no > lp + 1 {
                                result.output.push(OutputLine::Separator);
                            }
                        }
                    }

                    let display_line = match &opts.replace {
                        Some(rep) if !opts.invert => apply_replace(re, &line, rep),
                        _ => line.clone(),
                    };

                    let matches = if opts.invert || opts.replace.is_some() {
                        Vec::new()
                    } else {
                        find_matches(re, &line)
                    };
                    result.output.push(OutputLine::Match {
                        line_no,
                        line: display_line,
                        matches,
                    });
                }

                last_printed = Some(line_no);
                after_remaining = use_after;
            }

            if let Some(max) = opts.max_count {
                if result.match_count >= max {
                    hit_limit = true;
                    if after_remaining == 0 {
                        break;
                    }
                }
            }
        } else if build_output && after_remaining > 0 {
            result.output.push(OutputLine::Context {
                line_no,
                line: line.clone(),
            });
            last_printed = Some(line_no);
            after_remaining -= 1;
        }

        if build_output && use_before > 0 {
            ring.push_back((line_no, line));
            while ring.len() > use_before {
                ring.pop_front();
            }
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let mut p = std::env::temp_dir();
            p.push(format!(
                "fsearch_test_{}_{}_{}",
                tag,
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir_all(&p).unwrap();
            TempDir(p)
        }

        fn file(&self, name: &str, content: &str) -> PathBuf {
            let p = self.0.join(name);
            fs::write(&p, content).unwrap();
            p
        }

        fn file_bytes(&self, name: &str, content: &[u8]) -> PathBuf {
            let p = self.0.join(name);
            fs::write(&p, content).unwrap();
            p
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn default_opts() -> SearchOptions {
        SearchOptions::default()
    }

    fn re(pattern: &str) -> Regex {
        Regex::new(pattern).unwrap()
    }

    fn output_lines(result: &FileResult) -> Vec<String> {
        result
            .output
            .iter()
            .map(|o| match o {
                OutputLine::Match { line, .. } => format!("M:{}", line),
                OutputLine::MatchOnly { text, .. } => format!("O:{}", text),
                OutputLine::Context { line, .. } => format!("C:{}", line),
                OutputLine::Separator => "--".to_string(),
            })
            .collect()
    }

    #[test]
    fn finds_matching_line() {
        let dir = TempDir::new("find");
        let file = dir.file("a.txt", "hello\nworld\n");
        let r = search_file(&file, &re("world"), &default_opts()).unwrap();
        assert!(r.matched);
        assert_eq!(r.match_count, 1);
        assert_eq!(r.occurrence_count, 1);
        assert!(!r.skipped);
        assert_eq!(output_lines(&r), vec!["M:world"]);
    }

    #[test]
    fn no_match_returns_empty() {
        let dir = TempDir::new("nomatch");
        let file = dir.file("a.txt", "hello\nworld\n");
        let r = search_file(&file, &re("xyz"), &default_opts()).unwrap();
        assert!(!r.matched);
        assert_eq!(r.match_count, 0);
        assert_eq!(r.occurrence_count, 0);
        assert!(!r.skipped);
        assert!(r.output.is_empty());
    }

    #[test]
    fn multiple_matches_same_line_counted_once() {
        let dir = TempDir::new("multiline");
        let file = dir.file("a.txt", "foo foo foo\n");
        let r = search_file(&file, &re("foo"), &default_opts()).unwrap();
        assert_eq!(r.match_count, 1);
        assert_eq!(r.occurrence_count, 1);
    }

    #[test]
    fn count_matches_counts_occurrences() {
        let dir = TempDir::new("cm");
        let file = dir.file("a.txt", "foo foo foo\nbar foo\n");
        let mut opts = default_opts();
        opts.count = true;
        opts.count_matches = true;
        let r = search_file(&file, &re("foo"), &opts).unwrap();
        assert_eq!(r.match_count, 2);
        assert_eq!(r.occurrence_count, 4);
    }

    #[test]
    fn count_matches_with_invert_uses_lines() {
        let dir = TempDir::new("cm_inv");
        let file = dir.file("a.txt", "drop\nkeep\ndrop\n");
        let mut opts = default_opts();
        opts.count = true;
        opts.count_matches = true;
        opts.invert = true;
        let r = search_file(&file, &re("drop"), &opts).unwrap();
        assert_eq!(r.match_count, 1);
        assert_eq!(r.occurrence_count, 1);
    }

    #[test]
    fn invert_mode() {
        let dir = TempDir::new("invert");
        let file = dir.file("a.txt", "keep\ndrop\nkeep2\n");
        let mut opts = default_opts();
        opts.invert = true;
        let r = search_file(&file, &re("drop"), &opts).unwrap();
        assert!(r.matched);
        assert_eq!(r.match_count, 2);
        assert_eq!(output_lines(&r), vec!["M:keep", "M:keep2"]);
    }

    #[test]
    fn context_before_after() {
        let dir = TempDir::new("ctx");
        let file = dir.file("a.txt", "l1\nl2\nl3\nl4\nl5\n");
        let mut opts = default_opts();
        opts.before = 1;
        opts.after = 1;
        let r = search_file(&file, &re("l3"), &opts).unwrap();
        assert_eq!(output_lines(&r), vec!["C:l2", "M:l3", "C:l4"]);
    }

    #[test]
    fn context_separator_between_groups() {
        let dir = TempDir::new("sep");
        let file = dir.file("a.txt", "m1\nx\nx\nm2\n");

        let r = search_file(&file, &re("m"), &default_opts()).unwrap();
        assert_eq!(output_lines(&r), vec!["M:m1", "M:m2"]);

        let mut opts = default_opts();
        opts.after = 1;
        let r = search_file(&file, &re("m"), &opts).unwrap();
        assert_eq!(output_lines(&r), vec!["M:m1", "C:x", "--", "M:m2"]);
    }

    #[test]
    fn files_with_matches_mode() {
        let dir = TempDir::new("files");
        let file = dir.file("a.txt", "hit\nmiss\nhit\n");
        let mut opts = default_opts();
        opts.files_with_matches = true;
        let r = search_file(&file, &re("hit"), &opts).unwrap();
        assert!(r.matched);
        assert_eq!(r.match_count, 2);
        assert!(r.output.is_empty());
    }

    #[test]
    fn files_without_match_mode() {
        let dir = TempDir::new("files_without");
        let file = dir.file("a.txt", "hello\nworld\n");
        let mut opts = default_opts();
        opts.files_without_match = true;
        let r = search_file(&file, &re("xyz"), &opts).unwrap();
        assert!(!r.matched);
        assert!(!r.skipped);
        assert!(r.output.is_empty());
    }

    #[test]
    fn count_mode() {
        let dir = TempDir::new("count");
        let file = dir.file("a.txt", "hit\nmiss\nhit\n");
        let mut opts = default_opts();
        opts.count = true;
        let r = search_file(&file, &re("hit"), &opts).unwrap();
        assert_eq!(r.match_count, 2);
        assert!(r.output.is_empty());
    }

    #[test]
    fn binary_skipped_by_default() {
        let dir = TempDir::new("bin_skip");
        let file = dir.file_bytes("a.bin", b"hello\0world");
        let r = search_file(&file, &re("hello"), &default_opts()).unwrap();
        assert!(!r.matched);
        assert!(r.skipped);
    }

    #[test]
    fn binary_searched_when_enabled() {
        let dir = TempDir::new("bin_on");
        let file = dir.file_bytes("a.bin", b"hello\0world");
        let mut opts = default_opts();
        opts.search_binary = true;
        let r = search_file(&file, &re("hello"), &opts).unwrap();
        assert!(r.matched);
        assert_eq!(r.match_count, 1);
        assert!(!r.skipped);
    }

    #[test]
    fn non_utf8_file_supported() {
        let dir = TempDir::new("nonutf8");
        let path = dir.file_bytes("a.txt", b"hello\n\xff\xfe\nworld\n");

        let r = search_file(&path, &re("world"), &default_opts()).unwrap();
        assert!(r.matched);

        let r2 = search_file(&path, &re("hello"), &default_opts()).unwrap();
        assert!(r2.matched);
    }

    #[test]
    fn only_matching_outputs_each_match() {
        let dir = TempDir::new("om");
        let file = dir.file("a.txt", "foo bar foo\nbaz\n");
        let mut opts = default_opts();
        opts.only_matching = true;
        let r = search_file(&file, &re("foo"), &opts).unwrap();
        assert_eq!(output_lines(&r), vec!["O:foo", "O:foo"]);
    }

    #[test]
    fn only_matching_ignores_context() {
        let dir = TempDir::new("om_ctx");
        let file = dir.file("a.txt", "l1\nl2 foo\nl3\n");
        let mut opts = default_opts();
        opts.only_matching = true;
        opts.before = 5;
        opts.after = 5;
        let r = search_file(&file, &re("foo"), &opts).unwrap();
        assert_eq!(output_lines(&r), vec!["O:foo"]);
    }

    #[test]
    fn only_matching_with_invert_falls_back() {
        let dir = TempDir::new("om_inv");
        let file = dir.file("a.txt", "keep\ndrop\n");
        let mut opts = default_opts();
        opts.only_matching = true;
        opts.invert = true;
        let r = search_file(&file, &re("drop"), &opts).unwrap();
        assert_eq!(output_lines(&r), vec!["M:keep"]);
    }

    #[test]
    fn max_count_limits_matches() {
        let dir = TempDir::new("maxc");
        let file = dir.file("a.txt", "hit\nhit\nhit\nhit\n");
        let mut opts = default_opts();
        opts.max_count = Some(2);
        let r = search_file(&file, &re("hit"), &opts).unwrap();
        assert_eq!(r.match_count, 2);
        assert_eq!(output_lines(&r), vec!["M:hit", "M:hit"]);
    }

    #[test]
    fn max_count_zero_returns_empty() {
        let dir = TempDir::new("maxc0");
        let file = dir.file("a.txt", "hit\n");
        let mut opts = default_opts();
        opts.max_count = Some(0);
        let r = search_file(&file, &re("hit"), &opts).unwrap();
        assert!(!r.matched);
        assert!(r.output.is_empty());
    }

    #[test]
    fn max_count_with_context_still_stops_at_limit() {
        let dir = TempDir::new("maxc_ctx");
        let file = dir.file("a.txt", "l1\nhit\nl3\nhit\nl5\n");
        let mut opts = default_opts();
        opts.max_count = Some(1);
        opts.before = 1;
        opts.after = 1;
        let r = search_file(&file, &re("hit"), &opts).unwrap();
        assert_eq!(output_lines(&r), vec!["C:l1", "M:hit", "C:l3"]);
    }

    #[test]
    fn encoding_gbk_decodes() {
        let dir = TempDir::new("gbk");
        let (bytes, _, _) = encoding_rs::GBK.encode("你好\nhello\n");
        let file = dir.file_bytes("a.txt", &bytes);

        let mut opts = default_opts();
        opts.encoding = Some(encoding_rs::GBK);
        let r = search_file(&file, &re("你好"), &opts).unwrap();
        assert!(r.matched);

        let r2 = search_file(&file, &re("hello"), &opts).unwrap();
        assert!(r2.matched);
    }

    #[test]
    fn encoding_utf8_is_streaming() {
        let dir = TempDir::new("utf8_explicit");
        let file = dir.file("a.txt", "hello\nworld\n");

        let mut opts = default_opts();
        opts.encoding = Some(encoding_rs::UTF_8);
        let r = search_file(&file, &re("world"), &opts).unwrap();
        assert!(r.matched);
        assert_eq!(output_lines(&r), vec!["M:world"]);
    }

    #[test]
    fn replace_substitutes_matches() {
        let dir = TempDir::new("replace");
        let file = dir.file("a.txt", "foo bar foo\n");
        let mut opts = default_opts();
        opts.replace = Some("X".to_string());
        let r = search_file(&file, &re("foo"), &opts).unwrap();
        assert_eq!(output_lines(&r), vec!["M:X bar X"]);
    }

    #[test]
    fn replace_with_capture_group() {
        let dir = TempDir::new("replace_cap");
        let file = dir.file("a.txt", "fn foo\n");
        let mut opts = default_opts();
        opts.replace = Some("[$1]".to_string());
        let r = search_file(&file, &re(r"fn (\w+)"), &opts).unwrap();
        assert_eq!(output_lines(&r), vec!["M:[foo]"]);
    }

    #[test]
    fn replace_not_applied_when_invert() {
        let dir = TempDir::new("replace_inv");
        let file = dir.file("a.txt", "keep\ndrop\n");
        let mut opts = default_opts();
        opts.invert = true;
        opts.replace = Some("X".to_string());
        let r = search_file(&file, &re("drop"), &opts).unwrap();
        assert_eq!(output_lines(&r), vec!["M:keep"]);
    }

    #[test]
    fn replace_not_applied_in_only_matching() {
        let dir = TempDir::new("replace_om");
        let file = dir.file("a.txt", "foo bar foo\n");
        let mut opts = default_opts();
        opts.only_matching = true;
        opts.replace = Some("X".to_string());
        let r = search_file(&file, &re("foo"), &opts).unwrap();
        assert_eq!(output_lines(&r), vec!["O:foo", "O:foo"]);
    }

    // ---------- 字面量预筛 ----------

    #[test]
    fn prefilter_accepts_matching_lines() {
        let pf = Prefilter::new(&["needle".to_string()], false).unwrap();
        assert!(pf.might_match("a needle here"));
        assert!(!pf.might_match("no match here"));
    }

    #[test]
    fn prefilter_ascii_case_insensitive() {
        let pf = Prefilter::new(&["needle".to_string()], true).unwrap();
        assert!(pf.might_match("A NEEDLE Here"));
        assert!(pf.might_match("a needle here"));
    }

    #[test]
    fn prefilter_multiple_literals() {
        let pf = Prefilter::new(&["foo".to_string(), "bar".to_string()], false).unwrap();
        assert!(pf.might_match("has foo"));
        assert!(pf.might_match("has bar"));
        assert!(!pf.might_match("has baz"));
    }

    #[test]
    fn prefilter_used_in_search() {
        let dir = TempDir::new("prefilter");
        let file = dir.file("a.txt", "first line\nneedle here\nthird line\n");
        let pf = Arc::new(Prefilter::new(&["needle".to_string()], false).unwrap());
        let mut opts = default_opts();
        opts.prefilter = Some(pf);
        let r = search_file(&file, &re("needle"), &opts).unwrap();
        assert!(r.matched);
        assert_eq!(r.match_count, 1);
    }

    #[test]
    fn prefilter_does_not_miss_case_sensitive() {
        let dir = TempDir::new("pf_case");
        let file = dir.file("a.txt", "NEEDLE\nneedle\nNeedle\n");
        let pf = Arc::new(Prefilter::new(&["needle".to_string()], false).unwrap());
        let mut opts = default_opts();
        opts.prefilter = Some(pf);
        let r = search_file(&file, &re("needle"), &opts).unwrap();
        assert_eq!(r.match_count, 1);
    }

    // ---------- 字节级预筛 ----------

    #[test]
    fn prefilter_bytes_matches() {
        let pf = Prefilter::new(&["needle".to_string()], false).unwrap();
        assert!(pf.might_match_bytes(b"a needle here"));
        assert!(!pf.might_match_bytes(b"no match here"));
    }

    #[test]
    fn prefilter_bytes_works_with_non_utf8() {
        let pf = Prefilter::new(&["needle".to_string()], false).unwrap();
        assert!(pf.might_match_bytes(b"\xff\xfeneedle\xff"));
        assert!(!pf.might_match_bytes(b"\xff\xfe\xff"));
    }

    #[test]
    fn byte_prefilter_skips_without_context() {
        let dir = TempDir::new("byte_pf");
        let file = dir.file("a.txt", "first\nneedle\nthird\n");
        let pf = Arc::new(Prefilter::new(&["needle".to_string()], false).unwrap());
        let mut opts = default_opts();
        opts.prefilter = Some(pf);
        let r = search_file(&file, &re("needle"), &opts).unwrap();
        assert_eq!(r.match_count, 1);
        assert_eq!(output_lines(&r), vec!["M:needle"]);
    }

    #[test]
    fn byte_prefilter_skips_with_context() {
        let dir = TempDir::new("byte_pf_ctx");
        let file = dir.file("a.txt", "l1\nneedle\nl3\n");
        let pf = Arc::new(Prefilter::new(&["needle".to_string()], false).unwrap());
        let mut opts = default_opts();
        opts.prefilter = Some(pf);
        opts.before = 1;
        opts.after = 1;
        let r = search_file(&file, &re("needle"), &opts).unwrap();
        assert_eq!(output_lines(&r), vec!["C:l1", "M:needle", "C:l3"]);
    }

    #[test]
    fn byte_prefilter_non_utf8_with_context() {
        let dir = TempDir::new("byte_pf_nonutf8");
        let path = dir.file_bytes("a.txt", b"line1\n\xff\xfe\nline3\n");
        let pf = Arc::new(Prefilter::new(&["needle".to_string()], false).unwrap());
        let mut opts = default_opts();
        opts.prefilter = Some(pf);
        let r = search_file(&path, &re("xyz"), &opts).unwrap();
        assert!(!r.matched);
    }

    // ---------- 行号正确性（v0.7.0 修复） ----------

    #[test]
    fn line_number_correct_with_prefilter() {
        let dir = TempDir::new("line_no_pf");
        let mut content = String::new();
        for _ in 0..10 {
            content.push_str("filler line\n");
        }
        content.push_str("has needle here\n");
        content.push_str("more filler\n");
        let file = dir.file("a.txt", &content);

        let pf = Arc::new(Prefilter::new(&["needle".to_string()], false).unwrap());
        let mut opts = default_opts();
        opts.prefilter = Some(pf);

        let r = search_file(&file, &re("needle"), &opts).unwrap();
        assert_eq!(r.match_count, 1);
        match &r.output[0] {
            OutputLine::Match { line_no, .. } => {
                assert_eq!(*line_no, 11, "预筛跳过的行也必须计入行号");
            }
            _ => panic!("期望 Match 行"),
        }
    }

    #[test]
    fn line_number_correct_without_prefilter() {
        let dir = TempDir::new("line_no_nopf");
        let mut content = String::new();
        for _ in 0..10 {
            content.push_str("filler line\n");
        }
        content.push_str("has needle here\n");
        let file = dir.file("a.txt", &content);

        let r = search_file(&file, &re("needle"), &default_opts()).unwrap();
        match &r.output[0] {
            OutputLine::Match { line_no, .. } => assert_eq!(*line_no, 11),
            _ => panic!("期望 Match 行"),
        }
    }
}
