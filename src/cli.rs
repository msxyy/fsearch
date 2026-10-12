use clap::Parser;

/// 一个简单的文件内容搜索器
#[derive(Parser, Debug)]
#[command(name = "fsearch", version, about = "在文件中搜索匹配的内容")]
#[command(long_about = "在文件中搜索匹配的内容。\n\n\
默认搜索文件内容。\n\
--name  按文件名搜索。\n\
--files 只列出会被搜索的文件。\n\n\
-u / -uu / -uuu 逐级放宽忽略规则：\n\
-u    不读 .gitignore\n\
-uu   不读 .gitignore，且搜索隐藏文件\n\
-uuu  不读 .gitignore，搜索隐藏文件和二进制文件\n\n\
在搜索目录下放置 .fsearch.toml 可指定默认选项；命令行参数始终覆盖配置文件。")]
pub struct Cli {
    /// 搜索模式；使用 -e 时可省略；--files 模式下位置参数是路径
    pub pattern: Option<String>,

    /// 搜索路径，默认当前目录
    #[arg(default_value = ".")]
    pub path: String,

    /// 多个搜索模式，任一匹配即命中
    #[arg(short = 'e', long = "regexp", value_name = "PATTERN")]
    pub patterns: Vec<String>,

    /// 按文件名搜索，不搜索文件内容
    #[arg(long)]
    pub name: bool,

    /// 只列出会被搜索的文件，不搜索内容
    #[arg(long)]
    pub files: bool,

    /// 每个文件最多输出 N 条匹配
    #[arg(short = 'm', long = "max-count", value_name = "N")]
    pub max_count: Option<usize>,

    /// 输出统计信息（扫描文件数、匹配数、耗时）
    #[arg(long, num_args = 0, default_missing_value = "true")]
    pub stats: Option<bool>,

    /// 并行搜索但输出按路径排序
    #[arg(long, num_args = 0, default_missing_value = "true")]
    pub sort: Option<bool>,

    /// 把匹配部分替换成指定文本，输出替换后的行（不写回文件）
    #[arg(short = 'r', long = "replace", value_name = "TEXT")]
    pub replace: Option<String>,

    /// 不读取 .gitignore / .ignore
    #[arg(long, num_args = 0, default_missing_value = "true")]
    pub no_ignore: Option<bool>,

    /// 逐级放宽忽略规则：-u 不读 .gitignore；-uu 还搜隐藏文件；-uuu 还搜二进制
    #[arg(short = 'u', action = clap::ArgAction::Count)]
    pub unrestricted: u8,

    /// 指定文件编码，例如 gbk、gb18030、big5、shift_jis、euc-kr；默认 UTF-8
    #[arg(long, value_name = "ENCODING")]
    pub encoding: Option<String>,

    /// 限制递归深度
    #[arg(long = "max-depth", value_name = "N")]
    pub max_depth: Option<usize>,

    /// 跳过大于指定大小的文件，例如 10M、1G、500K
    #[arg(long = "max-filesize", value_name = "SIZE")]
    pub max_filesize: Option<String>,

    /// 静默模式：不输出匹配结果，只返回退出码
    #[arg(short = 'q', long, num_args = 0, default_missing_value = "true")]
    pub quiet: Option<bool>,

    /// 忽略大小写
    #[arg(short = 'i', long, num_args = 0, default_missing_value = "true")]
    pub ignore_case: Option<bool>,

    /// 显示行号
    #[arg(short = 'n', long, num_args = 0, default_missing_value = "true")]
    pub line_number: Option<bool>,

    /// 只输出匹配到的文件名
    #[arg(short = 'l', long, num_args = 0, default_missing_value = "true")]
    pub files_with_matches: Option<bool>,

    /// 只输出匹配的行数
    #[arg(short = 'c', long, num_args = 0, default_missing_value = "true")]
    pub count: Option<bool>,

    /// 统计匹配次数（每次出现都算一次），而非匹配行数；隐含 -c
    #[arg(long = "count-matches", num_args = 0, default_missing_value = "true",
          conflicts_with_all = ["files_with_matches", "files_without_match"])]
    pub count_matches: Option<bool>,

    /// 列出不含匹配的文件（-l 的反向）
    #[arg(long = "files-without-match", num_args = 0, default_missing_value = "true",
          conflicts_with_all = ["files_with_matches", "count", "count_matches"])]
    pub files_without_match: Option<bool>,

    /// 显示匹配行前后的上下文行数
    #[arg(short = 'C', long)]
    pub context: Option<usize>,

    /// 显示匹配行之后的上下文行数
    #[arg(short = 'A', long)]
    pub after: Option<usize>,

    /// 显示匹配行之前的上下文行数
    #[arg(short = 'B', long)]
    pub before: Option<usize>,

    /// 全词匹配
    #[arg(short = 'w', long, num_args = 0, default_missing_value = "true")]
    pub word: Option<bool>,

    /// 反向匹配：输出不含模式的行
    #[arg(short = 'v', long, num_args = 0, default_missing_value = "true")]
    pub invert: Option<bool>,

    /// 把模式当作固定字符串，不解释为正则
    #[arg(short = 'F', long, num_args = 0, default_missing_value = "true")]
    pub fixed: Option<bool>,

    /// 只输出匹配的部分，而不是整行
    #[arg(short = 'o', long, num_args = 0, default_missing_value = "true")]
    pub only_matching: Option<bool>,

    /// 按文件类型过滤，例如 --type rust，可重复
    #[arg(long = "type", value_name = "TYPE")]
    pub type_filter: Vec<String>,

    /// 排除指定文件类型，例如 --type-not rust，可重复
    #[arg(long = "type-not", value_name = "TYPE")]
    pub type_not_filter: Vec<String>,

    /// 以 JSON Lines 格式输出
    #[arg(long, num_args = 0, default_missing_value = "true")]
    pub json: Option<bool>,

    /// 颜色模式：auto / always / never
    #[arg(long, value_parser = ["auto", "always", "never"])]
    pub color: Option<String>,

    /// 跟随符号链接
    #[arg(long, num_args = 0, default_missing_value = "true")]
    pub follow: Option<bool>,

    /// 搜索隐藏文件
    #[arg(long, num_args = 0, default_missing_value = "true")]
    pub hidden: Option<bool>,

    /// 只搜索匹配 glob 的文件，例如 -g "*.rs"
    #[arg(short = 'g', long)]
    pub glob: Option<String>,

    /// 为指定目录建立/更新内容索引，保存到 <DIR>/.fsearch-index
    #[arg(long = "build-index", value_name = "DIR")]
    pub build_index: Option<String>,

    /// 使用已有索引加速搜索（仅支持长度 ≥ 3 的字面量 pattern）
    #[arg(long, num_args = 0, default_missing_value = "true")]
    pub use_index: Option<bool>,

    /// 生成 shell 补全脚本到 stdout（bash / zsh / fish / powershell / elvish）
    #[arg(long, hide = true, value_name = "SHELL")]
    pub generate_completions: Option<String>,

    /// 生成 man 手册页到 stdout
    #[arg(long, hide = true)]
    pub generate_man: bool,
}
