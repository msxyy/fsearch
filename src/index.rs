use std::collections::HashMap;
use std::fs;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::Result;
use ignore::WalkBuilder;

const MAGIC: &[u8; 8] = b"FSIDX002";

#[derive(Clone)]
struct FileEntry {
    path: String,
    size: u64,
    mtime: u64,
    /// 该文件内容里出现的所有 3-gram（可能有重复）
    trigrams: Vec<[u8; 3]>,
}

pub struct Index {
    files: Vec<FileEntry>,
    trigrams: HashMap<[u8; 3], Vec<u32>>,
}

/// 从文件内容提取所有 3-gram（转小写后按字节切）
fn extract_trigrams(content: &[u8]) -> Vec<[u8; 3]> {
    if content.len() < 3 {
        return Vec::new();
    }
    let lower: Vec<u8> = content.iter().map(|b| b.to_ascii_lowercase()).collect();
    let mut tris = Vec::with_capacity(lower.len() - 2);
    for i in 0..=lower.len() - 3 {
        tris.push([lower[i], lower[i + 1], lower[i + 2]]);
    }
    tris
}

/// 从全局倒排表重建：把每个文件的 trigrams 展开进去
fn rebuild_inverted(files: &[FileEntry]) -> HashMap<[u8; 3], Vec<u32>> {
    let mut trigrams: HashMap<[u8; 3], Vec<u32>> = HashMap::new();
    for (id, f) in files.iter().enumerate() {
        for tri in &f.trigrams {
            trigrams.entry(*tri).or_default().push(id as u32);
        }
    }
    for ids in trigrams.values_mut() {
        ids.sort_unstable();
        ids.dedup();
    }
    trigrams
}

impl Index {
    /// 从零构建索引（读取所有文件）
    pub fn build(root: &Path) -> Result<Self> {
        let mut files = Vec::new();

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

            let trigrams = extract_trigrams(&content);
            files.push(FileEntry {
                path: path.display().to_string(),
                size,
                mtime,
                trigrams,
            });
        }

