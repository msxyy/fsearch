# fsearch

[![CI](https://github.com/msxyy/fsearch/actions/workflows/ci.yml/badge.svg)](https://github.com/msxyy/fsearch/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org)

用 Rust 写的文件内容搜索器，是 ripgrep 的简化版。

## 下载

不想自己编译？直接下载预编译版本：

到 [Releases](https://github.com/msxyy/fsearch/releases) 页面，下载对应平台的压缩包，解压后即可使用。

| 平台 | 文件 |
|------|------|
| Windows x86_64 | `fsearch-windows-x86_64.zip` |
| Linux x86_64 | `fsearch-linux-x86_64.tar.gz` |
| macOS Intel | `fsearch-macos-x86_64.tar.gz` |
| macOS Apple Silicon | `fsearch-macos-aarch64.tar.gz` |

## 特性

- 正则搜索（`regex` crate）
- 递归目录 + 自动读取 `.gitignore`
- 忽略大小写、行号、只列文件名、计数
- 上下文：`-A` / `-B` / `-C`
- 全词匹配 `-w`、反向匹配 `-v`、固定字符串 `-F`
- 只输出匹配部分 `-o`
- 替换输出 `-r`
- 多模式 `-e`
- 按文件类型过滤 `--type`、排除类型 `--type-not`
- 按文件名搜索 `--name`、只列文件 `--files`
- 列出不匹配文件 `--files-without-match`
- 统计匹配次数 `--count-matches`
- 每文件上限 `-m N`、统计 `--stats`
- 排序输出 `--sort`
- 编码支持 `--encoding`（GBK、Big5、Shift_JIS 等）
- `-u` / `-uu` / `-uuu` 逐级放宽忽略规则
- 递归深度 `--max-depth`、文件大小 `--max-filesize`
- 静默模式 `-q`
- JSON Lines 输出 `--json`（含 begin/end/summary 事件）
- Shell 补全生成 `--generate-completions`
- Man 手册生成 `--generate-man`
- 并行遍历（`ignore` 的并行 Walker）
- 流式逐行搜索，内存与文件大小无关
- 支持非 UTF-8，跳过二进制文件
- 彩色高亮、正确的退出码（0/1/2）

## 安装

### 方式一：下载二进制（推荐）

见上文「下载」章节。

### 方式二：从源码编译

```bash
git clone https://github.com/msxyy/fsearch.git
cd fsearch
cargo build --release
```

编译好的 exe 在 `target/release/fsearch.exe`（Windows）或 `target/release/fsearch`（Linux/macOS）。

### 方式三：cargo install

```bash
cargo install --git https://github.com/msxyy/fsearch
```

## Shell 补全与 man 手册

`fsearch` 可以生成自己的补全脚本和手册页。

```bash
# bash
fsearch --generate-completions bash > fsearch.bash
sudo cp fsearch.bash /usr/share/bash-completion/completions/fsearch

# zsh
fsearch --generate-completions zsh > _fsearch
sudo cp _fsearch /usr/local/share/zsh/site-functions/_fsearch

# fish
fsearch --generate-completions fish > fsearch.fish
cp fsearch.fish ~/.config/fish/completions/fsearch.fish

# PowerShell：把下面这行加到 $PROFILE
fsearch --generate-completions powershell > fsearch.ps1
# . D:\path\to\fsearch.ps1

# man 手册
fsearch --generate-man > fsearch.1
sudo cp fsearch.1 /usr/share/man/man1/
```

## 用法

```
fsearch [OPTIONS] <PATTERN> [PATH]
fsearch [OPTIONS] -e <PATTERN> -e <PATTERN> ... [PATH]
fsearch [OPTIONS] --files [PATH]
```

`PATTERN` 是搜索模式，默认按正则解释。`PATH` 是搜索路径，默认当前目录。

### 常用示例

```bash
# 在当前目录搜索
fsearch "fn main" .

# 忽略大小写 + 行号
fsearch -i -n "error" ./src

# 只列出匹配到的文件
fsearch -l "TODO" .

# 只输出匹配行数
fsearch -c "unsafe" ./src

# 显示匹配行前后各 2 行上下文
fsearch -C 2 "panic" ./src

# 显示匹配行后 2 行
fsearch -A 2 "fn main" ./src

# 显示匹配行前 2 行
fsearch -B 2 "fn main" ./src

# 全词匹配，不会匹配到 define 里的 fn
fsearch -w "fn" ./src

# 反向匹配：输出不含 use 的行
fsearch -v "use" ./src

# 固定字符串，不解释正则元字符
fsearch -F "fn(" ./src

# 只输出匹配部分
fsearch -o "fn \w+" ./src

# 替换匹配部分（不写回文件）
fsearch -r "BAR" "foo" ./src

# 多个模式，任一匹配即命中
fsearch -e "fn main" -e "fn run" -n ./src

# 只搜 Rust 文件
fsearch --type rust "Result" .

# 排除 Rust 文件
fsearch --type-not rust -l "TODO" .

# 只搜匹配 glob 的文件
fsearch -g "*.toml" "version" .

# 按文件名搜索
fsearch --name "main" .

# 只列出会被搜索的文件
fsearch --files .

# 列出不含匹配的文件
fsearch --files-without-match "fn main" ./src

# 统计匹配次数（每次出现都算一次）
fsearch --count-matches "fn " ./src

# 每个文件最多输出 1 条匹配
fsearch -m 1 "fn " ./src

# 输出统计信息
fsearch --stats "Result" ./src

# 排序输出（并行搜索，按路径排序打印）
fsearch --sort -l "fn " .

# 忽略 .gitignore
fsearch --no-ignore -l "target" .

# 忽略 .gitignore，且搜索隐藏文件
fsearch -uu "target" .

# 不读 .gitignore，搜隐藏文件，也搜二进制
fsearch -uuu "fsearch" .

# 限制递归深度
fsearch --max-depth 1 -l "fn " .

# 跳过大于 1MB 的文件
fsearch --max-filesize 1M -l "fn " .

# 静默模式，只返回退出码
fsearch -q "fn main" ./src
echo $LASTEXITCODE

# 搜 GBK 编码的文件
fsearch --encoding gbk "中文" .

# JSON Lines 输出
fsearch --json "fn main" ./src

# 组合：忽略大小写、行号、上下文、只搜 Rust 文件
fsearch -i -n -C 1 --type rust "result" .
```

## 所有参数

| 参数 | 含义 |
|------|------|
| `-i` / `--ignore-case` | 忽略大小写 |
| `-n` / `--line-number` | 显示行号 |
| `-l` / `--files-with-matches` | 只输出匹配到的文件名 |
| `-c` / `--count` | 只输出匹配行数 |
| `-C N` / `--context N` | 匹配行前后各 N 行上下文 |
| `-A N` / `--after N` | 匹配行后 N 行上下文 |
| `-B N` / `--before N` | 匹配行前 N 行上下文 |
| `-w` / `--word` | 全词匹配 |
| `-v` / `--invert` | 反向匹配，输出不含模式的行 |
| `-F` / `--fixed` | 把模式当作固定字符串，不解释为正则 |
| `-o` / `--only-matching` | 只输出匹配部分，而不是整行 |
| `-r TEXT` / `--replace TEXT` | 把匹配部分替换成 TEXT，输出替换后的行（不写回文件） |
| `-e PATTERN` / `--regexp PATTERN` | 添加搜索模式，可重复；与位置参数 PATTERN 可同时使用 |
| `-m N` / `--max-count N` | 每个文件最多输出 N 条匹配 |
| `-g GLOB` / `--glob` | 只搜索匹配 glob 的文件 |
| `--type TYPE` | 按文件类型过滤，可重复 |
| `--type-not TYPE` | 排除指定文件类型，可重复 |
| `--name` | 按文件名搜索，不搜索文件内容 |
| `--files` | 只列出会被搜索的文件，不搜索内容 |
| `--files-without-match` | 列出不含匹配的文件（`-l` 的反向） |
| `--count-matches` | 统计匹配次数（每次出现都算一次）；隐含 `-c` |
| `--sort` | 并行搜索但输出按路径排序 |
| `--json` | 以 JSON Lines 格式输出 |
| `--stats` | 输出统计信息（扫描文件数、匹配数、耗时） |
| `--encoding ENC` | 指定文件编码，如 gbk、big5、shift_jis |
| `--no-ignore` | 不读取 `.gitignore` / `.ignore` |
| `-u` / `-uu` / `-uuu` | 逐级放宽忽略规则 |
| `--max-depth N` | 限制递归深度 |
| `--max-filesize SIZE` | 跳过大于 SIZE 的文件，支持 `10M`、`1G`、`500K` |
| `-q` / `--quiet` | 静默模式，只返回退出码 |
| `--color MODE` | 颜色模式：`auto` / `always` / `never` |
| `--follow` | 跟随符号链接 |
| `--hidden` | 搜索隐藏文件 |
| `--generate-completions SHELL` | 生成 shell 补全脚本到 stdout |
| `--generate-man` | 生成 man 手册页到 stdout |
| `-h` / `--help` | 显示帮助 |
| `-V` / `--version` | 显示版本 |

### `-u` / `-uu` / `-uuu` 的语义

| 参数 | 效果 |
|------|------|
| （默认） | 读 `.gitignore`，跳过隐藏文件，跳过二进制 |
| `-u` | 不读 `.gitignore` |
| `-uu` | 不读 `.gitignore`，且搜索隐藏文件 |
| `-uuu` | 不读 `.gitignore`，搜索隐藏文件和二进制文件 |

## 退出码

| 码 | 含义 |
|----|------|
| 0 | 找到至少一处匹配 |
| 1 | 没有找到任何匹配 |
| 2 | 出错（正则非法、路径不存在、未知类型等） |

与 `grep`、`rg`、`fd` 的约定一致，方便在脚本和 CI 里使用。

## 支持的文件类型

`--type` / `--type-not` 参数支持以下类型：

| 类型名 | 扩展名 |
|--------|--------|
| `rust` / `rs` | `.rs` |
| `python` / `py` | `.py` `.pyi` |
| `javascript` / `js` | `.js` `.mjs` `.cjs` `.jsx` |
| `typescript` / `ts` | `.ts` `.tsx` |
| `go` | `.go` |
| `c` | `.c` `.h` |
| `cpp` / `c++` / `cc` | `.cpp` `.cc` `.cxx` `.hpp` `.hh` `.hxx` |
| `java` | `.java` |
| `ruby` / `rb` | `.rb` |
| `php` | `.php` |
| `perl` | `.pl` `.pm` |
| `markdown` / `md` | `.md` `.markdown` |
| `toml` | `.toml` |
| `yaml` / `yml` | `.yaml` `.yml` |
| `json` | `.json` |
| `xml` | `.xml` |
| `html` | `.html` `.htm` |
| `css` | `.css` |
| `scss` | `.scss` |
| `shell` / `sh` / `bash` | `.sh` `.bash` `.zsh` `.fish` |
| `sql` | `.sql` |
| `lua` | `.lua` |
| `swift` | `.swift` |
| `kotlin` / `kt` | `.kt` `.kts` |
| `scala` | `.scala` |
| `haskell` / `hs` | `.hs` `.lhs` |

## 支持的编码

`--encoding` 参数接受 `encoding_rs` 支持的所有编码标签，常用：

`utf-8`（默认，走流式）、`gbk`、`gb18030`、`big5`、`shift_jis`、`euc-jp`、`euc-kr`、`windows-1252`、`iso-8859-1` 等。

指定非 UTF-8 编码时，整个文件会被解码到内存再搜索；默认 UTF-8 走流式，内存占用与文件大小无关。

## JSON 输出格式

`--json` 模式下每行一个独立的 JSON 对象，方便被工具或脚本逐行解析。

每个文件的输出以 `begin` 事件开始，`end` 事件结束：

```json
{"type":"begin","path":"./src/main.rs"}
{"type":"match","path":"./src/main.rs","line_number":20,"line":"fn main() -> ExitCode {","submatches":[{"start":0,"end":7}]}
{"type":"context","path":"./src/main.rs","line_number":21,"line":"    match run() {"}
{"type":"separator"}
{"type":"end","path":"./src/main.rs"}
```

`submatches` 是匹配在一行中的字节区间数组。

`-c` / `--count-matches` 模式下的统计事件：

```json
{"type":"count","path":"./src/main.rs","count":2}
```

`-l` 模式下的文件事件：

```json
{"type":"file","path":"./src/main.rs"}
```

`-o` 模式下的匹配事件，只含匹配文本：

```json
{"type":"match","path":"./src/main.rs","line_number":20,"match":"fn main"}
```

`--stats` 模式末尾的汇总事件：

```json
{"type":"summary","files_scanned":12,"files_matched":3,"matches":7,"elapsed_nanos":1234567}
```

## 项目结构

```
src/
├── main.rs     # 入口、参数组装、并行遍历
├── cli.rs      # clap 参数定义
├── search.rs   # 单文件流式搜索
└── printer.rs  # 输出（普通 / JSON / 静默）

tests/
└── cli.rs      # 集成测试

benches/
└── search.rs   # 简易 benchmark
```

## 测试

```bash
cargo test
```

包含 **89 个测试**：

- 52 个单元测试：搜索逻辑、参数组装、JSON 转义、多模式合并、大小解析
- 37 个集成测试：调用真实 exe，验证端到端行为

## 代码质量

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

CI 会在 Ubuntu、Windows、macOS 三个平台上跑格式检查、clippy 和测试。

## 性能

release 构建：

```bash
cargo build --release
```

与 ripgrep 对比（需要 `hyperfine`）：

```bash
cargo install hyperfine

hyperfine --warmup 3 \
  "./target/release/fsearch -n 'fn ' ./src" \
  "rg -n 'fn ' ./src"
```

`rg` 通常更快，它做了字面量预筛、SIMD、内存映射、并行管线等更激进的优化。`fsearch` 的目标是**功能对等 + 流式内存**，而不是性能击败 `rg`。

项目内自带简易 benchmark：

```bash
cargo bench
```

## 设计要点

- **并行搜索**：`ignore::WalkParallel` 遍历文件，多线程并行搜索
- **流式处理**：逐行读取文件，内存占用与文件大小无关
- **环形缓冲**：`-B` 上下文只保留最近 N 行，不会随文件增大
- **非 UTF-8 安全**：非法字节用替代字符替换，不会中断搜索
- **二进制检测**：读取前 8KB，遇到 `\0` 跳过（`-uuu` 时也搜索）
- **输出原子性**：每个文件的结果整体加锁打印，不会交错
- **`-m` 与上下文**：达到 `-m` 上限后仍会输出当前匹配的 after 上下文，与 ripgrep 行为一致
- **`--replace` 与高亮**：替换后清空匹配区间，避免字节偏移越界
- **`.gitignore` 生效范围**：即使不在 git 仓库里，也会读取 `.gitignore`（通过 `require_git(false)`）
- **`--sort` 模式**：并行搜索，收集结果后按路径排序，再串行打印
- **`-e` 与位置参数**：有 `-e` 时，位置参数被当作路径；无 `-e` 时，位置参数当作模式

## 后续规划

- [ ] 增量索引模式
- [ ] 字面量预筛（`aho-corasick`）提升性能
- [ ] 配置文件 `.fsearch.toml`
- [ ] `lib.rs` 拆分，供其他程序依赖

## 贡献

欢迎提交 Issue 和 PR。

## 许可

本项目基于 [MIT 协议](LICENSE) 发布。