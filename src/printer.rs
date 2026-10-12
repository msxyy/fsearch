use std::io::{IsTerminal, Write};

use anyhow::Result;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::search::{FileResult, OutputLine};

/// 根据 color 参数与 stdout 是否终端决定是否启用颜色
pub fn choose_color(choice: &str) -> ColorChoice {
    match choice {
        "always" => ColorChoice::Always,
        "never" => ColorChoice::Never,
        _ => {
            if std::io::stdout().is_terminal() {
                ColorChoice::Auto
            } else {
                ColorChoice::Never
            }
        }
    }
}

/// 输出接口：普通文本 / JSON / 静默
pub enum Output {
    Plain(Printer),
    Json(JsonPrinter),
    /// --quiet：什么都不输出
    Null,
}

impl Output {
    pub fn print_file_result(
        &mut self,
        result: &FileResult,
        files_with_matches: bool,
        count: bool,
        count_matches: bool,
    ) -> Result<()> {
        match self {
            Output::Plain(p) => {
                p.print_file_result(result, files_with_matches, count, count_matches)
            }
            Output::Json(p) => {
                p.print_file_result(result, files_with_matches, count, count_matches)
            }
            Output::Null => Ok(()),
        }
    }

    pub fn print_filename_only(&mut self, path: &str) -> Result<()> {
        match self {
            Output::Plain(p) => p.print_filename_pub(path),
            Output::Json(p) => p.print_file_event(path),
            Output::Null => Ok(()),
        }
    }

    pub fn print_summary(
        &mut self,
        files_scanned: usize,
        files_matched: usize,
        matches: usize,
        elapsed_nanos: u128,
    ) -> Result<()> {
        match self {
            Output::Plain(_) => Ok(()),
            Output::Json(p) => {
                p.print_summary(files_scanned, files_matched, matches, elapsed_nanos)
            }
            Output::Null => Ok(()),
        }
    }
}

/// 普通文本输出
pub struct Printer {
    stdout: StandardStream,
    show_line_number: bool,
    show_filename: bool,
    show_column: bool,
    vimgrep: bool,
    heading: bool,
    null_separator: bool,
    /// 上一个打印的文件路径（heading 模式用来判断是否需要重新打印文件名）
    last_heading: Option<String>,
}

impl Printer {
    pub fn new(choice: ColorChoice, show_line_number: bool) -> Self {
        Self {
            stdout: StandardStream::stdout(choice),
            show_line_number,
            show_filename: true,
            show_column: false,
            vimgrep: false,
            heading: false,
            null_separator: false,
            last_heading: None,
        }
    }

    pub fn with_filename(mut self, v: bool) -> Self {
        self.show_filename = v;
        self
    }

    pub fn with_column(mut self, v: bool) -> Self {
        self.show_column = v;
        self
    }

    pub fn with_vimgrep(mut self, v: bool) -> Self {
        self.vimgrep = v;
        // vimgrep 隐含显示列号
        if v {
            self.show_column = true;
        }
        self
    }

    pub fn with_heading(mut self, v: bool) -> Self {
        self.heading = v;
        self
    }

    pub fn with_null_separator(mut self, v: bool) -> Self {
        self.null_separator = v;
        self
    }

    pub fn print_file_result(
        &mut self,
        result: &FileResult,
        files_with_matches: bool,
        count: bool,
        count_matches: bool,
    ) -> Result<()> {
        if files_with_matches {
            return self.print_filename_pub(&result.path);
        }
        if count {
            let n = if count_matches {
                result.occurrence_count
            } else {
                result.match_count
            };
            return self.print_count(&result.path, n);
        }

        // vimgrep 模式：每个匹配单独一行 file:line:col:content
        if self.vimgrep {
            return self.print_vimgrep(result);
        }

        // heading 模式：文件名单独一行，匹配行不带文件名
        if self.heading {
            let saved = self.show_filename;
            self.show_filename = false;
            let mut printed_heading = false;
            for line in &result.output {
                if matches!(
                    line,
                    OutputLine::Match { .. } | OutputLine::MatchOnly { .. }
                ) && !printed_heading
                {
                    self.print_heading(&result.path)?;
                    printed_heading = true;
                }
                self.print_one_line(&result.path, line)?;
            }
            self.show_filename = saved;
            return Ok(());
        }

        // 默认模式
        for line in &result.output {
            self.print_one_line(&result.path, line)?;
        }
        Ok(())
    }

