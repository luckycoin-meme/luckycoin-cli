use std::str::FromStr;

use solana_program::pubkey::Pubkey;
use solana_sdk::signature::Signer;

use crate::{
    args::StakeArgs, cu_limits::CU_LIMIT_CLAIM, send_and_confirm::ComputeBudget,
    utils::amount_f64_to_u64, Miner,
};

impl Miner {
    // 用于进行质押操作
    pub async fn stake(&self, args: StakeArgs) {
        // 获取签名者(矿工的公钥)
        let signer = self.signer();
        // 确定发送者的代币账户地址
        let sender = match args.token_account {
            // 如果提供了代币账户地址，则将其解析为 Pubkey
            Some(address) => {
                Pubkey::from_str(&address).expect("Failed to parse token account address")
            }
            // 如果没有提供地址，则计算关联代币账户地址
            None => spl_associated_token_account::get_associated_token_address(
                &signer.pubkey(),
                &ore_api::consts::MINT_ADDRESS,
            ),
        };

        // 获取指定的代币账户
        let Ok(Some(token_account)) = self.rpc_client.get_token_account(&sender).await else {
            // 如果获取失败，打印错误信息并返回
            println!("Failed to fetch token account");
            return;
        };

        // 解析质押金额
        let amount: u64 = if let Some(amount) = args.amount {
            // 如果提供了金额，则将其从浮点数转换为无符号整数
            amount_f64_to_u64(amount)
        } else {
            // 如果没有提供金额，则从代币账户的余额中获取金额
            u64::from_str(token_account.token_amount.amount.as_str())
                .expect("Failed to parse token balance")
        };

        // 创建质押交易指令
        let ix = ore_api::instruction::stake(signer.pubkey(), sender, amount);
        // 发送并确认交易
        self.send_and_confirm(&[ix], ComputeBudget::Fixed(CU_LIMIT_CLAIM), false)
            .await
            .ok(); // 忽略可能的错误
    }
}
