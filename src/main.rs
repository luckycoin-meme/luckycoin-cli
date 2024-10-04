mod args;
mod balance;
mod benchmark;
mod busses;
mod claim;
mod close;
mod config;
mod cu_limits;
mod dynamic_fee;
mod error;
#[cfg(feature = "admin")]
mod initialize;
mod mine;
mod open;
mod pool;
mod proof;
mod rewards;
mod send_and_confirm;
mod stake;
mod transfer;
mod upgrade;
mod utils;

use futures::StreamExt;
use pool::Pool;
use std::{sync::Arc, sync::RwLock};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::protocol::Message;

use args::*;
use clap::{command, Parser, Subcommand};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    signature::{read_keypair_file, Keypair},
};
use utils::Tip;

// 定义一个矿工(Miner)结构体，包含矿工的各种配置和状态信息
struct Miner {
    // 矿工的密钥对文件路径
    pub keypair_filepath: Option<String>,
    // 优选费用，可选
    pub priority_fee: Option<u64>,
    // 动态费用的URL,可选
    pub dynamic_fee_url: Option<String>,
    // 是否启用动态费用
    pub dynamic_fee: bool,
    // RPC客户端，用于区块链节点通信
    pub rpc_client: Arc<RpcClient>,
    // 费用支付者的密钥对文件路径，可选
    pub fee_payer_filepath: Option<String>,
    // JITO客户端，用于与JITO服务通信
    pub jito_client: Arc<RpcClient>,
    // 当前的小费（tip），使用读写锁保护，确保线程安全
    pub tip: Arc<std::sync::RwLock<u64>>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    #[command(about = "Fetch an account balance")]
    Balance(BalanceArgs),

    #[command(about = "Benchmark your hashpower")]
    Benchmark(BenchmarkArgs),

    #[command(about = "Fetch the bus account balances")]
    Busses(BussesArgs),

    #[command(about = "Claim your mining rewards")]
    Claim(ClaimArgs),

    #[command(about = "Close your account to recover rent")]
    Close(CloseArgs),

    #[command(about = "Fetch the program config")]
    Config(ConfigArgs),

    #[command(about = "Start mining")]
    Mine(MineArgs),

    #[command(about = "Fetch a proof account by address")]
    Proof(ProofArgs),

    #[command(about = "Fetch the current reward rate for each difficulty level")]
    Rewards(RewardsArgs),

    #[command(about = "Stake to earn a rewards multiplier")]
    Stake(StakeArgs),

    #[command(about = "Send ORE to anyone, anywhere in the world")]
    Transfer(TransferArgs),

    #[command(about = "Upgrade your ORE tokens from v1 to v2")]
    Upgrade(UpgradeArgs),

    #[command(about = "Update your on-chain pool balance on-demand")]
    UpdatePoolBalance(UpdatePoolBalanceArgs),

    #[cfg(feature = "admin")]
    #[command(about = "Initialize the program")]
    Initialize(InitializeArgs),
}

#[derive(Parser, Debug)]
#[command(about, version)]
struct Args {
    #[arg(
        long,
        value_name = "NETWORK_URL",
        help = "Network address of your RPC provider",
        global = true
    )]
    rpc: Option<String>,

    #[clap(
        global = true,
        short = 'C',
        long = "config",
        id = "PATH",
        help = "Filepath to config file."
    )]
    config_file: Option<String>,

    #[arg(
        long,
        value_name = "KEYPAIR_FILEPATH",
        help = "Filepath to signer keypair.",
        global = true
    )]
    keypair: Option<String>,

    #[arg(
        long,
        value_name = "FEE_PAYER_FILEPATH",
        help = "Filepath to transaction fee payer keypair.",
        global = true
    )]
    fee_payer: Option<String>,

    #[arg(
        long,
        value_name = "MICROLAMPORTS",
        help = "Price to pay for compute units. If dynamic fees are enabled, this value will be used as the cap.",
        default_value = "100000",
        global = true
    )]
    priority_fee: Option<u64>,

    #[arg(
        long,
        value_name = "DYNAMIC_FEE_URL",
        help = "RPC URL to use for dynamic fee estimation.",
        global = true
    )]
    dynamic_fee_url: Option<String>,

    #[arg(long, help = "Enable dynamic priority fees", global = true)]
    dynamic_fee: bool,

    #[arg(
        long,
        value_name = "JITO",
        help = "Add jito tip to the miner. Defaults to false.",
        global = true
    )]
    jito: bool,

    #[command(subcommand)]
    command: Commands,
}

