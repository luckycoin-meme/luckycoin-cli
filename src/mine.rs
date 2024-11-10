use std::{
    sync::{Arc, RwLock},
    time::Instant,
    usize,
};

use colored::*;
use drillx::{
    equix::{self},
    Hash, Solution,
};
use ore_api::{
    consts::{BUS_ADDRESSES, BUS_COUNT, EPOCH_DURATION},
    state::{Bus, Config},
};
use ore_utils::AccountDeserialize;
use rand::Rng;
use solana_program::pubkey::Pubkey;
use solana_rpc_client::spinner;
use solana_sdk::signer::Signer;

use crate::{
    args::MineArgs,
    error::Error,
    pool::Pool,
    send_and_confirm::ComputeBudget,
    utils::{
        amount_u64_to_string, get_clock, get_config, get_updated_proof_with_authority, proof_pubkey,
    },
    Miner,
};

impl Miner {
    // 定义一个公共的异步函数 `mine`，用于处理矿工的不同挖掘模式（池挖矿或单人挖矿）。
    pub async fn mine(&self, args: MineArgs) -> Result<(), Error> {
        // 使用 `match` 语句处理 `args.pool_url` 的 `Some` 和 `None` 两种情况。
        match args.pool_url {
            // 当 `args.pool_url` 为 `Some` 时，表示用户指定了矿池 URL。
            Some(ref pool_url) => {
                // 创建一个 `Pool` 结构体实例，包含 HTTP 客户端和矿池 URL。
                let pool = &Pool {
                    http_client: reqwest::Client::new(), // 创建一个新的HTTP客户端
                    pool_url: pool_url.clone(), // 复制矿池URL
                };
                // 调用 `mine_pool` 异步方法，并等待其完成。
                // 使用 `?` 操作符处理可能产生的错误，并将错误向上抛出。
                self.mine_pool(args, pool).await?;
            }
            // 当 `args.pool_url` 为 `None` 时，表示用户选择单人挖矿。
            None => {
                // 调用 `mine_solo` 异步方法，并等待其完成。
                self.mine_solo(args).await;
            }
        }
        // 返回 `Ok(())`，表示函数成功执行。
        Ok(())
    }

    async fn mine_solo(&self, args: MineArgs) {
        // 如果需要，打开账户
        let signer = self.signer();
        self.open().await;

        // 检查线程数
        self.check_num_cores(args.cores);

        // 开始循环挖矿
        let mut last_hash_at = 0;
        let mut last_balance = 0;
        loop {
            // 获取工作量证明
            let config = get_config(&self.rpc_client).await;
            println!("config.................: {:?}", config);
            // 打印当前状态信息
            let proof = get_updated_proof_with_authority(&self.rpc_client, signer.pubkey(), last_hash_at).await;
            println!(
                "\n\nStake: {} ORE\n{}  Multiplier: {:12}x",
                amount_u64_to_string(proof.balance),
                if last_hash_at.gt(&0) {
                    format!(
                        "  Change: {} ORE\n",
                        amount_u64_to_string(proof.balance.saturating_sub(last_balance))
                    )
                } else {
                    "".to_string()
                },
                calculate_multiplier(proof.balance, config.top_balance)
            );
            // 更新上次的哈希值和余额
            last_hash_at = proof.last_hash_at;
            last_balance = proof.balance;

            // 计算截止时间
            let cutoff_time = self.get_cutoff(proof.last_hash_at, args.buffer_time).await;

            // 构建Nonce索引
            let mut nonce_indices = Vec::with_capacity(args.cores as usize);
            for n in 0..(args.cores) {
                let nonce = u64::MAX.saturating_div(args.cores).saturating_mul(n);
                nonce_indices.push(nonce);
            }

            // 运行挖矿算法
            let solution = Self::find_hash_par(
                proof.challenge,
                cutoff_time,
                args.cores,
                config.min_difficulty as u32,
                nonce_indices.as_slice(),
            )
                .await;

            // 构建指令集
            let mut ixs = vec![ore_api::instruction::auth(proof_pubkey(signer.pubkey()))];
            let mut compute_budget = 500_000;

            // 根据条件增加计算预算并添加重置指令
            if self.should_reset(config).await && rand::thread_rng().gen_range(0..100).eq(&0) {
                compute_budget += 100_000;
                ixs.push(ore_api::instruction::reset(signer.pubkey()));
            }

            // 构建挖矿指令
            ixs.push(ore_api::instruction::mine(
                signer.pubkey(),
                signer.pubkey(),
                self.find_bus().await,
                solution,
            ));

            // 提交交易
            self.send_and_confirm(&ixs, ComputeBudget::Fixed(compute_budget), false)
                .await
                .ok();
        }
    }

