use luckycoin_api::{
    consts::{BUS_ADDRESSES, TOKEN_DECIMALS},
    state::Bus,
};

use crate::Miner;

impl Miner {
    // 这段代码主要功能是从多个地址获取账户数据并尝试将其转换为Bus对象，然后打印出每个Bus的ID和对应的奖励值。
    // 通过使用异步编程，代码能够高校地处理多个请求
    pub async fn busses(&self) {
        // 克隆RPC客户端，以便在异步环境中使用
        let client = self.rpc_client.clone();
        // 遍历预定义的Bus地址列表
        for address in BUS_ADDRESSES.iter() {
            // 异步获取指定地址的账户数据，使用 unwrap() 来处理可能的错误
            let data = client.get_account_data(address).await.unwrap();
            // 尝试将获取的字节数据转化为Bus类型
            match Bus::try_from_bytes(&data) {
                // 如果转换成功
                Ok(bus) => {
                    // 将奖励地址转化为浮点数，并根据TOKEN_DECIMALS 调整小数点位数
                    let rewards = (bus.rewards as f64) / 10f64.powf(TOKEN_DECIMALS as f64);
                    // 打印出公交ID和对应的奖励值
                    println!("Bus {}: {:} ORE", bus.id, rewards);
                }
                // 如果转换失败，捕获错误但不做任何处理
                Err(_) => {}
            }
        }
    }
}