#[tokio::main]
async fn main() {
    // 解析命令行参数
    let args = Args::parse();
    // Load the config file from custom path, the default path, or use default config values
    let cli_config = if let Some(config_file) = &args.config_file {
        // 如果指定了配置文件路径，则尝试加载它
        solana_cli_config::Config::load(config_file).unwrap_or_else(|_| {
            eprintln!("error: Could not find config file `{}`", config_file);
            std::process::exit(1); // 找不到配置文件则退出程序
        })
    } else if let Some(config_file) = &*solana_cli_config::CONFIG_FILE {
        // 如果没有指定，尝试加载默认配置文件
        solana_cli_config::Config::load(config_file).unwrap_or_default()
    } else {
        // 否则使用默认配置值
        solana_cli_config::Config::default()
    };

    // 初始化矿工所需的参数
    let cluster = args.rpc.unwrap_or(cli_config.json_rpc_url); // 获取RPC URL
    let default_keypair = args.keypair.unwrap_or(cli_config.keypair_path.clone()); // 获取密钥对路径
    let fee_payer_filepath = args.fee_payer.unwrap_or(default_keypair.clone()); // 获取费用支付者路径
    // 创建与 Solana 区块链的 RPC 客户端
    let rpc_client = RpcClient::new_with_commitment(cluster, CommitmentConfig::confirmed());
    // 创建与Jito 的 API 交互的 RPC 客户端
    let jito_client =
        RpcClient::new("https://mainnet.block-engine.jito.wtf/api/v1/transactions".to_string());

    // 创建共享状态用于存储小费信息
    let tip = Arc::new(RwLock::new(0_u64));

    let tip_clone = Arc::clone(&tip);

    // 流动性质押
    if args.jito {
        let url = "ws://bundles-api-rest.jito.wtf/api/v1/bundles/tip_stream"; // WebSocket URL
        let (ws_stream, _) = connect_async(url).await.unwrap(); // 连接到WebSocket
        let (_, mut read) = ws_stream.split(); // 拆分 WebSocket 流
        // 启动异步任务处理小费流
        tokio::spawn(async move {
            while let Some(message) = read.next().await { // 循环读取消息
                if let Ok(Message::Text(text)) = message { // 处理文本消息
                    if let Ok(tips) = serde_json::from_str::<Vec<Tip>>(&text) { // 解析JSON
                        for item in tips {
                            let mut tip = tip_clone.write().unwrap(); // 获取锁
                            *tip = (item.landed_tips_50th_percentile * (10_f64).powf(9.0)) as u64; // 更新小费值
                        }
                    }
                }
            }
        });
    }

    // 创建矿工实例
    let miner = Arc::new(Miner::new(
        Arc::new(rpc_client), // RPC客户端
        args.priority_fee, // 优先费用
        Some(default_keypair), // 密钥对
        args.dynamic_fee_url, // 动态费用URL
        args.dynamic_fee, // 动态费用标志
        Some(fee_payer_filepath), // 费用支付者文件路径
        Arc::new(jito_client), // Jito客户端
        tip, // 小费状态
    ));

    // 根据命令行参数执行相应的矿工操作
    match args.command {
        Commands::Balance(args) => {
            miner.balance(args).await; // 查询余额
        }
        Commands::Benchmark(args) => {
            miner.benchmark(args).await; // 进行基准测试
        }
        Commands::Busses(_) => {
            miner.busses().await; // 处理Busses操作
        }
        Commands::Claim(args) => {
            if let Err(err) = miner.claim(args).await { // 处理奖励领取操作
                println!("{:?}", err); // 打印错误
            }
        }
        Commands::Close(_) => {
            miner.close().await; // 关闭矿工
        }
        Commands::Config(_) => { // 关闭配置
            miner.config().await;
        }
        Commands::Mine(args) => {
            if let Err(err) = miner.mine(args).await { // 开始挖矿
                println!("{:?}", err); // 打印错误
            }
        }
        Commands::Proof(args) => {
            miner.proof(args).await; // 处理证明
        }
        Commands::Rewards(_) => { // 查询奖励
            miner.rewards().await;
        }
        Commands::Stake(args) => { //进行质押
            miner.stake(args).await;
        }
        Commands::Transfer(args) => { //进行转账
            miner.transfer(args).await;
        }
        Commands::Upgrade(args) => {
            miner.upgrade(args).await; // 升级矿工
        }
        Commands::UpdatePoolBalance(args) => {
            // 更新池余额
            let pool = Pool {
                http_client: reqwest::Client::new(), // 创建HTTP客户端
                pool_url: args.pool_url, // 池URL
            };
            if let Err(err) = pool.post_update_balance(miner.as_ref()).await { // 创建 HTTP 客户端
                println!("{:?}", err); // 打印错误
            }
        }
        #[cfg(feature = "admin")]
        Commands::Initialize(_) => {
            miner.initialize().await; // 初始化矿工（仅限管理员）
        }
    }
}

impl Miner {
    pub fn new(
        rpc_client: Arc<RpcClient>,
        priority_fee: Option<u64>,
        keypair_filepath: Option<String>,
        dynamic_fee_url: Option<String>,
        dynamic_fee: bool,
        fee_payer_filepath: Option<String>,
        jito_client: Arc<RpcClient>,
        tip: Arc<std::sync::RwLock<u64>>,
    ) -> Self {
        Self {
            rpc_client,
            keypair_filepath,
            priority_fee,
            dynamic_fee_url,
            dynamic_fee,
            fee_payer_filepath,
            jito_client,
            tip,
        }
    }

    pub fn signer(&self) -> Keypair {
        match self.keypair_filepath.clone() {
            Some(filepath) => read_keypair_file(filepath.clone())
                .expect(format!("No keypair found at {}", filepath).as_str()),
            None => panic!("No keypair provided"),
        }
    }

    pub fn fee_payer(&self) -> Keypair {
        match self.fee_payer_filepath.clone() {
            Some(filepath) => read_keypair_file(filepath.clone())
                .expect(format!("No fee payer keypair found at {}", filepath).as_str()),
            None => panic!("No fee payer keypair provided"),
        }
    }
}
