# Changelog

所有重要变更都记录在这个文件里。

格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。

## [Unreleased]

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
- `-r TEXT` / `--replace TEXT` 替换匹配部分，输出替换后的行
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
- 47 个测试：31 单元 + 16 集成

[Unreleased]: https://github.com/msxyy/fsearch/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/msxyy/fsearch/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/msxyy/fsearch/releases/tag/v0.1.0