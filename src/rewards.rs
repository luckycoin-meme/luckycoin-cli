use crate::{
    utils::{amount_u64_to_string, get_config},
    Miner,
};

impl Miner {
    // 用于计算和显示奖励信息
    pub async fn rewards(&self) {
        // 从RPC客户端获取配置
        let config = get_config(&self.rpc_client).await;
        // 设置基础奖励
        let base_reward_rate = config.base_reward_rate;

        // 初始化字符串，用于存储奖励信息
        let mut s = format!(
            "{}: {} ORE", // 初始化格式信息
            config.min_difficulty,
            amount_u64_to_string(base_reward_rate) // 将基础奖励率转换为可读格式
        )
            .to_string();
        // 循环计算奖励率，从 1 到 31
        for i in 1..32 {
            // 计算当前难度下的奖励率，使用 saturating_mul 进行溢出保护
            let reward_rate = base_reward_rate.saturating_mul(2u64.saturating_pow(i));
            // 更新字符串，将当前难度和对应的奖励率添加到信息中
            s = format!(
                "{}\n{}: {} ORE", // 格式化当前难度和奖励信息
                s,
                config.min_difficulty as u32 + i, // 当前难度
                amount_u64_to_string(reward_rate) //当前奖励率的可读格式
            );
        }
        // 打印最终的奖励信息
        println!("{}", s);
    }
}
