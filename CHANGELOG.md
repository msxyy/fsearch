# Changelog

所有重要变更都记录在这个文件里。

格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。

## [Unreleased]

## [0.6.1] - 2026-10-12

### Fixed

- 配置文件加载：搜索路径里没有 `.fsearch.toml` 时，
  回退到当前工作目录里的 `.fsearch.toml`

## [0.6.0] - 2026-10-12

### Added

- 配置文件支持：在搜索目录放 `.fsearch.toml`，为该目录及其子目录指定默认选项
  - 支持布尔开关、数值、字符串、列表四类字段
  - 命令行参数始终优先于配置文件
  - 未知字段会报错
- 索引增量更新：`--use-index` 时只重建 size 或 mtime 变化的文件
  - 索引格式从 `FSIDX001` 升到 `FSIDX002`
  - 旧格式索引会被自动识别并走全量重建

### Changed

- CLI 参数全面改为 `Option<T>`，便于和配置文件合并
- 新增 `config` 模块

## [0.5.0] - 2026-10-12

### Added

- 拆出 `lib.rs`，`fsearch` 现在同时是一个库
- 新增 `pattern` 模块
- 字面量预筛（`Prefilter`）：用 `aho-corasick` 快速跳过不可能命中的行
- 索引自动更新：`--use-index` 时检查 size 和 mtime

### Changed

- `SearchOptions` 加 `prefilter` 字段
- `JsonPrinter` 实现 `Default`
- 项目结构改为 `lib + bin`

## [0.4.0] - 2026-10-12

### Added

- `--build-index <DIR>` 为目录建立内容索引
- `--use-index` 使用已有索引加速搜索
- 索引使用 3-gram 倒排表

## [0.3.0] - 2026-10-12

### Added

- `-e PATTERN` / `--regexp PATTERN` 多模式
- `--files-without-match` 列出不含匹配的文件
- `--count-matches` 统计匹配次数
- `--max-depth N` 限制递归深度
- `--max-filesize SIZE` 跳过大于指定大小的文件
- `-q` / `--quiet` 静默模式
- `--generate-completions <SHELL>` 生成 shell 补全脚本
- `--generate-man` 生成 man 手册页

### Fixed

- 多个 `-e` 时，位置参数被误当作搜索模式而非路径

## [0.2.0] - 2026-10-12

### Added

- `--name` 按文件名搜索
- `--files` 只列出会被搜索的文件
- `-m N` / `--max-count N`
- `--stats`
- `--no-ignore`
- `-u` / `-uu` / `-uuu` 逐级放宽忽略规则
- `--encoding` 指定文件编码
- `--type-not` 排除指定文件类型
- `-r TEXT` / `--replace TEXT`
- `--sort`
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

[Unreleased]: https://github.com/msxyy/fsearch/compare/v0.6.1...HEAD
[0.6.1]: https://github.com/msxyy/fsearch/compare/v0.6.0...v0.6.1
[0.6.0]: https://github.com/msxyy/fsearch/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/msxyy/fsearch/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/msxyy/fsearch/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/msxyy/fsearch/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/msxyy/fsearch/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/msxyy/fsearch/releases/tag/v0.1.0