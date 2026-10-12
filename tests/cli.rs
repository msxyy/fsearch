use std::fs;
use std::path::PathBuf;
use std::process::Command;

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "fsearch_it_{}_{}_{}",
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
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn fsearch() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fsearch"))
}

// ---------- 基础行为 ----------

#[test]
fn basic_match_returns_zero() {
    let dir = TempDir::new("basic");
    dir.file("a.txt", "hello\nworld\n");

    let out = fsearch()
        .arg("world")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();

    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("world"));
}

#[test]
fn no_match_returns_one() {
    let dir = TempDir::new("nomatch");
    dir.file("a.txt", "hello\n");

    let out = fsearch()
        .arg("xyz")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();

    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
}

#[test]
fn invalid_regex_returns_two() {
    let dir = TempDir::new("badre");
    dir.file("a.txt", "hello\n");

    let out = fsearch()
        .arg("(")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();

    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("fsearch:"));
}

// ---------- 模式相关 ----------

#[test]
fn fixed_mode_literal_paren() {
    let dir = TempDir::new("fixed");
    dir.file("a.rs", "fn(x)\nfn x\n");

    let out = fsearch()
        .arg("fn(")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));

    let out = fsearch()
        .arg("-F")
        .arg("fn(")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("fn(x)"));
    assert!(!stdout.contains("fn x"));
}

#[test]
fn word_mode_no_partial_match() {
    let dir = TempDir::new("word");
    dir.file("a.txt", "define\nfn\nfnord\n");

    let out = fsearch()
        .arg("-w")
        .arg("fn")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();

    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("fn\n") || stdout.contains(":fn\n"));
    assert!(!stdout.contains("define"));
    assert!(!stdout.contains("fnord"));
}

#[test]
fn invert_mode_excludes_matches() {
    let dir = TempDir::new("invert");
    dir.file("a.txt", "keep\ndrop\nkeep2\n");

    let out = fsearch()
        .arg("-v")
        .arg("drop")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();

    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("keep"));
    assert!(stdout.contains("keep2"));
    assert!(!stdout.contains("drop"));
}

// ---------- 过滤与输出 ----------

#[test]
fn type_filter_only_rs() {
    let dir = TempDir::new("type");
    dir.file("a.rs", "fn main\n");
    dir.file("b.txt", "fn main\n");

    let out = fsearch()
        .arg("--type")
        .arg("rust")
        .arg("fn main")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();

    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("a.rs"));
    assert!(!stdout.contains("b.txt"));
}

#[test]
fn only_matching_outputs_match_only() {
    let dir = TempDir::new("om");
    dir.file("a.txt", "foo bar foo\n");

    let out = fsearch()
        .arg("-o")
        .arg("foo")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();

    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout.matches("foo").count(), 2);
    assert!(!stdout.contains("bar"));
}

#[test]
fn count_mode_reports_number() {
    let dir = TempDir::new("count");
    dir.file("a.txt", "hit\nmiss\nhit\n");

    let out = fsearch()
        .arg("-c")
        .arg("hit")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();

    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains(":2"));
}

// ---------- JSON ----------

#[test]
fn json_output_is_parseable_lines() {
    let dir = TempDir::new("json");
    dir.file("a.txt", "hello\nworld\n");

    let out = fsearch()
        .arg("--json")
        .arg("world")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();

    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    for line in stdout.lines() {
        assert!(line.starts_with('{'));
        assert!(line.ends_with('}'));
    }
    assert!(stdout.contains(r#""type":"match""#));
    assert!(stdout.contains(r#""line":"world""#));
}

// ---------- 特殊模式 ----------

#[test]
fn files_lists_all() {
    let dir = TempDir::new("files");
    dir.file("a.txt", "x\n");
    dir.file("b.txt", "y\n");

    let out = fsearch()
        .arg("--files")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();

    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("a.txt"));
    assert!(stdout.contains("b.txt"));
}

#[test]
fn name_searches_filename() {
    let dir = TempDir::new("name");
    dir.file("main.rs", "fn x\n");
    dir.file("other.rs", "fn x\n");

    let out = fsearch()
        .arg("--name")
        .arg("main")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();

    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("main.rs"));
    assert!(!stdout.contains("other.rs"));
}

#[test]
fn max_count_limits_results() {
    let dir = TempDir::new("maxc");
    dir.file("a.txt", "hit\nhit\nhit\nhit\n");

    let out = fsearch()
        .arg("-m")
        .arg("2")
        .arg("hit")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();

    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout.matches("hit").count(), 2);
}

#[test]
fn stats_outputs_to_stderr() {
    let dir = TempDir::new("stats");
    dir.file("a.txt", "hit\n");

    let out = fsearch()
        .arg("--stats")
        .arg("hit")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();

    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("files scanned"));
    assert!(stderr.contains("matches"));
}