        let trigrams = rebuild_inverted(&files);
        Ok(Index { files, trigrams })
    }

    /// 增量更新：复用旧索引里未变化文件的 trigram，只重新读变化的文件
    ///
    /// `old` 是之前的索引；为 `None` 时等价于全量构建。
    pub fn update(root: &Path, old: Option<&Index>) -> Result<Self> {
        // 旧索引按路径索引
        let old_by_path: HashMap<&str, &FileEntry> = match old {
            Some(idx) => idx.files.iter().map(|f| (f.path.as_str(), f)).collect(),
            None => HashMap::new(),
        };

        let mut files: Vec<FileEntry> = Vec::new();
        let mut reused = 0usize;
        let mut reindexed = 0usize;

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

            let path_str = path.display().to_string();

            // 未变化 → 复用旧条目（含 trigrams）
            if let Some(old_entry) = old_by_path.get(path_str.as_str()) {
                if old_entry.size == size && old_entry.mtime == mtime {
                    files.push((*old_entry).clone());
                    reused += 1;
                    continue;
                }
            }

            // 新文件或已变化 → 重新读取内容
            let content = match fs::read(path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            if content.contains(&0) {
                continue;
            }
            let trigrams = extract_trigrams(&content);
            files.push(FileEntry {
                path: path_str,
                size,
                mtime,
                trigrams,
            });
            reindexed += 1;
        }

        eprintln!(
            "fsearch: 索引更新完成（复用 {} 个，重建 {} 个）",
            reused, reindexed
        );

        let trigrams = rebuild_inverted(&files);
        Ok(Index { files, trigrams })
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let file = fs::File::create(path)?;
        let mut w = BufWriter::new(file);

        w.write_all(MAGIC)?;
        w.write_all(&(self.files.len() as u32).to_le_bytes())?;
        w.write_all(&(self.trigrams.len() as u32).to_le_bytes())?;

        // 文件区：路径、大小、mtime、该文件的 trigram 列表
        for f in &self.files {
            let pb = f.path.as_bytes();
            w.write_all(&(pb.len() as u32).to_le_bytes())?;
            w.write_all(pb)?;
            w.write_all(&f.size.to_le_bytes())?;
            w.write_all(&f.mtime.to_le_bytes())?;
            w.write_all(&(f.trigrams.len() as u32).to_le_bytes())?;
            for tri in &f.trigrams {
                w.write_all(tri)?;
            }
        }

        // 倒排表区
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
            anyhow::bail!("索引文件格式不正确（magic 不匹配，可能是旧版本索引）");
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

            r.read_exact(&mut buf4)?;
            let tri_count = u32::from_le_bytes(buf4) as usize;
            let mut trigrams = Vec::with_capacity(tri_count);
            for _ in 0..tri_count {
                let mut tri = [0u8; 3];
                r.read_exact(&mut tri)?;
                trigrams.push(tri);
            }

            files.push(FileEntry {
                path,
                size,
                mtime,
                trigrams,
            });
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

    /// 索引是否已过期（任何文件 size/mtime 变了，或有新文件/删除文件）
    pub fn is_stale(&self, root: &Path) -> bool {
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

/// 加载索引并增量更新
///
/// 返回 `(index, updated)`：
/// - `updated = true` 表示索引被创建或更新过
/// - `updated = false` 表示磁盘上的索引仍然新鲜，直接返回
///
/// 若磁盘上没有索引，或索引是老格式（FSIDX001），会走全量构建。
pub fn load_or_update(root: &Path) -> Result<(Index, bool)> {
    let idx_path = index_path(root);

    // 尝试加载旧索引
    let old = if idx_path.exists() {
        match Index::load(&idx_path) {
            Ok(idx) => Some(idx),
            Err(_) => {
                // 格式不对或损坏，删掉走全量
                let _ = fs::remove_file(&idx_path);
                None
            }
        }
    } else {
        None
    };

    // 有旧索引且新鲜 → 直接用
    if let Some(ref idx) = old {
        if !idx.is_stale(root) {
            return Ok((old.unwrap(), false));
        }
    }

    // 增量更新（old 为 None 时全量）
    let new_idx = Index::update(root, old.as_ref())?;
    new_idx.save(&idx_path)?;
    Ok((new_idx, true))
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
    fn load_or_update_creates_when_missing() {
        let dir = TempDir::new("lorb_new");
        dir.file("a.txt", "hello\n");

        let (idx, updated) = load_or_update(&dir.0).unwrap();
        assert!(updated);
        assert_eq!(idx.file_count(), 1);
        assert!(index_path(&dir.0).exists());
    }

    #[test]
    fn load_or_update_reuses_when_fresh() {
        let dir = TempDir::new("lorb_fresh");
        dir.file("a.txt", "hello\n");

        let (_, _) = load_or_update(&dir.0).unwrap();
        let (_, updated) = load_or_update(&dir.0).unwrap();
        assert!(!updated);
    }

    #[test]
    fn load_or_update_rebuilds_when_stale() {
        let dir = TempDir::new("lorb_stale");
        dir.file("a.txt", "hello\n");

        let (_, _) = load_or_update(&dir.0).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        dir.file("b.txt", "new\n");

        let (idx, updated) = load_or_update(&dir.0).unwrap();
        assert!(updated);
        assert_eq!(idx.file_count(), 2);
    }

    // ---------- 增量更新专项 ----------

    #[test]
    fn update_reuses_unchanged_trigrams() {
        let dir = TempDir::new("up_reuse");
        dir.file("a.txt", "hello world\n");
        dir.file("b.txt", "foo bar\n");

        let idx1 = Index::build(&dir.0).unwrap();
        // 没变过，update 后内容应完全一致
        let idx2 = Index::update(&dir.0, Some(&idx1)).unwrap();
        assert_eq!(idx2.file_count(), 2);
        assert_eq!(idx2.candidates("hello").unwrap().len(), 1);
        assert_eq!(idx2.candidates("foo").unwrap().len(), 1);
    }

    #[test]
    fn update_picks_up_new_file() {
        let dir = TempDir::new("up_new");
        dir.file("a.txt", "hello world\n");

        let idx1 = Index::build(&dir.0).unwrap();
        dir.file("b.txt", "needle here\n");

        let idx2 = Index::update(&dir.0, Some(&idx1)).unwrap();
        assert_eq!(idx2.file_count(), 2);
        let cands = idx2.candidates("needle").unwrap();
        assert_eq!(cands.len(), 1);
        assert!(cands[0].contains("b.txt"));
    }

    #[test]
    fn update_removes_deleted_file() {
        let dir = TempDir::new("up_del");
        let p = dir.file("a.txt", "hello\n");
        dir.file("b.txt", "world\n");

        let idx1 = Index::build(&dir.0).unwrap();
        std_fs::remove_file(&p).unwrap();

        let idx2 = Index::update(&dir.0, Some(&idx1)).unwrap();
        assert_eq!(idx2.file_count(), 1);
        let cands = idx2.candidates("hello").unwrap();
        assert!(cands.is_empty());
    }

    #[test]
    fn update_reindexes_changed_file() {
        let dir = TempDir::new("up_change");
        dir.file("a.txt", "hello\n");

        let idx1 = Index::build(&dir.0).unwrap();
        assert_eq!(idx1.candidates("hello").unwrap().len(), 1);
        assert!(idx1.candidates("needle").unwrap().is_empty());

        std::thread::sleep(std::time::Duration::from_millis(1100));
        std_fs::write(dir.0.join("a.txt"), "needle here\n").unwrap();

        let idx2 = Index::update(&dir.0, Some(&idx1)).unwrap();
        assert!(idx2.candidates("hello").unwrap().is_empty());
        assert_eq!(idx2.candidates("needle").unwrap().len(), 1);
    }

    #[test]
    fn update_with_none_is_full_build() {
        let dir = TempDir::new("up_none");
        dir.file("a.txt", "hello\n");

        let idx = Index::update(&dir.0, None).unwrap();
        assert_eq!(idx.file_count(), 1);
    }

    #[test]
    fn old_format_magic_is_rejected() {
        let dir = TempDir::new("old_magic");
        // 写一个 FSIDX001 的假文件
        let p = dir.0.join(".fsearch-index");
        std_fs::write(&p, b"FSIDX001this is old format").unwrap();

        let result = Index::load(&p);
        assert!(result.is_err());

        // load_or_update 应该丢弃旧索引并重建
        let (idx, updated) = load_or_update(&dir.0).unwrap();
        assert!(updated);
        assert_eq!(idx.file_count(), 0);
    }
}
