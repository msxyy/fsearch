//! 正则模式构建。
//!
//! 这里处理 `-F`（固定字符串）、`-w`（全词匹配）、多模式合并，
//! 以及判断 pattern 是否可以当字面量用来预筛。

/// 根据 -F / -w 组装单个模式的最终正则字符串
///
/// - `fixed`：转义所有正则元字符
/// - `word`：用 `\b(?:...)\b` 包裹，实现全词匹配
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

/// 把多个模式合并成一个正则字符串
///
/// - 空列表返回空串
/// - 单模式等价于 `build_pattern`
/// - 多模式用 `|` 连接，非 `-w` 时用 `(?:...)` 包裹
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

/// 如果所有模式都能当成纯字面量，返回这些字面量；否则返回 None
///
/// 用于构建字面量预筛器（`Prefilter`）。返回 None 的情况：
///
/// - `word == true`（因为 `\b` 边界不能直接用字面量）
/// - 任一模式含正则元字符，且没开 `fixed`
///
/// 注意：返回的字面量仅用于预筛（快速排除不可能命中的行），
/// 不保证和最终正则有 100% 一致的语义，所以命中后仍需 regex 复核。
pub fn literal_patterns(patterns: &[String], fixed: bool, word: bool) -> Option<Vec<String>> {
    if word {
        return None;
    }
    let mut out = Vec::with_capacity(patterns.len());
    for p in patterns {
        if fixed || is_literal(p) {
            out.push(p.clone());
        } else {
            return None;
        }
    }
    Some(out)
}

/// 检查一个 pattern 是否不含正则元字符
fn is_literal(p: &str) -> bool {
    !p.chars().any(|c| {
        matches!(
            c,
            '\\' | '.' | '*' | '+' | '?' | '[' | ']' | '{' | '}' | '(' | ')' | '|' | '^' | '$'
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;

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

    #[test]
    fn literal_single_plain() {
        let pats = vec!["hello".to_string()];
        assert_eq!(
            literal_patterns(&pats, false, false),
            Some(vec!["hello".to_string()])
        );
    }

    #[test]
    fn literal_rejects_regex_meta() {
        let pats = vec!["a.c".to_string()];
        assert!(literal_patterns(&pats, false, false).is_none());
    }

    #[test]
    fn literal_accepts_with_fixed() {
        let pats = vec!["a.c".to_string()];
        assert_eq!(
            literal_patterns(&pats, true, false),
            Some(vec!["a.c".to_string()])
        );
    }

    #[test]
    fn literal_rejects_with_word() {
        let pats = vec!["hello".to_string()];
        assert!(literal_patterns(&pats, false, true).is_none());
    }

    #[test]
    fn literal_multiple_patterns() {
        let pats = vec!["foo".to_string(), "bar".to_string()];
        assert_eq!(
            literal_patterns(&pats, false, false),
            Some(vec!["foo".to_string(), "bar".to_string()])
        );
    }
}
