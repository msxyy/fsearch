use std::collections::HashMap;
use std::fs;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::Result;
use ignore::WalkBuilder;

const MAGIC: &[u8; 8] = b"FSIDX001";

struct FileEntry {
    path: String,
    size: u64,
    mtime: u64,
}

pub struct Index {
    files: Vec<FileEntry>,
    trigrams: HashMap<[u8; 3], Vec<u32>>,
}

impl Index {
    /// 为目录建立新索引（重新扫描全部文件）
    pub fn build(root: &Path) -> Result<Self> {
        let mut files = Vec::new();
        let mut trigrams: HashMap<[u8; 3], Vec<u32>> = HashMap::new();

        let walker = WalkBuilder::new(root)
            .hidden(true)
            .require_git(false)
            .build();

        for entry in walker.flatten() {
            if !entry.file_type().is_some_and(|ft| ft.is_file()) {
                continue;
            }
            let path = entry.path();
            if path.file_name().and_then(|s| s.to_str()) == Some(".fsearch-index") {
                continue;
            }
            let metadata = match fs::metadata(path) {
                Ok(m) => m,
                Err(_) => continue,
            };
            let size = metadata.len();
            let mtime = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);

            let content = match fs::read(path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            if content.contains(&0) {
                continue;
            }

            let file_id = files.len() as u32;
            let path_str = path.display().to_string();

            let lower: Vec<u8> = content.iter().map(|b| b.to_ascii_lowercase()).collect();
            if lower.len() >= 3 {
                for i in 0..=lower.len() - 3 {
                    let tri = [lower[i], lower[i + 1], lower[i + 2]];
                    trigrams.entry(tri).or_default().push(file_id);
                }
            }

            files.push(FileEntry {
                path: path_str,
                size,
                mtime,
            });
        }

        for ids in trigrams.values_mut() {
            ids.sort_unstable();
            ids.dedup();
        }

        Ok(Index { files, trigrams })
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let file = fs::File::create(path)?;
        let mut w = BufWriter::new(file);

        w.write_all(MAGIC)?;
        w.write_all(&(self.files.len() as u32).to_le_bytes())?;
        w.write_all(&(self.trigrams.len() as u32).to_le_bytes())?;

        for f in &self.files {
            let pb = f.path.as_bytes();
            w.write_all(&(pb.len() as u32).to_le_bytes())?;
            w.write_all(pb)?;
            w.write_all(&f.size.to_le_bytes())?;
            w.write_all(&f.mtime.to_le_bytes())?;
        }

        for (tri, ids) in &self.trigrams {
            w.write_all(tri)?;
            w.write_all(&(ids.len() as u32).to_le_bytes())?;
            for id in ids {
                w.write_all(&id.to_le_bytes())?;
            }
        }

        w.flush()?;
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self> {
        let file = fs::File::open(path)?;
        let mut r = BufReader::new(file);

        let mut magic = [0u8; 8];
        r.read_exact(&mut magic)?;
        if &magic != MAGIC {
            anyhow::bail!("索引文件格式不正确（magic 不匹配）");
        }

        let mut buf4 = [0u8; 4];
        r.read_exact(&mut buf4)?;
        let file_count = u32::from_le_bytes(buf4) as usize;
        r.read_exact(&mut buf4)?;
        let trigram_count = u32::from_le_bytes(buf4) as usize;

        let mut files = Vec::with_capacity(file_count);
        for _ in 0..file_count {
            r.read_exact(&mut buf4)?;
            let path_len = u32::from_le_bytes(buf4) as usize;
            let mut path_bytes = vec![0u8; path_len];
            r.read_exact(&mut path_bytes)?;
            let path = String::from_utf8_lossy(&path_bytes).into_owned();

            let mut buf8 = [0u8; 8];
            r.read_exact(&mut buf8)?;
            let size = u64::from_le_bytes(buf8);
            r.read_exact(&mut buf8)?;
            let mtime = u64::from_le_bytes(buf8);

            files.push(FileEntry { path, size, mtime });
        }

        let mut trigrams = HashMap::with_capacity(trigram_count);
        for _ in 0..trigram_count {
            let mut tri = [0u8; 3];
            r.read_exact(&mut tri)?;
            r.read_exact(&mut buf4)?;
            let id_count = u32::from_le_bytes(buf4) as usize;
            let mut ids = Vec::with_capacity(id_count);
            for _ in 0..id_count {
                r.read_exact(&mut buf4)?;
                ids.push(u32::from_le_bytes(buf4));
            }
            trigrams.insert(tri, ids);
        }

        Ok(Index { files, trigrams })
    }

    /// 检查索引相对磁盘上的当前状态是否已过期
    ///
    /// 满足下列任一条件即视为过期：
    /// - 有文件不存在了
    /// - 有文件的 size 或 mtime 变了
    /// - 目录下出现了索引里没有的新文件
    pub fn is_stale(&self, root: &Path) -> bool {
        // 先按索引里的清单检查
        for f in &self.files {
            let p = Path::new(&f.path);
            let metadata = match fs::metadata(p) {
                Ok(m) => m,
                Err(_) => return true,
            };
            if metadata.len() != f.size {
                return true;
            }
            let mtime = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            if mtime != f.mtime {
                return true;
            }
        }

        // 再查有没有索引里没记录的新文件
        let walker = WalkBuilder::new(root)
            .hidden(true)
            .require_git(false)
            .build();
        for entry in walker.flatten() {
            if !entry.file_type().is_some_and(|ft| ft.is_file()) {
                continue;
            }
            let path = entry.path();
            if path.file_name().and_then(|s| s.to_str()) == Some(".fsearch-index") {
                continue;
            }
            let path_str = path.display().to_string();
            if !self.files.iter().any(|f| f.path == path_str) {
                return true;
            }
        }

        false
    }

    /// 如果 pattern 全是字面量，返回候选文件路径；否则返回 None
    pub fn candidates(&self, pattern: &str) -> Option<Vec<String>> {
        if pattern.chars().any(|c| {
            matches!(
                c,
                '\\' | '.' | '*' | '+' | '?' | '[' | ']' | '{' | '}' | '(' | ')' | '|' | '^' | '$'
            )
        }) {
            return None;
        }

        let lower: Vec<u8> = pattern
            .as_bytes()
            .iter()
            .map(|b| b.to_ascii_lowercase())
            .collect();

        if lower.len() < 3 {
            return None;
        }

        let mut candidate_sets: Vec<&Vec<u32>> = Vec::new();
        for i in 0..=lower.len() - 3 {
            let tri = [lower[i], lower[i + 1], lower[i + 2]];
            match self.trigrams.get(&tri) {
                Some(ids) => candidate_sets.push(ids),
                None => return Some(Vec::new()),
            }
        }

        if candidate_sets.is_empty() {
            return Some(Vec::new());
        }

        candidate_sets.sort_by_key(|s| s.len());
        let mut result: Vec<u32> = candidate_sets[0].clone();
        for s in &candidate_sets[1..] {
            result.retain(|id| s.binary_search(id).is_ok());
            if result.is_empty() {
                return Some(Vec::new());
            }
        }

        Some(
            result
                .iter()
                .map(|&id| self.files[id as usize].path.clone())
                .collect(),
        )
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }
}

/// 索引文件的默认存储位置
pub fn index_path(root: &Path) -> PathBuf {
    root.join(".fsearch-index")
}

/// 建立索引并保存，返回索引的文件数
pub fn build_and_save(root: &Path) -> Result<usize> {
    let index = Index::build(root)?;
    let path = index_path(root);
    index.save(&path)?;
    Ok(index.file_count())
}

/// 加载索引；不存在或过期则重建
///
/// 返回 `(index, rebuilt)`：
/// - `rebuilt = true` 表示索引被创建或重建
/// - `rebuilt = false` 表示直接用了磁盘上的旧索引
pub fn load_or_rebuild(root: &Path) -> Result<(Index, bool)> {
    let idx_path = index_path(root);
    if !idx_path.exists() {
        let idx = Index::build(root)?;
        idx.save(&idx_path)?;
        return Ok((idx, true));
    }
    let idx = Index::load(&idx_path)?;
    if idx.is_stale(root) {
        let new_idx = Index::build(root)?;
        new_idx.save(&idx_path)?;
        Ok((new_idx, true))
    } else {
        Ok((idx, false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs as std_fs;

    struct TempDir(PathBuf);
    impl TempDir {
        fn new(tag: &str) -> Self {
            let mut p = std::env::temp_dir();
            p.push(format!(
                "fsearch_idx_{}_{}_{}",
                tag,
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std_fs::create_dir_all(&p).unwrap();
            TempDir(p)
        }
        fn file(&self, name: &str, content: &str) -> PathBuf {
            let p = self.0.join(name);
            std_fs::write(&p, content).unwrap();
            p
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std_fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn build_index_finds_files() {
        let dir = TempDir::new("build");
        dir.file("a.txt", "hello world\n");
        dir.file("b.txt", "foo bar\n");

        let index = Index::build(&dir.0).unwrap();
        assert_eq!(index.file_count(), 2);
    }

    #[test]
    fn candidates_exact_literal() {
        let dir = TempDir::new("cand");
        dir.file("a.txt", "hello world\n");
        dir.file("b.txt", "foo bar\n");
        dir.file("c.txt", "hello there\n");

        let index = Index::build(&dir.0).unwrap();
        let cands = index.candidates("hello").unwrap();
        assert_eq!(cands.len(), 2);
        assert!(cands.iter().any(|p| p.contains("a.txt")));
        assert!(cands.iter().any(|p| p.contains("c.txt")));
        assert!(!cands.iter().any(|p| p.contains("b.txt")));
    }

    #[test]
    fn candidates_returns_none_for_regex() {
        let dir = TempDir::new("regex");
        dir.file("a.txt", "hello\n");

        let index = Index::build(&dir.0).unwrap();
        assert!(index.candidates("hel.*o").is_none());
        assert!(index.candidates("^hello").is_none());
    }

    #[test]
    fn candidates_empty_for_no_match() {
        let dir = TempDir::new("nomatch");
        dir.file("a.txt", "hello\n");

        let index = Index::build(&dir.0).unwrap();
        let cands = index.candidates("xyz").unwrap();
        assert!(cands.is_empty());
    }

    #[test]
    fn candidates_short_pattern_returns_none() {
        let dir = TempDir::new("short");
        dir.file("a.txt", "hello\n");

        let index = Index::build(&dir.0).unwrap();
        assert!(index.candidates("hi").is_none());
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = TempDir::new("save");
        dir.file("a.txt", "hello world\n");
        dir.file("b.txt", "foo bar\n");

        let index = Index::build(&dir.0).unwrap();
        let idx_path = index_path(&dir.0);
        index.save(&idx_path).unwrap();

        let loaded = Index::load(&idx_path).unwrap();
        assert_eq!(loaded.file_count(), 2);

        let cands = loaded.candidates("hello").unwrap();
        assert_eq!(cands.len(), 1);
    }

    #[test]
    fn case_insensitive_candidates() {
        let dir = TempDir::new("case");
        dir.file("a.txt", "HELLO World\n");

        let index = Index::build(&dir.0).unwrap();
        let cands = index.candidates("hello").unwrap();
        assert_eq!(cands.len(), 1);
    }

    #[test]
    fn skips_binary_files() {
        let dir = TempDir::new("bin");
        std_fs::write(dir.0.join("a.bin"), b"hello\0world").unwrap();
        dir.file("b.txt", "hello\n");

        let index = Index::build(&dir.0).unwrap();
        assert_eq!(index.file_count(), 1);
    }

    #[test]
    fn stale_when_file_modified() {
        let dir = TempDir::new("stale_mod");
        dir.file("a.txt", "hello\n");
        let index = Index::build(&dir.0).unwrap();
        assert!(!index.is_stale(&dir.0));

        // 改内容，size 和 mtime 都会变
        std::thread::sleep(std::time::Duration::from_millis(1100));
        std_fs::write(dir.0.join("a.txt"), "hello world, longer\n").unwrap();
        assert!(index.is_stale(&dir.0));
    }

    #[test]
    fn stale_when_file_deleted() {
        let dir = TempDir::new("stale_del");
        let p = dir.file("a.txt", "hello\n");
        let index = Index::build(&dir.0).unwrap();
        std_fs::remove_file(&p).unwrap();
        assert!(index.is_stale(&dir.0));
    }

    #[test]
    fn stale_when_new_file_added() {
        let dir = TempDir::new("stale_add");
        dir.file("a.txt", "hello\n");
        let index = Index::build(&dir.0).unwrap();
        dir.file("b.txt", "new file\n");
        assert!(index.is_stale(&dir.0));
    }

    #[test]
    fn load_or_rebuild_creates_when_missing() {
        let dir = TempDir::new("lorb_new");
        dir.file("a.txt", "hello\n");

        let (idx, rebuilt) = load_or_rebuild(&dir.0).unwrap();
        assert!(rebuilt);
        assert_eq!(idx.file_count(), 1);
        assert!(index_path(&dir.0).exists());
    }

    #[test]
    fn load_or_rebuild_reuses_when_fresh() {
        let dir = TempDir::new("lorb_fresh");
        dir.file("a.txt", "hello\n");

        let (_, _) = load_or_rebuild(&dir.0).unwrap();
        let (_, rebuilt) = load_or_rebuild(&dir.0).unwrap();
        assert!(!rebuilt);
    }

    #[test]
    fn load_or_rebuild_rebuilds_when_stale() {
        let dir = TempDir::new("lorb_stale");
        dir.file("a.txt", "hello\n");

        let (_, _) = load_or_rebuild(&dir.0).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        dir.file("b.txt", "new\n");

        let (idx, rebuilt) = load_or_rebuild(&dir.0).unwrap();
        assert!(rebuilt);
        assert_eq!(idx.file_count(), 2);
    }
}