    async fn mine_pool(&self, args: MineArgs, pool: &Pool) -> Result<(), Error> {
        // 注册矿池成员(如果需要)
        let mut pool_member = pool.post_pool_register(self).await?;
        // 获取矿池成员的索引
        let nonce_index = pool_member.id as u64;
        // 获取链上的矿池账户信息
        let pool_address = pool.get_pool_address().await?;
        // 初始化链上矿池成员的状态
        let mut pool_member_onchain: ore_pool_api::state::Member;
        // 检查线程数
        self.check_num_cores(args.cores);
        // 开始循环挖矿
        let mut last_hash_at = 0;
        let mut last_balance: i64;
        loop {
            // 获取最新的挑战信息
            let member_challenge = pool.get_updated_pool_challenge(last_hash_at).await?;
            // 更新上次的余额和哈希值
            last_balance = pool_member.total_balance;
            last_hash_at = member_challenge.challenge.lash_hash_at;
            // 计算截止时间
            let cutoff_time = self.get_cutoff(last_hash_at, member_challenge.buffer).await;
            // 构建Nonce索引
            let num_total_members = member_challenge.num_total_members.max(1);
            let u64_unit = u64::MAX.saturating_div(num_total_members);
            let left_bound = u64_unit.saturating_mul(nonce_index);
            let range_per_core = u64_unit.saturating_div(args.cores);
            let mut nonce_indices = Vec::with_capacity(args.cores as usize);
            for n in 0..(args.cores) {
                let index = left_bound + n * range_per_core;
                nonce_indices.push(index);
            }
            // 运行挖矿算法
            let solution = Self::find_hash_par(
                member_challenge.challenge.challenge,
                cutoff_time,
                args.cores,
                member_challenge.challenge.min_difficulty as u32,
                nonce_indices.as_slice(),
            )
                .await;
            // 向矿池运营商提交解决方案
            pool.post_pool_solution(self, &solution).await?;
            // 获取更新后的矿池成员信息
            pool_member = pool.get_pool_member(self).await?;
            // 获取链上更新后的矿池成员信息
            pool_member_onchain = pool
                .get_pool_member_onchain(self, pool_address.address)
                .await?;
            // 打印进度信息
            println!(
                "Claimable ORE balance: {}",
                amount_u64_to_string(pool_member_onchain.balance)
            );
            if last_hash_at.gt(&0) {
                println!(
                    "Change of ORE credits in pool: {}",
                    amount_u64_to_string(
                        pool_member.total_balance.saturating_sub(last_balance) as u64
                    )
                )
            }
        }
    }

