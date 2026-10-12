use std::{
    collections::VecDeque,
    fs::{self, File},
    io::{BufRead, BufReader, Read},
    path::Path,
};

use anyhow::Result;
use encoding_rs::Encoding;
use regex::Regex;

/// 打印用的输出行
pub enum OutputLine {
    Match {
        line_no: usize,
        line: String,
        matches: Vec<(usize, usize)>,
    },
    /// 只输出匹配文本（-o 模式）
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
    pub output: Vec<OutputLine>,
}

/// 搜索选项
pub struct SearchOptions {
    pub files_with_matches: bool,
    pub count: bool,
    pub before: usize,
    pub after: usize,
    pub invert: bool,
    pub only_matching: bool,
    /// 每个文件最多输出 N 条匹配；None 表示不限制
    pub max_count: Option<usize>,
    /// 是否搜索二进制文件（-uuu 时启用）
    pub search_binary: bool,
    /// 文件编码；None 表示 UTF-8（流式），Some 表示指定编码（整体解码）
    pub encoding: Option<&'static Encoding>,
    /// 替换文本；None 表示不替换
    pub replace: Option<String>,
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
enum LineSource {
    /// 逐字节读行，UTF-8 lossy
    Stream {
        reader: BufReader<File>,
        buf: Vec<u8>,
    },
    /// 已整体解码到内存的行
    Memory(std::vec::IntoIter<String>),
}

impl LineSource {
    fn next_line(&mut self) -> Option<String> {
        match self {
            LineSource::Stream { reader, buf } => {
                buf.clear();
                match reader.read_until(b'\n', buf) {
                    Ok(0) => None,
                    Ok(n) => {
                        let mut end = n;
                        if end > 0 && buf[end - 1] == b'\n' {
                            end -= 1;
                        }
                        if end > 0 && buf[end - 1] == b'\r' {
                            end -= 1;
                        }
                        Some(String::from_utf8_lossy(&buf[..end]).into_owned())
                    }
                    Err(_) => None,
                }
            }
            LineSource::Memory(it) => it.next(),
        }
    }
}

/// 对单个文件执行搜索，返回结果，不负责打印
pub fn search_file(path: &Path, re: &Regex, opts: &SearchOptions) -> Result<FileResult> {
    let path_str = path.display().to_string();
    let mut result = FileResult {
        path: path_str,
        matched: false,
        match_count: 0,
        output: Vec::new(),
    };

    // -m 0：直接返回空结果
    if matches!(opts.max_count, Some(0)) {
        return Ok(result);
    }

    // 默认跳过二进制；-uuu 时也搜索
    if !opts.search_binary && is_binary(path) {
        return Ok(result);
    }

    // 选择行来源：UTF-8 走流式，其他编码整体解码
    let mut source = match opts.encoding {
        Some(enc) if !std::ptr::eq(enc, encoding_rs::UTF_8) => {
            let bytes = match fs::read(path) {
                Ok(b) => b,
                Err(_) => return Ok(result),
            };
            let (cow, _, _) = enc.decode(&bytes);
            let lines: Vec<String> = cow.lines().map(String::from).collect();
            LineSource::Memory(lines.into_iter())
        }
        _ => {
            let file = match File::open(path) {
                Ok(f) => f,
                Err(_) => return Ok(result),
            };
            LineSource::Stream {
                reader: BufReader::new(file),
                buf: Vec::new(),
            }
        }
    };

    let build_output = !opts.files_with_matches && !opts.count;
    let use_context = !opts.only_matching && (opts.before > 0 || opts.after > 0);
    let use_before = if opts.only_matching { 0 } else { opts.before };
    let use_after = if opts.only_matching { 0 } else { opts.after };

    let mut ring: VecDeque<(usize, String)> = VecDeque::new();
    let mut after_remaining: usize = 0;
    let mut last_printed: Option<usize> = None;
    let mut line_no: usize = 0;
    let mut hit_limit = false;

    while let Some(line) = source.next_line() {
        line_no += 1;

        // 已达上限：只输出当前匹配剩余 after 上下文
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

        let is_match = re.is_match(&line);
        let hit = if opts.invert { !is_match } else { is_match };

        if hit {
            result.match_count += 1;
            result.matched = true;

            if build_output {
                // 1) before 上下文（-o 模式跳过）
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

                // 2) -o 模式：只输出匹配文本
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

                    // 替换优先：如果指定 --replace，输出替换后的整行
                    let (display_line, matches) = match &opts.replace {
                        Some(rep) if !opts.invert => {
                            // 替换后匹配位置已改变，不再高亮
                            (apply_replace(re, &line, rep), Vec::new())
                        }
                        _ => {
                            let m = if opts.invert {
                                Vec::new()
                            } else {
                                find_matches(re, &line)
                            };
                            (line.clone(), m)
                        }
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
        SearchOptions {
            files_with_matches: false,
            count: false,
            before: 0,
            after: 0,
            invert: false,
            only_matching: false,
            max_count: None,
            search_binary: false,
            encoding: None,
            replace: None,
        }
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
        assert_eq!(output_lines(&r), vec!["M:world"]);
    }

    #[test]
    fn no_match_returns_empty() {
        let dir = TempDir::new("nomatch");
        let file = dir.file("a.txt", "hello\nworld\n");
        let r = search_file(&file, &re("xyz"), &default_opts()).unwrap();
        assert!(!r.matched);
        assert_eq!(r.match_count, 0);
        assert!(r.output.is_empty());
    }

    #[test]
    fn multiple_matches_same_line_counted_once() {
        let dir = TempDir::new("multiline");
        let file = dir.file("a.txt", "foo foo foo\n");
        let r = search_file(&file, &re("foo"), &default_opts()).unwrap();
        assert_eq!(r.match_count, 1);
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
    }

    #[test]
    fn non_utf8_file_supported() {
        let dir = TempDir::new("nonutf8");
        let path = dir.file_bytes("a.txt", b"hello\n\xff\xfe\nworld\n");

        let r = search_file(&path, &re("world"), &default_opts()).unwrap();
        assert!(r.matched);
        assert_eq!(r.match_count, 1);

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
        assert_eq!(r.match_count, 1);

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
        assert!(r.matched);
        assert_eq!(output_lines(&r), vec!["M:X bar X"]);
    }

    #[test]
    fn replace_clears_matches() {
        // 回归测试：替换后 matches 应为空，避免 printer 用旧区间切片越界
        let dir = TempDir::new("replace_clear");
        let file = dir.file("a.txt", "foo bar foo\n");
        let mut opts = default_opts();
        opts.replace = Some("X".to_string());
        let r = search_file(&file, &re("foo"), &opts).unwrap();
        match &r.output[0] {
            OutputLine::Match { matches, .. } => assert!(matches.is_empty()),
            _ => panic!("expected Match"),
        }
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
}
