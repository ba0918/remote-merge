use clap::{Parser, Subcommand};
use remote_merge::config;

/// ローカルとリモートサーバ間のファイル差分をTUIでグラフィカルに表示・マージするツール
#[derive(Parser, Debug)]
#[command(name = "remote-merge", version, about)]
struct Cli {
    /// 比較対象のサーバ名（localとの比較）
    #[arg(short, long)]
    server: Option<String>,

    /// 比較の左側（デフォルト: local）
    #[arg(long)]
    left: Option<String>,

    /// 比較の右側
    #[arg(long)]
    right: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// プロジェクト設定ファイルを初期化する
    Init,

    /// 差分があるファイルの一覧を表示
    Status {
        /// 比較対象のサーバ名
        #[arg(short, long)]
        server: Option<String>,

        /// 比較の左側
        #[arg(long)]
        left: Option<String>,

        /// 比較の右側
        #[arg(long)]
        right: Option<String>,

        /// 出力フォーマット (text / json)
        #[arg(long, default_value = "text")]
        format: String,

        /// サマリーのみ出力
        #[arg(long)]
        summary: bool,
    },

    /// 特定ファイルの差分を表示
    Diff {
        /// 対象パス
        path: String,

        /// 比較の左側
        #[arg(long)]
        left: Option<String>,

        /// 比較の右側
        #[arg(long)]
        right: Option<String>,

        /// 出力フォーマット (text / json)
        #[arg(long, default_value = "text")]
        format: String,

        /// 出力行数の上限
        #[arg(long)]
        max_lines: Option<usize>,

        /// 出力ファイル数の上限（ディレクトリ指定時）
        #[arg(long)]
        max_files: Option<usize>,
    },

    /// ファイルをマージする
    Merge {
        /// 対象パス
        path: String,

        /// マージ元（この内容でマージ先を上書き）
        #[arg(long)]
        left: Option<String>,

        /// マージ先
        #[arg(long)]
        right: Option<String>,

        /// 実行せず確認のみ
        #[arg(long)]
        dry_run: bool,

        /// 確認プロンプトを省略
        #[arg(long)]
        force: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ログ初期化
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Init) => {
            // TODO: Phase 1-4 で実装
            println!("remote-merge init: .remote-merge.toml を生成します（未実装）");
        }
        Some(Commands::Status { .. }) => {
            // TODO: Phase 4 で実装
            println!("remote-merge status: 差分一覧を表示します（未実装）");
        }
        Some(Commands::Diff { .. }) => {
            // TODO: Phase 4 で実装
            println!("remote-merge diff: 差分を表示します（未実装）");
        }
        Some(Commands::Merge { .. }) => {
            // TODO: Phase 4 で実装
            println!("remote-merge merge: マージを実行します（未実装）");
        }
        None => {
            // TUI モード
            // サーバ設定を読み込む
            let config = config::load_config()?;

            let server_name = cli
                .server
                .or(cli.right)
                .unwrap_or_else(|| {
                    // 設定の最初のサーバを使用
                    config
                        .servers
                        .keys()
                        .next()
                        .cloned()
                        .unwrap_or_else(|| "develop".to_string())
                });

            tracing::info!(
                "TUI モード起動: local ↔ {}",
                server_name
            );

            // TODO: Phase 1-2 で TUI を実装
            println!(
                "remote-merge TUI モード: local ↔ {} （未実装）",
                server_name
            );
            println!("設定ファイルの読み込みに成功しました");
            println!("サーバ数: {}", config.servers.len());
            println!("ローカルルート: {}", config.local.root_dir.display());
        }
    }

    Ok(())
}