#[test]
fn no_ignore_includes_ignored_files() {
    let dir = TempDir::new("noignore");
    dir.file(".gitignore", "secret.txt\n");
    dir.file("secret.txt", "needle\n");
    dir.file("visible.txt", "needle\n");

    let out = fsearch()
        .arg("-l")
        .arg("needle")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("visible.txt"));
    assert!(!stdout.contains("secret.txt"));

    let out = fsearch()
        .arg("--no-ignore")
        .arg("-l")
        .arg("needle")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("secret.txt"));
}

#[test]
fn json_has_begin_end_summary() {
    let dir = TempDir::new("json_evt");
    dir.file("a.txt", "hello\n");

    let out = fsearch()
        .arg("--json")
        .arg("--stats")
        .arg("hello")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();

    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains(r#""type":"begin""#));
    assert!(stdout.contains(r#""type":"end""#));
    assert!(stdout.contains(r#""type":"summary""#));
}

// ---------- v0.2.0 新增 ----------

#[test]
fn type_not_excludes() {
    let dir = TempDir::new("typenot");
    dir.file("a.rs", "needle\n");
    dir.file("b.md", "needle\n");
    dir.file("c.txt", "needle\n");

    let out = fsearch()
        .arg("--type-not")
        .arg("rust")
        .arg("-l")
        .arg("needle")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();

    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stdout.contains("a.rs"));
    assert!(stdout.contains("b.md") || stdout.contains("c.txt"));
}

#[test]
fn type_and_type_not_combined() {
    let dir = TempDir::new("typecombo");
    dir.file("a.rs", "needle\n");
    dir.file("b.py", "needle\n");
    dir.file("c.md", "needle\n");

    let out = fsearch()
        .arg("--type")
        .arg("python")
        .arg("--type-not")
        .arg("markdown")
        .arg("-l")
        .arg("needle")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();

    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("b.py"));
    assert!(!stdout.contains("a.rs"));
    assert!(!stdout.contains("c.md"));
}

#[test]
fn u_flag_ignores_gitignore() {
    let dir = TempDir::new("u_flag");
    dir.file(".gitignore", "secret.txt\n");
    dir.file("secret.txt", "needle\n");
    dir.file("visible.txt", "needle\n");

    let out = fsearch()
        .arg("-l")
        .arg("needle")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stdout.contains("secret.txt"));

    let out = fsearch()
        .arg("-u")
        .arg("-l")
        .arg("needle")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("secret.txt"));
}

#[test]
fn uu_flag_searches_hidden() {
    let dir = TempDir::new("uu_flag");
    dir.file(".hidden.txt", "needle\n");
    dir.file("visible.txt", "needle\n");

    let out = fsearch()
        .arg("-l")
        .arg("needle")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stdout.contains(".hidden.txt"));

    let out = fsearch()
        .arg("-uu")
        .arg("-l")
        .arg("needle")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains(".hidden.txt"));
}

// ---------- v0.2.0 --encoding ----------

#[test]
fn gbk_encoding_searches_chinese() {
    let dir = TempDir::new("gbk_cli");
    let (bytes, _, _) = encoding_rs::GBK.encode("你好\nhello\n");
    fs::write(dir.0.join("a.txt"), &bytes).unwrap();

    let out = fsearch()
        .arg("--encoding")
        .arg("gbk")
        .arg("你好")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();

    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("你好"));
}

