use std::str::FromStr;

use solana_program::pubkey::Pubkey;
use solana_sdk::signature::Signer;

use crate::{
    args::BalanceArgs,
    utils::{amount_u64_to_string, get_proof_with_authority},
    Miner,
};

impl Miner {
    // Query miner token balance
    pub async fn balance(&self, args: BalanceArgs) {
        // 获取当前签名者信息
        let signer = self.signer();
        // 根据提供的地址或签名者的公钥确定查询的地址
        let address = if let Some(address) = args.address {
            if let Ok(address) = Pubkey::from_str(&address) {
                address
            } else {
                // 如果地址无效，则打印错误信息并退出函数
                println!("Invalid address: {:?}", address);
                return;
            }
        } else {
            // 如果没有提供地址，则使用签名者公钥
            signer.pubkey()
        };
        println!("打印钱包地址:{}", address);
        // 通过RPC客户端获取指定地址的证明信息
        let proof = get_proof_with_authority(&self.rpc_client, address).await;

        // 获取与指定地址关联的代币账户地址
        let token_account_address = spl_associated_token_account::get_associated_token_address(
            &address,
            &ore_api::consts::MINT_ADDRESS,
        );
        // 通过RPC客户端获取代币账户的余额
        let token_balance = if let Ok(Some(token_account)) = self
            .rpc_client
            .get_token_account(&token_account_address)
            .await
        {
            // 如果获取成功，返回账户的UI显示金额
            token_account.token_amount.ui_amount_string
        } else {
            // 如果获取失败，默认余额为0
            "0".to_string()
        };
        // 打印账户余额和质押金额
        println!(
            "Balance: {} ORE\nStake: {} ORE",
            token_balance,
            amount_u64_to_string(proof.balance)
        )
    }
}
