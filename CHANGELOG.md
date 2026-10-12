# Changelog

所有重要变更都记录在这个文件里。

格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。

## [Unreleased]

## [0.1.0] - 2026-10-12

### Added

- 正则搜索，支持 `-i` 忽略大小写
- 递归目录遍历，自动读取 `.gitignore`
- 输出控制：`-n` 行号、`-l` 文件名、`-c` 计数
- 上下文：`-C` / `-A` / `-B`
- 模式控制：`-w` 全词、`-v` 反向、`-F` 固定字符串
- 输出模式：`-o` 只输出匹配部分
- 文件过滤：`-g` glob、`--type` 按类型
- 特殊模式：`--name` 按文件名搜索、`--files` 只列文件
- 限制与统计：`-m N` 每文件上限、`--stats`
- 忽略控制：`--no-ignore`
- 输出格式：`--json` JSON Lines
- 并行遍历 + 流式逐行搜索 + 环形缓冲
- 支持非 UTF-8，跳过二进制文件
- 退出码：0 找到 / 1 未找到 / 2 出错
- 47 个测试：31 单元 + 16 集成

[Unreleased]: https://github.com/yourname/fsearch/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/yourname/fsearch/releases/tag/v0.1.0