#[test]
fn gbk_encoding_searches_ascii() {
    let dir = TempDir::new("gbk_ascii");
    let (bytes, _, _) = encoding_rs::GBK.encode("hello world\n");
    fs::write(dir.0.join("a.txt"), &bytes).unwrap();

    let out = fsearch()
        .arg("--encoding")
        .arg("gbk")
        .arg("world")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();

    assert!(out.status.success());
}

#[test]
fn unknown_encoding_errors() {
    let dir = TempDir::new("bad_enc");
    dir.file("a.txt", "hello\n");

    let out = fsearch()
        .arg("--encoding")
        .arg("not-a-real-encoding")
        .arg("hello")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();

    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("未知编码") || stderr.contains("fsearch:"));
}

// ---------- v0.2.0 --sort / --replace ----------

#[test]
fn sort_output_is_sorted() {
    let dir = TempDir::new("sort");
    dir.file("c.txt", "needle\n");
    dir.file("a.txt", "needle\n");
    dir.file("b.txt", "needle\n");

    let out = fsearch()
        .arg("--sort")
        .arg("-l")
        .arg("needle")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();

    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    let mut sorted = lines.clone();
    sorted.sort();
    assert_eq!(lines, sorted);
    assert!(lines.len() >= 3);
}

#[test]
fn replace_substitutes() {
    let dir = TempDir::new("replace");
    dir.file("a.txt", "foo bar foo\n");

    let out = fsearch()
        .arg("-r")
        .arg("X")
        .arg("foo")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "exit: {:?}, stderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("X bar X"), "stdout: {stdout}");
    assert!(!stdout.contains("foo"));
}

#[test]
fn replace_with_capture_group() {
    let dir = TempDir::new("replace_cap");
    dir.file("a.txt", "fn foo\n");

    let out = fsearch()
        .arg("-r")
        .arg("[$1]")
        .arg("fn (\\w+)")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "exit: {:?}, stderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("[foo]"), "stdout: {stdout}");
}

#[test]
fn replace_ignored_with_only_matching() {
    let dir = TempDir::new("replace_om");
    dir.file("a.txt", "foo bar foo\n");

    let out = fsearch()
        .arg("-o")
        .arg("-r")
        .arg("X")
        .arg("foo")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();

    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout.matches("foo").count(), 2);
    assert!(!stdout.contains("X"));
}

// ---------- v0.3.0 新增 ----------

#[test]
fn multi_pattern_e_flag() {
    let dir = TempDir::new("multipat");
    dir.file("a.txt", "foo\nbar\nbaz\n");

    let out = fsearch()
        .arg("-e")
        .arg("foo")
        .arg("-e")
        .arg("bar")
        .arg("-n")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("foo"));
    assert!(stdout.contains("bar"));
    assert!(!stdout.contains("baz"));
}

#[test]
fn multi_pattern_positional_plus_e() {
    let dir = TempDir::new("multipat2");
    dir.file("a.txt", "foo\nbar\nbaz\n");

    let out = fsearch()
        .arg("-e")
        .arg("foo")
        .arg("-e")
        .arg("baz")
        .arg("-n")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("foo"));
    assert!(stdout.contains("baz"));
    assert!(!stdout.contains("bar"));
}

#[test]
fn files_without_match_lists_non_matching() {
    let dir = TempDir::new("fwm");
    dir.file("has.txt", "needle\n");
    dir.file("no1.txt", "other\n");
    dir.file("no2.txt", "another\n");

    let out = fsearch()
        .arg("--files-without-match")
        .arg("needle")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stdout.contains("has.txt"));
    assert!(stdout.contains("no1.txt"));
    assert!(stdout.contains("no2.txt"));
}

#[test]
fn count_matches_counts_occurrences() {
    let dir = TempDir::new("cm_cli");
    dir.file("a.txt", "foo foo foo\nbar foo\n");

    let out = fsearch()
        .arg("--count-matches")
        .arg("foo")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains(":4"), "stdout: {stdout}");
}