    /// vimgrep 输出：对每个匹配都输出 file:line:col:content
    fn print_vimgrep(&mut self, result: &FileResult) -> Result<()> {
        for line in &result.output {
            match line {
                OutputLine::Match {
                    line_no,
                    line,
                    matches,
                } => {
                    if matches.is_empty() {
                        // 无高亮信息（如 -v 或 -r），只输出一次
                        self.print_vimgrep_prefix(&result.path, *line_no, 1)?;
                        writeln!(self.stdout, "{}", line)?;
                    } else {
                        for &(start, _end) in matches {
                            self.print_vimgrep_prefix(&result.path, *line_no, start + 1)?;
                            writeln!(self.stdout, "{}", line)?;
                        }
                    }
                }
                OutputLine::MatchOnly { line_no, text } => {
                    self.print_vimgrep_prefix(&result.path, *line_no, 1)?;
                    writeln!(self.stdout, "{}", text)?;
                }
                OutputLine::Context { .. } | OutputLine::Separator => {
                    // vimgrep 模式忽略上下文和分隔符
                }
            }
        }
        Ok(())
    }

    fn print_vimgrep_prefix(&mut self, path: &str, line_no: usize, col: usize) -> Result<()> {
        if self.show_filename {
            self.stdout
                .set_color(ColorSpec::new().set_fg(Some(Color::Magenta)).set_bold(true))?;
            write!(self.stdout, "{}", path)?;
            self.stdout
                .set_color(ColorSpec::new().set_fg(Some(Color::Cyan)))?;
            write!(self.stdout, ":")?;
            self.stdout.reset()?;
        }
        self.stdout
            .set_color(ColorSpec::new().set_fg(Some(Color::Green)))?;
        write!(self.stdout, "{}", line_no)?;
        self.stdout
            .set_color(ColorSpec::new().set_fg(Some(Color::Cyan)))?;
        write!(self.stdout, ":")?;
        self.stdout.reset()?;
        self.stdout
            .set_color(ColorSpec::new().set_fg(Some(Color::Green)))?;
        write!(self.stdout, "{}", col)?;
        self.stdout
            .set_color(ColorSpec::new().set_fg(Some(Color::Cyan)))?;
        write!(self.stdout, ":")?;
        self.stdout.reset()?;
        Ok(())
    }

    /// 打印单行
    fn print_one_line(&mut self, path: &str, line: &OutputLine) -> Result<()> {
        match line {
            OutputLine::Match {
                line_no,
                line,
                matches,
            } => {
                self.print_match(path, *line_no, line, matches)?;
            }
            OutputLine::MatchOnly { line_no, text } => {
                self.print_match_only(path, *line_no, text)?;
            }
            OutputLine::Context { line_no, line } => {
                self.print_context(path, *line_no, line, false)?;
            }
            OutputLine::Separator => {
                self.print_context("", 0, "", true)?;
            }
        }
        Ok(())
    }

    /// heading 模式：打印文件名
    fn print_heading(&mut self, path: &str) -> Result<()> {
        self.stdout
            .set_color(ColorSpec::new().set_fg(Some(Color::Magenta)).set_bold(true))?;
        writeln!(self.stdout, "{}", path)?;
        self.stdout.reset()?;
        self.last_heading = Some(path.to_string());
        Ok(())
    }

    /// 对外公开的文件名输出（`-l` / `--name` / `--files` / `--files-without-match`）
    pub fn print_filename_pub(&mut self, path: &str) -> Result<()> {
        self.stdout
            .set_color(ColorSpec::new().set_fg(Some(Color::Magenta)).set_bold(true))?;
        if self.null_separator {
            write!(self.stdout, "{}\0", path)?;
        } else {
            writeln!(self.stdout, "{}", path)?;
        }
        self.stdout.reset()?;
        Ok(())
    }

