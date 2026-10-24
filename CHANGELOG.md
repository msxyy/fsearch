# Changelog

所有重要变更都记录在这个文件里。

格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。

## [Unreleased]

### Added

- 短名 `fs`：`fsearch` 的第二个可执行入口，两个 bin 共用同一份 CLI 逻辑
  - 程序名取自 `argv[0]`，所以 `fs --help`、`fs --version`、
    `fs --generate-man`、`fs --generate-completions` 显示的都是 `fs`
  - 发布包（Windows zip / Unix tar.gz）同时包含 `fsearch` 和 `fs`

### Changed

- CLI 主流程从 `src/main.rs` 移到 `src/app.rs`（`lib.rs` 新增 `app` / `cli` 模块），
  `src/main.rs` 变成薄包装；`src/bin/fs.rs` 同样只有三行
- 库新增公共入口 `fsearch::app::run()`（别名 `fsearch::run_cli`）

## [0.7.3] - 2026-10-13

### Fixed

- `Cargo.toml` 版本号同步到 v0.7.2 的构建配置改动；
  v0.7.2 的 tag 没有改版本号，导致 `fsearch --version` 仍显示 0.7.1

## [0.7.2] - 2026-10-12

### Changed

- Windows 版改用静态 CRT 链接，不再依赖 `VCRUNTIME140.dll` / `MSVCP140.dll`
- 用户下载 Windows exe 后无需安装 VC++ Redistributable，解压即用

## [0.7.1] - 2026-10-12

### Fixed

- 字节级预筛启用时行号错误：预筛跳过的行没有计入行号，
  导致 `-n` 显示的是"匹配序号"而非真实行号

## [0.7.0] - 2026-10-12

### Added

- 索引持久化到用户缓存目录，不再污染项目目录
  - Windows：`%LOCALAPPDATA%\fsearch\cache\<hash>.fsearch-index`
  - macOS：`~/Library/Caches/fsearch/cache/<hash>.fsearch-index`
  - Linux：`$XDG_CACHE_HOME/fsearch/cache/<hash>.fsearch-index` 或 `~/.cache/fsearch/cache/<hash>.fsearch-index`
  - 缓存目录不可用时回退到项目目录的 `.fsearch-index`
  - 缓存文件名是目录规范化路径的哈希，同一目录多次调用自动复用
- `--no-config`：显式忽略 `.fsearch.toml` 配置文件
- `index::is_indexable(pattern)`：判断 pattern 是否可被索引加速
- `--column`：显示每个匹配的 1-based 列号
- `--vimgrep`：输出 `file:line:col:content`，同一行的多个匹配各占一行
- `--no-filename` / `--with-filename`：控制是否显示文件名
- `-0` / `--null`：路径之间用 NUL 字节分隔，方便管道传给 `xargs -0`
- `--heading` / `--no-heading`：按文件分组输出，文件名单独一行
- 字节级预筛：无上下文（`-A`/`-B` 都为 0）时，预筛直接在 `&[u8]` 上跑，
  不命中的行跳过 UTF-8 解码

### Changed

- `--use-index` 参数类型从 `bool` 改为 `Option<bool>`
- `--use-index` 会先判断 pattern 是否可被索引加速；不可以时不建索引，直接回退
- `Prefilter` 新增 `might_match_bytes(&[u8])`
- `LineSource::Stream` 增加 `prefilter` / `skip_prefilter` 字段
- `Printer` 新增 `with_filename` / `with_column` / `with_vimgrep` /
  `with_heading` / `with_null_separator` 构建方法
- `Config` 新增 `column` / `vimgrep` / `heading` / `no_heading` /
  `no_filename` / `with_filename` / `null_separator` 字段

### Fixed

- 缓存路径补上 `cache` 子目录，与文档一致
- heading 模式不再重复打印文件名

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

[Unreleased]: https://github.com/msxyy/fsearch/compare/v0.7.3...HEAD
[0.7.3]: https://github.com/msxyy/fsearch/compare/v0.7.2...v0.7.3
[0.7.2]: https://github.com/msxyy/fsearch/compare/v0.7.1...v0.7.2
[0.7.1]: https://github.com/msxyy/fsearch/compare/v0.7.0...v0.7.1
[0.7.0]: https://github.com/msxyy/fsearch/compare/v0.6.1...v0.7.0
[0.6.1]: https://github.com/msxyy/fsearch/compare/v0.6.0...v0.6.1
[0.6.0]: https://github.com/msxyy/fsearch/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/msxyy/fsearch/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/msxyy/fsearch/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/msxyy/fsearch/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/msxyy/fsearch/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/msxyy/fsearch/releases/tag/v0.1.0