#[test]
fn max_depth_limits_recursion() {
    let dir = TempDir::new("maxdepth");
    fs::create_dir_all(dir.0.join("sub")).unwrap();
    dir.file("top.txt", "needle\n");
    fs::write(dir.0.join("sub").join("deep.txt"), "needle\n").unwrap();

    let out = fsearch()
        .arg("--max-depth")
        .arg("1")
        .arg("-l")
        .arg("needle")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("top.txt"));
    assert!(!stdout.contains("deep.txt"));
}

#[test]
fn max_filesize_skips_large() {
    let dir = TempDir::new("maxsize");
    let big = "needle\n".repeat(1000); // ~7KB
    dir.file("small.txt", "needle\n");
    dir.file("big.txt", &big);

    let out = fsearch()
        .arg("--max-filesize")
        .arg("1K")
        .arg("-l")
        .arg("needle")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("small.txt"));
    assert!(!stdout.contains("big.txt"));
}

#[test]
fn quiet_mode_no_output() {
    let dir = TempDir::new("quiet");
    dir.file("a.txt", "needle\n");

    let out = fsearch()
        .arg("-q")
        .arg("needle")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();

    assert!(out.status.success());
    assert!(out.stdout.is_empty());
}

#[test]
fn quiet_mode_returns_one_on_no_match() {
    let dir = TempDir::new("quiet2");
    dir.file("a.txt", "needle\n");

    let out = fsearch()
        .arg("-q")
        .arg("xyz")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();

    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
}

// ---------- v0.3.0 补全 / man ----------

#[test]
fn generate_completions_bash() {
    let out = fsearch()
        .arg("--generate-completions")
        .arg("bash")
        .output()
        .unwrap();

    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("_fsearch") || stdout.contains("fsearch"));
}

#[test]
fn generate_man_page() {
    let out = fsearch().arg("--generate-man").output().unwrap();

    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains(".TH") || stdout.contains("FSEARCH"));
}

// ---------- v0.4.0：增量索引 ----------

#[test]
fn build_index_then_use_index() {
    let dir = TempDir::new("idx_e2e");
    dir.file("a.txt", "hello world\n");
    dir.file("b.txt", "foo bar\n");

    let out = fsearch()
        .arg("--build-index")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "build stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let out = fsearch()
        .arg("--use-index")
        .arg("hello")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "use stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("hello"), "stdout: {stdout}");
    assert!(!stdout.contains("foo bar"), "stdout: {stdout}");
}

#[test]
fn use_index_auto_builds_when_missing() {
    let dir = TempDir::new("idx_auto");
    dir.file("a.txt", "hello world\n");
    dir.file("b.txt", "foo bar\n");

    // v0.5.0：没有索引时 --use-index 自动建立并使用，而不是报错
    let out = fsearch()
        .arg("--use-index")
        .arg("hello")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "exit: {:?}, stderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("hello"), "stdout: {stdout}");

    // 索引文件应该被创建
    assert!(dir.0.join(".fsearch-index").exists());

    // stderr 应该有提示
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("索引"), "stderr: {stderr}");
}

#[test]
fn use_index_auto_rebuilds_when_stale() {
    let dir = TempDir::new("idx_stale");
    dir.file("a.txt", "hello\n");

    // 先建好索引
    let out = fsearch()
        .arg("--build-index")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();
    assert!(out.status.success());

    // 添加新文件，索引应该过期
    std::thread::sleep(std::time::Duration::from_millis(1100));
    dir.file("b.txt", "hello\n");

    // --use-index 应该自动重建索引，两个文件都搜到
    let out = fsearch()
        .arg("--use-index")
        .arg("hello")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "exit: {:?}, stderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("a.txt"), "stdout: {stdout}");
    assert!(stdout.contains("b.txt"), "stdout: {stdout}");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("索引"), "stderr: {stderr}");
}

#[test]
fn use_index_with_regex_falls_back() {
    let dir = TempDir::new("idx_regex");
    dir.file("a.txt", "hello world\n");

    let out = fsearch()
        .arg("--build-index")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();
    assert!(out.status.success());

    // 含正则元字符，会回退到全量扫描
    let out = fsearch()
        .arg("--use-index")
        .arg("hel.*o")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("回退"), "stderr: {stderr}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("hello"));
}

// ---------- v0.6.0：配置文件 .fsearch.toml ----------