    fn print_match(
        &mut self,
        path: &str,
        line_no: usize,
        line: &str,
        matches: &[(usize, usize)],
    ) -> Result<()> {
        if self.show_filename {
            self.stdout
                .set_color(ColorSpec::new().set_fg(Some(Color::Magenta)).set_bold(true))?;
            write!(self.stdout, "{}", path)?;
            self.stdout
                .set_color(ColorSpec::new().set_fg(Some(Color::Cyan)))?;
            write!(self.stdout, ":")?;
            self.stdout.reset()?;
        }

        if self.show_line_number {
            self.stdout
                .set_color(ColorSpec::new().set_fg(Some(Color::Green)))?;
            write!(self.stdout, "{}", line_no)?;
            self.stdout
                .set_color(ColorSpec::new().set_fg(Some(Color::Cyan)))?;
            write!(self.stdout, ":")?;
            self.stdout.reset()?;
        }

        if self.show_column {
            // 取第一个匹配的列号（1-based）；没有匹配时列号为 1
            let col = matches.first().map(|(s, _)| s + 1).unwrap_or(1);
            self.stdout
                .set_color(ColorSpec::new().set_fg(Some(Color::Green)))?;
            write!(self.stdout, "{}", col)?;
            self.stdout
                .set_color(ColorSpec::new().set_fg(Some(Color::Cyan)))?;
            write!(self.stdout, ":")?;
            self.stdout.reset()?;
        }

        self.write_highlighted(line, matches)?;
        self.stdout.reset()?;
        Ok(())
    }

    fn print_match_only(&mut self, path: &str, line_no: usize, text: &str) -> Result<()> {
        if self.show_filename {
            self.stdout
                .set_color(ColorSpec::new().set_fg(Some(Color::Magenta)).set_bold(true))?;
            write!(self.stdout, "{}", path)?;
            self.stdout
                .set_color(ColorSpec::new().set_fg(Some(Color::Cyan)))?;
            write!(self.stdout, ":")?;
            self.stdout.reset()?;
        }
        if self.show_line_number {
            self.stdout
                .set_color(ColorSpec::new().set_fg(Some(Color::Green)))?;
            write!(self.stdout, "{}", line_no)?;
            self.stdout
                .set_color(ColorSpec::new().set_fg(Some(Color::Cyan)))?;
            write!(self.stdout, ":")?;
            self.stdout.reset()?;
        }
        self.stdout
            .set_color(ColorSpec::new().set_fg(Some(Color::Red)).set_bold(true))?;
        writeln!(self.stdout, "{}", text)?;
        self.stdout.reset()?;
        Ok(())
    }

    fn print_context(
        &mut self,
        path: &str,
        line_no: usize,
        line: &str,
        is_separator: bool,
    ) -> Result<()> {
        if is_separator {
            self.stdout
                .set_color(ColorSpec::new().set_fg(Some(Color::Cyan)))?;
            writeln!(self.stdout, "--")?;
            self.stdout.reset()?;
            return Ok(());
        }

        if self.show_filename {
            self.stdout
                .set_color(ColorSpec::new().set_fg(Some(Color::Magenta)).set_bold(true))?;
            write!(self.stdout, "{}", path)?;
            self.stdout
                .set_color(ColorSpec::new().set_fg(Some(Color::Cyan)))?;
            write!(self.stdout, "-")?;
            self.stdout.reset()?;
        }
        if self.show_line_number {
            self.stdout
                .set_color(ColorSpec::new().set_fg(Some(Color::Green)))?;
            write!(self.stdout, "{}", line_no)?;
            self.stdout
                .set_color(ColorSpec::new().set_fg(Some(Color::Cyan)))?;
            write!(self.stdout, "-")?;
            self.stdout.reset()?;
        }
        writeln!(self.stdout, "{}", line)?;
        Ok(())
    }

    fn print_count(&mut self, path: &str, count: usize) -> Result<()> {
        self.stdout
            .set_color(ColorSpec::new().set_fg(Some(Color::Magenta)).set_bold(true))?;
        write!(self.stdout, "{}", path)?;
        self.stdout
            .set_color(ColorSpec::new().set_fg(Some(Color::Cyan)))?;
        write!(self.stdout, ":")?;
        self.stdout.reset()?;
        writeln!(self.stdout, "{}", count)?;
        Ok(())
    }

    fn write_highlighted(&mut self, line: &str, matches: &[(usize, usize)]) -> Result<()> {
        if matches.is_empty() {
            writeln!(self.stdout, "{}", line)?;
            return Ok(());
        }

        let mut last = 0usize;

        for &(start, end) in matches {
            if start > last {
                write!(self.stdout, "{}", &line[last..start])?;
            }
            self.stdout
                .set_color(ColorSpec::new().set_fg(Some(Color::Red)).set_bold(true))?;
            write!(self.stdout, "{}", &line[start..end])?;
            self.stdout.reset()?;
            last = end;
        }

        if last < line.len() {
            write!(self.stdout, "{}", &line[last..])?;
        }

        writeln!(self.stdout)?;
        Ok(())
    }
}