    /*
     * 实现了一个并行挖矿函数 find_hash_par，用于寻找满足特定难度的哈希值
     */
    async fn find_hash_par(
        challenge: [u8; 32], // 哈希挑战值
        cutoff_time: u64, // 挖矿截止时间（秒）
        cores: u64, // 可用核心线程数
        min_difficulty: u32, // 最小挖矿难度要求
        nonce_indices: &[u64], // 非随机书索引列表
    ) -> Solution {
        // 创建一个可在线程间共享的进度条
        let progress_bar = Arc::new(spinner::new_progress_bar());
        // 创建一个可在线程间共享的读写锁，用于记录全局最佳难度
        let global_best_difficulty = Arc::new(RwLock::new(0u32));
        // 设置初始进度条消息
        progress_bar.set_message("Mining...");
        // 获取系统中的所有核心 ID，并过滤出指定数量的核心
        let core_ids = core_affinity::get_core_ids().unwrap();
        // 创建线程句柄向量，用于管理各个核心上的工作线程
        let core_ids = core_ids.into_iter().filter(|id| id.id < (cores as usize));
        let handles: Vec<_> = core_ids
            .map(|i| {
                let global_best_difficulty = Arc::clone(&global_best_difficulty);
                std::thread::spawn({
                    let progress_bar = progress_bar.clone();
                    let nonce = nonce_indices[i.id];
                    let mut memory = equix::SolverMemory::new();
                    move || {
                        // 将当前线程绑定到指定核心
                        let _ = core_affinity::set_for_current(i);

                        // 开始哈希计算
                        let timer = Instant::now();
                        let mut nonce = nonce;
                        let mut best_nonce = nonce;
                        let mut best_difficulty = 0;
                        let mut best_hash = Hash::default();
                        loop {
                            // 计算哈希值
                            let hxs = drillx::hashes_with_memory(
                                &mut memory,
                                &challenge,
                                &nonce.to_le_bytes(),
                            );

                            // 查找最佳难度分数
                            for hx in hxs {
                                let difficulty = hx.difficulty();
                                if difficulty.gt(&best_difficulty) {
                                    best_nonce = nonce;
                                    best_difficulty = difficulty;
                                    best_hash = hx;
                                    if best_difficulty.gt(&*global_best_difficulty.read().unwrap())
                                    {
                                        *global_best_difficulty.write().unwrap() = best_difficulty;
                                    }
                                }
                            }

                            // 如果达到截止时间，则退出循环
                            if nonce % 100 == 0 {
                                let global_best_difficulty =
                                    *global_best_difficulty.read().unwrap();
                                if timer.elapsed().as_secs().ge(&cutoff_time) {
                                    if i.id == 0 {
                                        progress_bar.set_message(format!(
                                            "Mining... (difficulty {})",
                                            global_best_difficulty,
                                        ));
                                    }
                                    if global_best_difficulty.ge(&min_difficulty) {
                                        // Mine until min difficulty has been met
                                        break;
                                    }
                                } else if i.id == 0 {
                                    progress_bar.set_message(format!(
                                        "Mining... (difficulty {}, time {})",
                                        global_best_difficulty,
                                        format_duration(
                                            cutoff_time.saturating_sub(timer.elapsed().as_secs())
                                                as u32
                                        ),
                                    ));
                                }
                            }

                            // 增加非随机数
                            nonce += 1;
                        }

                        // 返回最佳非随机数及其哈希值
                        (best_nonce, best_difficulty, best_hash)
                    }
                })
            })
            .collect();

        // 等待所有线程完成，并返回最佳非随机数
        let mut best_nonce = 0;
        let mut best_difficulty = 0;
        let mut best_hash = Hash::default();
        for h in handles {
            if let Ok((nonce, difficulty, hash)) = h.join() {
                if difficulty > best_difficulty {
                    best_difficulty = difficulty;
                    best_nonce = nonce;
                    best_hash = hash;
                }
            }
        }

        // 更新日志
        progress_bar.finish_with_message(format!(
            "Best hash: {} (difficulty {})",
            bs58::encode(best_hash.h).into_string(),
            best_difficulty
        ));

        Solution::new(best_hash.d, best_nonce.to_le_bytes())
    }

    pub fn check_num_cores(&self, cores: u64) {
        let num_cores = num_cpus::get() as u64;
        if cores.gt(&num_cores) {
            println!("{} Cannot exceeds available cores ({})", "WARNING".bold().yellow(),
                num_cores
            );
        }
    }

    async fn should_reset(&self, config: Config) -> bool {
        let clock = get_clock(&self.rpc_client).await;
        config
            .last_reset_at
            .saturating_add(EPOCH_DURATION)
            .saturating_sub(5) // Buffer
            .le(&clock.unix_timestamp)
    }

    async fn get_cutoff(&self, last_hash_at: i64, buffer_time: u64) -> u64 {
        let clock = get_clock(&self.rpc_client).await;
        last_hash_at
            .saturating_add(60)
            .saturating_sub(buffer_time as i64)
            .saturating_sub(clock.unix_timestamp)
            .max(0) as u64
    }

    async fn find_bus(&self) -> Pubkey {
        // Fetch the bus with the largest balance
        if let Ok(accounts) = self.rpc_client.get_multiple_accounts(&BUS_ADDRESSES).await {
            let mut top_bus_balance: u64 = 0;
            let mut top_bus = BUS_ADDRESSES[0];
            for account in accounts {
                if let Some(account) = account {
                    if let Ok(bus) = Bus::try_from_bytes(&account.data) {
                        if bus.rewards.gt(&top_bus_balance) {
                            top_bus_balance = bus.rewards;
                            top_bus = BUS_ADDRESSES[bus.id as usize];
                        }
                    }
                }
            }
            return top_bus;
        }

        // Otherwise return a random bus
        let i = rand::thread_rng().gen_range(0..BUS_COUNT);
        BUS_ADDRESSES[i]
    }
}

fn calculate_multiplier(balance: u64, top_balance: u64) -> f64 {
    1.0 + (balance as f64 / top_balance as f64).min(1.0f64)
}

fn format_duration(seconds: u32) -> String {
    let minutes = seconds / 60;
    let remaining_seconds = seconds % 60;
    format!("{:02}:{:02}", minutes, remaining_seconds)
}
