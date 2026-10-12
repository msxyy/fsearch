# fsearch

用 Rust 写的文件内容搜索器，是 ripgrep 的简化版。

## 特性

- 正则搜索（`regex` crate）
- 递归目录 + 自动读取 `.gitignore`
- 忽略大小写、行号、只列文件名、计数
- 上下文：`-A` / `-B` / `-C`
- 全词匹配 `-w`、反向匹配 `-v`、固定字符串 `-F`
- 只输出匹配部分 `-o`
- 按文件类型过滤 `--type rust`
- JSON Lines 输出 `--json`
- 并行遍历（`ignore` 的并行 Walker）
- 流式逐行搜索，内存与文件大小无关
- 支持非 UTF-8，跳过二进制文件
- 彩色高亮、正确的退出码（0/1/2）

## 安装

从源码安装：

```bash
cargo install --path .
```

安装后可在任意目录使用：

```bash
fsearch "fn main" .
```

## 用法

```
fsearch [OPTIONS] <PATTERN> [PATH]
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

# 只搜 Rust 文件
fsearch --type rust "Result" .

# 只搜匹配 glob 的文件
fsearch -g "*.toml" "version" .

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
| `-g GLOB` / `--glob` | 只搜索匹配 glob 的文件 |
| `--type TYPE` | 按文件类型过滤，可重复 |
| `--json` | 以 JSON Lines 格式输出 |
| `--color MODE` | 颜色模式：`auto` / `always` / `never` |
| `--follow` | 跟随符号链接 |
| `--hidden` | 搜索隐藏文件 |
| `-h` / `--help` | 显示帮助 |
| `-V` / `--version` | 显示版本 |

## 退出码

| 码 | 含义 |
|----|------|
| 0 | 找到至少一处匹配 |
| 1 | 没有找到任何匹配 |
| 2 | 出错（正则非法、路径不存在、未知类型等） |

与 `grep`、`rg`、`fd` 的约定一致，方便在脚本和 CI 里使用。

## 支持的文件类型

`--type` 参数支持以下类型：

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

## JSON 输出格式

`--json` 模式下每行一个独立的 JSON 对象，方便被工具或脚本逐行解析。

匹配行事件：

```json
{"type":"match","path":"./src/main.rs","line_number":20,"line":"fn main() -> ExitCode {","submatches":[{"start":0,"end":7}]}
```

`submatches` 是匹配在一行中的字节区间数组。

上下文行事件：

```json
{"type":"context","path":"./src/main.rs","line_number":21,"line":"    match run() {"}
```

分组分隔符事件：

```json
{"type":"separator"}
```

`-c` 模式下的统计事件：

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

## 项目结构

```
src/
├── main.rs     # 入口、参数组装、并行遍历
├── cli.rs      # clap 参数定义
├── search.rs   # 单文件流式搜索
└── printer.rs  # 输出（普通 / JSON）

tests/
└── cli.rs      # 集成测试
```

## 测试

```bash
cargo test
```

包含 **38 个测试**：

- 28 个单元测试：搜索逻辑、参数组装、JSON 转义
- 10 个集成测试：调用真实 exe，验证端到端行为

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

## 设计要点

- **并行搜索**：`ignore::WalkParallel` 遍历文件，多线程并行搜索
- **流式处理**：逐行读取文件，内存占用与文件大小无关
- **环形缓冲**：`-B` 上下文只保留最近 N 行，不会随文件增大
- **非 UTF-8 安全**：非法字节用替代字符替换，不会中断搜索
- **二进制检测**：读取前 8KB，遇到 `\0` 跳过
- **输出原子性**：每个文件的结果整体加锁打印，不会交错

## 许可

MIT