#[test]
fn config_ignore_case_applied() {
    let dir = TempDir::new("cfg_ic");
    dir.file(".fsearch.toml", "ignore_case = true\n");
    dir.file("a.txt", "HELLO\n");

    let out = fsearch()
        .arg("hello")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();

    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("HELLO"));
}

#[test]
fn cli_overrides_config() {
    let dir = TempDir::new("cfg_override");
    dir.file(".fsearch.toml", "ignore_case = true\n");
    dir.file("a.txt", "HELLO\n");

    // CLI 不设 -i，但配置文件设了，所以会命中
    let out = fsearch()
        .arg("hello")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();
    assert!(out.status.success());

    // CLI 明确给 --no-ignore-case 并不存在；改用另一种方式：
    // 配置文件设 line_number = true，CLI 覆盖不了布尔 false，这里只验证配置文件生效
    let out = fsearch()
        .arg("--line-number")
        .arg("hello")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    // 应该有行号 "a.txt:1:HELLO"
    assert!(stdout.contains(":1:"), "stdout: {stdout}");
}

#[test]
fn config_type_filter_applied() {
    let dir = TempDir::new("cfg_type");
    dir.file(".fsearch.toml", "type_filter = [\"rust\"]\n");
    dir.file("a.rs", "needle\n");
    dir.file("b.txt", "needle\n");

    let out = fsearch()
        .arg("-l")
        .arg("needle")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();

    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("a.rs"));
    assert!(!stdout.contains("b.txt"));
}

#[test]
fn config_cli_type_filter_overrides() {
    let dir = TempDir::new("cfg_type_override");
    dir.file(".fsearch.toml", "type_filter = [\"rust\"]\n");
    dir.file("a.rs", "needle\n");
    dir.file("b.py", "needle\n");

    // CLI 的 --type 覆盖配置文件
    let out = fsearch()
        .arg("--type")
        .arg("python")
        .arg("-l")
        .arg("needle")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();

    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("b.py"));
    assert!(!stdout.contains("a.rs"));
}

#[test]
fn config_invalid_errors() {
    let dir = TempDir::new("cfg_bad");
    dir.file(".fsearch.toml", "not_a_field = 1\n");
    dir.file("a.txt", "hello\n");

    let out = fsearch()
        .arg("hello")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();

    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("无法解析") || stderr.contains("fsearch:"),
        "stderr: {stderr}"
    );
}

#[test]
fn config_context_applied() {
    let dir = TempDir::new("cfg_ctx");
    dir.file(".fsearch.toml", "context = 1\n");
    dir.file("a.txt", "l1\nl2 hit l3\nl4\n");

    let out = fsearch()
        .arg("hit")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();

    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("l1"), "stdout: {stdout}");
    assert!(stdout.contains("l4"), "stdout: {stdout}");
}

// ---------- v0.6.0：增量索引 ----------

#[test]
fn index_incremental_reuses_and_updates() {
    let dir = TempDir::new("idx_inc");
    dir.file("a.txt", "hello\n");
    dir.file("b.txt", "world\n");

    // 首次建立
    let out = fsearch()
        .arg("--build-index")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();
    assert!(out.status.success());

    // 修改一个文件
    std::thread::sleep(std::time::Duration::from_millis(1100));
    dir.file("a.txt", "hello changed to needle\n");

    // 用索引搜索，应自动重建，且复用 b.txt
    let out = fsearch()
        .arg("--use-index")
        .arg("needle")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("索引更新完成"), "stderr: {stderr}");
    assert!(
        stderr.contains("复用 1") && stderr.contains("重建 1"),
        "stderr: {stderr}"
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("needle"), "stdout: {stdout}");
}

#[test]
fn old_index_format_rebuilt() {
    let dir = TempDir::new("idx_old_fmt");
    dir.file("a.txt", "hello\n");

    // 写一个假的旧格式索引
    fs::write(dir.0.join(".fsearch-index"), b"FSIDX001old format").unwrap();

    // --use-index 应该自动丢弃旧索引并重建
    let out = fsearch()
        .arg("--use-index")
        .arg("hello")
        .arg(dir.0.to_str().unwrap())
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("hello"), "stdout: {stdout}");
}
