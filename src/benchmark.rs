use std::{sync::Arc, time::Instant};

use drillx::equix;
use solana_rpc_client::spinner;

use crate::{args::BenchmarkArgs, Miner};

const TEST_DURATION: i64 = 30; //基准测试持续时间(秒)

impl Miner {
    // 用于进行性能基准测试:这段代码主要用于多线程并行计算来评估矿工的哈希能力。在指定的时间内，它在多个CPU核心上运行哈希计算，
    // 并最终输出每秒的哈希数量。这种基准测试可以用于优化矿工的性能或不同配置的哈希。
    pub async fn benchmark(&self, args: BenchmarkArgs) {
        // 检查线程的核心数是否有效
        self.check_num_cores(args.cores);
        // 初始化基准测试
        let challenge = [0; 32]; // 哈希挑战，初始化为32字节的零数组

        let progress_bar = Arc::new(spinner::new_progress_bar()); // 创建进度条
        progress_bar.set_message(format!(
            "Benchmarking. This will take {} sec...",
            TEST_DURATION // 设置进度条消息，显示预计时间
        ));

        // 获取可用的CPU核心ID
        let core_ids = core_affinity::get_core_ids().unwrap();
        // 为每个核心线程分派工作
        let handles: Vec<_> = core_ids.into_iter().map(|i| {
                std::thread::spawn({
                    move || {
                        let timer = Instant::now(); //记录开始时间
                        // 计算每个线程的初始nonce值
                        let first_nonce = u64::MAX.saturating_div(args.cores).saturating_mul(i.id as u64);
                        let mut nonce = first_nonce; // 初始化nonce
                        let mut memory = equix::SolverMemory::new(); // 创建哈希计算内存
                        loop {
                            // 如果线程超ID超过核心数，返回0
                            if (i.id as u64).ge(&args.cores) {
                                return 0;
                            }

                            // 将当前线程绑定到特定核心
                            let _ = core_affinity::set_for_current(i);

                            // 计算哈希值
                            let _hx = drillx::hash_with_memory(
                                &mut memory,
                                &challenge,
                                &nonce.to_le_bytes(),
                            );

                            // 增加nonce值
                            nonce += 1;

                            // 如果经过的时间超过指定的测试时间，退出循环
                            if (timer.elapsed().as_secs() as i64).ge(&TEST_DURATION) {
                                break;
                            }
                        }

                        // 返回当前线程计算的哈希数量
                        nonce - first_nonce
                    }
                })
            })
            .collect();

        // 等待所有线程完成并汇总结果
        let mut total_nonces = 0;
        for h in handles { // 等待线程结束并获取返回值
            if let Ok(count) = h.join() {
                total_nonces += count; // 累加每个线程的nonce计数
            }
        }

        // 更新进度条并输出哈希能力
        progress_bar.finish_with_message(format!(
            "Hashpower: {} H/sec", // 输出每秒哈希数量
            total_nonces.saturating_div(TEST_DURATION as u64),
        ));
    }
}
