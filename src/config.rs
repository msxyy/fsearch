//! 配置文件解析。
//!
//! 在搜索目录里放 `.fsearch.toml`，可以为该目录及其子目录指定默认选项。
//! 命令行参数始终优先于配置文件。
//!
//! 示例 `.fsearch.toml`：
//!
//! ```toml
//! ignore_case = true
//! line_number = true
//! type_filter = ["rust", "toml"]
//! context = 2
//! max_depth = 5
//! max_filesize = "1M"
//! ```

use std::path::Path;

use anyhow::Result;
use serde::Deserialize;

/// 配置文件的字段
///
/// 全部字段都是 `Option<T>`：`None` 表示配置文件没设这一项，
/// 此时使用 CLI 参数或内置默认。
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    // 布尔开关
    pub ignore_case: Option<bool>,
    pub line_number: Option<bool>,
    pub hidden: Option<bool>,
    pub follow: Option<bool>,
    pub word: Option<bool>,
    pub invert: Option<bool>,
    pub fixed: Option<bool>,
    pub only_matching: Option<bool>,
    pub quiet: Option<bool>,
    pub no_ignore: Option<bool>,
    pub sort: Option<bool>,
    pub stats: Option<bool>,
    pub json: Option<bool>,

    // 数值
    pub context: Option<usize>,
    pub before: Option<usize>,
    pub after: Option<usize>,
    pub max_count: Option<usize>,
    pub max_depth: Option<usize>,
    pub unrestricted: Option<u8>,

    // 字符串
    pub color: Option<String>,
    pub encoding: Option<String>,
    pub glob: Option<String>,
    pub max_filesize: Option<String>,
    pub replace: Option<String>,

    // 列表
    pub type_filter: Option<Vec<String>>,
    pub type_not_filter: Option<Vec<String>>,
}

/// 配置文件的文件名
pub const CONFIG_FILENAME: &str = ".fsearch.toml";

impl Config {
    /// 从 `root/.fsearch.toml` 加载。
    ///
    /// 文件不存在时返回空配置（全 `None`）。文件存在但解析失败时报错。
    pub fn load(root: &Path) -> Result<Self> {
        let path = root.join(CONFIG_FILENAME);
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|e| anyhow::anyhow!("无法读取 {}: {}", path.display(), e))?;
        let cfg: Self = toml::from_str(&text)
            .map_err(|e| anyhow::anyhow!("无法解析 {}: {}", path.display(), e))?;
        Ok(cfg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let mut p = std::env::temp_dir();
            p.push(format!(
                "fsearch_cfg_{}_{}_{}",
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

        fn write(&self, name: &str, content: &str) {
            fs::write(self.0.join(name), content).unwrap();
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn load_missing_returns_default() {
        let dir = TempDir::new("missing");
        let cfg = Config::load(&dir.0).unwrap();
        assert!(cfg.ignore_case.is_none());
        assert!(cfg.type_filter.is_none());
    }

    #[test]
    fn load_simple_flags() {
        let dir = TempDir::new("flags");
        dir.write(
            CONFIG_FILENAME,
            r#"
ignore_case = true
line_number = true
context = 2
"#,
        );
        let cfg = Config::load(&dir.0).unwrap();
        assert_eq!(cfg.ignore_case, Some(true));
        assert_eq!(cfg.line_number, Some(true));
        assert_eq!(cfg.context, Some(2));
        assert!(cfg.word.is_none());
    }

    #[test]
    fn load_lists() {
        let dir = TempDir::new("lists");
        dir.write(
            CONFIG_FILENAME,
            r#"
type_filter = ["rust", "toml"]
type_not_filter = ["markdown"]
"#,
        );
        let cfg = Config::load(&dir.0).unwrap();
        assert_eq!(
            cfg.type_filter,
            Some(vec!["rust".to_string(), "toml".to_string()])
        );
        assert_eq!(cfg.type_not_filter, Some(vec!["markdown".to_string()]));
    }

    #[test]
    fn load_unknown_field_errors() {
        let dir = TempDir::new("unknown");
        dir.write(CONFIG_FILENAME, "not_a_real_field = 1\n");
        assert!(Config::load(&dir.0).is_err());
    }

    #[test]
    fn load_invalid_toml_errors() {
        let dir = TempDir::new("invalid");
        dir.write(CONFIG_FILENAME, "this is not toml {{{");
        assert!(Config::load(&dir.0).is_err());
    }

    #[test]
    fn load_string_values() {
        let dir = TempDir::new("strings");
        dir.write(
            CONFIG_FILENAME,
            r#"
color = "always"
max_filesize = "10M"
glob = "*.rs"
"#,
        );
        let cfg = Config::load(&dir.0).unwrap();
        assert_eq!(cfg.color, Some("always".to_string()));
        assert_eq!(cfg.max_filesize, Some("10M".to_string()));
        assert_eq!(cfg.glob, Some("*.rs".to_string()));
    }
}
