# Changelog

所有重要变更都记录在这个文件里。

格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。

## [Unreleased]

## [0.5.0] - 2026-10-12

### Added

- 拆出 `lib.rs`，`fsearch` 现在同时是一个库，可被其他程序依赖
- 新增 `pattern` 模块：`build_pattern`、`build_combined_pattern`、`literal_patterns`
- 字面量预筛（`Prefilter`）：用 `aho-corasick` 快速跳过不可能命中的行，
  只在可能命中的行上跑正则。对 `-F` 或纯字面量模式自动启用
- 索引自动更新：`--use-index` 时会检查文件 size 和 mtime，
  索引过期或不存在时自动重建，不需要手动再跑 `--build-index`

### Changed

- `SearchOptions` 加 `prefilter` 字段；用 `#[derive(Default)]` 提供默认值
- `SearchOptions` 不再是 `pub struct` 手动实现 `Default`，改用 derive
- `JsonPrinter` 实现 `Default`
- 项目结构改为 `lib + bin`，`src/main.rs` 只负责 CLI 和遍历

### Fixed

- 集成测试 `use_index_without_index_errors` 改为验证自动建立索引的新行为

## [0.4.0] - 2026-10-12

### Added

- `--build-index <DIR>` 为目录建立内容索引，保存到 `<DIR>/.fsearch-index`
- `--use-index` 使用已有索引加速搜索；仅支持长度 ≥ 3 的字面量 pattern，
  含正则元字符时自动回退到全量扫描
- 索引使用 3-gram 倒排表，只对候选文件做精确搜索

## [0.3.0] - 2026-10-12

### Added

- `-e PATTERN` / `--regexp PATTERN` 支持多个搜索模式，任一匹配即命中
- `--files-without-match` 列出不含匹配的文件（`-l` 的反向）
- `--count-matches` 统计匹配次数（每次出现都算一次），而非匹配行数
- `--max-depth N` 限制递归深度
- `--max-filesize SIZE` 跳过大于指定大小的文件（支持 `10M`、`1G`、`500K`）
- `-q` / `--quiet` 静默模式，只返回退出码
- `--generate-completions <SHELL>` 生成 shell 补全脚本
- `--generate-man` 生成 man 手册页

### Fixed

- 多个 `-e` 时，位置参数被误当作搜索模式而非路径

## [0.2.0] - 2026-10-12

### Added

- `--name` 按文件名搜索
- `--files` 只列出会被搜索的文件
- `-m N` / `--max-count N` 每文件最多输出 N 条匹配
- `--stats` 输出扫描文件数、匹配数、耗时
- `--no-ignore` 不读取 `.gitignore`
- `-u` / `-uu` / `-uuu` 逐级放宽忽略规则
- `--encoding` 指定文件编码（gbk、big5、shift_jis 等）
- `--type-not` 排除指定文件类型
- `-r TEXT` / `--replace TEXT` 替换匹配部分
- `--sort` 并行搜索但按路径排序输出
- `--json` 补充 `begin` / `end` / `summary` 事件

### Fixed

- `--replace` 替换后字节偏移失效导致的越界 panic
- `.gitignore` 在非 git 仓库目录下不生效的问题
- `-m` 达到上限后 `-A` 上下文被截断的问题

## [0.1.0] - 2026-10-12

### Added

- 正则搜索，支持 `-i` 忽略大小写
- 递归目录遍历，自动读取 `.gitignore`
- 输出控制：`-n` 行号、`-l` 文件名、`-c` 计数
- 上下文：`-C` / `-A` / `-B`
- 模式控制：`-w` 全词、`-v` 反向、`-F` 固定字符串
- 输出模式：`-o` 只输出匹配部分
- 文件过滤：`-g` glob、`--type` 按类型
- 输出格式：`--json` JSON Lines
- 并行遍历 + 流式逐行搜索 + 环形缓冲
- 支持非 UTF-8，跳过二进制文件
- 退出码：0 找到 / 1 未找到 / 2 出错

[Unreleased]: https://github.com/msxyy/fsearch/compare/v0.5.0...HEAD
[0.5.0]: https://github.com/msxyy/fsearch/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/msxyy/fsearch/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/msxyy/fsearch/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/msxyy/fsearch/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/msxyy/fsearch/releases/tag/v0.1.0