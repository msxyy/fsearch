use clap::Parser;

/// 一个简单的文件内容搜索器
#[derive(Parser, Debug)]
#[command(name = "fsearch", version, about = "在文件中搜索匹配的内容")]
#[command(long_about = "在文件中搜索匹配的内容。\n\n\
默认搜索文件内容。\n\
--name  按文件名搜索。\n\
--files 只列出会被搜索的文件。")]
pub struct Cli {
    /// 搜索模式；--files 模式下可省略
    pub pattern: Option<String>,

    /// 搜索路径，默认当前目录；--files 模式下唯一位置参数即路径
    #[arg(default_value = ".")]
    pub path: String,

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
    #[arg(long)]
    pub stats: bool,

    /// 不读取 .gitignore / .ignore
    #[arg(long)]
    pub no_ignore: bool,

    /// 忽略大小写
    #[arg(short = 'i', long)]
    pub ignore_case: bool,

    /// 显示行号
    #[arg(short = 'n', long)]
    pub line_number: bool,

    /// 只输出匹配到的文件名
    #[arg(short = 'l', long)]
    pub files_with_matches: bool,

    /// 只输出匹配的行数
    #[arg(short = 'c', long)]
    pub count: bool,

    /// 显示匹配行前后的上下文行数
    #[arg(short = 'C', long, default_value_t = 0)]
    pub context: usize,

    /// 显示匹配行之后的上下文行数
    #[arg(short = 'A', long)]
    pub after: Option<usize>,

    /// 显示匹配行之前的上下文行数
    #[arg(short = 'B', long)]
    pub before: Option<usize>,

    /// 全词匹配
    #[arg(short = 'w', long)]
    pub word: bool,

    /// 反向匹配：输出不含模式的行
    #[arg(short = 'v', long)]
    pub invert: bool,

    /// 把模式当作固定字符串，不解释为正则
    #[arg(short = 'F', long)]
    pub fixed: bool,

    /// 只输出匹配的部分，而不是整行
    #[arg(short = 'o', long)]
    pub only_matching: bool,

    /// 按文件类型过滤，例如 --type rust，可重复
    #[arg(long = "type", value_name = "TYPE")]
    pub type_filter: Vec<String>,

    /// 以 JSON Lines 格式输出
    #[arg(long)]
    pub json: bool,

    /// 颜色模式：auto / always / never
    #[arg(long, default_value = "auto", value_parser = ["auto", "always", "never"])]
    pub color: String,

    /// 跟随符号链接
    #[arg(long)]
    pub follow: bool,

    /// 搜索隐藏文件
    #[arg(long)]
    pub hidden: bool,

    /// 只搜索匹配 glob 的文件，例如 -g "*.rs"
    #[arg(short = 'g', long)]
    pub glob: Option<String>,
}