/// JSON Lines 输出
pub struct JsonPrinter;

impl JsonPrinter {
    pub fn new() -> Self {
        Self
    }

    pub fn print_file_result(
        &mut self,
        result: &FileResult,
        files_with_matches: bool,
        count: bool,
        count_matches: bool,
    ) -> Result<()> {
        let stdout = std::io::stdout();
        let mut out = stdout.lock();

        writeln!(
            out,
            r#"{{"type":"begin","path":{}}}"#,
            json_str(&result.path)
        )?;

        if files_with_matches {
            writeln!(
                out,
                r#"{{"type":"file","path":{}}}"#,
                json_str(&result.path)
            )?;
        } else if count {
            let n = if count_matches {
                result.occurrence_count
            } else {
                result.match_count
            };
            writeln!(
                out,
                r#"{{"type":"count","path":{},"count":{}}}"#,
                json_str(&result.path),
                n
            )?;
        } else {
            for line in &result.output {
                match line {
                    OutputLine::Match {
                        line_no,
                        line,
                        matches,
                    } => {
                        let subs: Vec<String> = matches
                            .iter()
                            .map(|(s, e)| format!(r#"{{"start":{},"end":{}}}"#, s, e))
                            .collect();
                        writeln!(
                            out,
                            r#"{{"type":"match","path":{},"line_number":{},"line":{},"submatches":[{}]}}"#,
                            json_str(&result.path),
                            line_no,
                            json_str(line),
                            subs.join(",")
                        )?;
                    }
                    OutputLine::MatchOnly { line_no, text } => {
                        writeln!(
                            out,
                            r#"{{"type":"match","path":{},"line_number":{},"match":{}}}"#,
                            json_str(&result.path),
                            line_no,
                            json_str(text)
                        )?;
                    }
                    OutputLine::Context { line_no, line } => {
                        writeln!(
                            out,
                            r#"{{"type":"context","path":{},"line_number":{},"line":{}}}"#,
                            json_str(&result.path),
                            line_no,
                            json_str(line)
                        )?;
                    }
                    OutputLine::Separator => {
                        writeln!(out, r#"{{"type":"separator"}}"#)?;
                    }
                }
            }
        }

        writeln!(out, r#"{{"type":"end","path":{}}}"#, json_str(&result.path))?;

        Ok(())
    }

    pub fn print_file_event(&mut self, path: &str) -> Result<()> {
        let stdout = std::io::stdout();
        let mut out = stdout.lock();
        writeln!(out, r#"{{"type":"file","path":{}}}"#, json_str(path))?;
        Ok(())
    }

    pub fn print_summary(
        &mut self,
        files_scanned: usize,
        files_matched: usize,
        matches: usize,
        elapsed_nanos: u128,
    ) -> Result<()> {
        let stdout = std::io::stdout();
        let mut out = stdout.lock();
        writeln!(
            out,
            r#"{{"type":"summary","files_scanned":{},"files_matched":{},"matches":{},"elapsed_nanos":{}}}"#,
            files_scanned, files_matched, matches, elapsed_nanos
        )?;
        Ok(())
    }
}

impl Default for JsonPrinter {
    fn default() -> Self {
        Self::new()
    }
}

fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_str_basic() {
        assert_eq!(json_str("hello"), r#""hello""#);
    }

    #[test]
    fn json_str_escapes_quotes_and_backslash() {
        assert_eq!(json_str(r#"a"b"#), r#""a\"b""#);
        assert_eq!(json_str(r"a\b"), r#""a\\b""#);
    }

    #[test]
    fn json_str_escapes_control_chars() {
        assert_eq!(json_str("a\nb"), r#""a\nb""#);
        assert_eq!(json_str("a\tb"), r#""a\tb""#);
        assert_eq!(json_str("a\x01b"), r#""a\u0001b""#);
    }

    #[test]
    fn json_str_keeps_unicode() {
        assert_eq!(json_str("你好"), r#""你好""#);
    }
}
