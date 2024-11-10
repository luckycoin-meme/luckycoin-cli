use solana_sdk::signature::Signer;

use crate::{send_and_confirm::ComputeBudget, utils::proof_pubkey, Miner};

impl Miner {
    pub async fn open(&self) {
        // 如果矿工已经注册，则提前返回
        let signer = self.signer(); // 获取矿工的签名者
        let fee_payer = self.fee_payer(); // 获取交易的费用支付者
        let proof_address = proof_pubkey(signer.pubkey()); // 从签名者的公钥派生出证明地址

        // 检查证明地址的账户是否已经存在
        if self.rpc_client.get_account(&proof_address).await.is_ok() {
            println!("检查证明地址的账户是否已经存在");
            return; // 如果存在，提前退出，因为矿工已经注册
        }

        // 如果尚未注册，继续生成挑战
        println!("正在生成挑战...");

        // 创建一个用于开启矿工的交易指令
        let ix = luckycoin_api::sdk::open(signer.pubkey(), signer.pubkey(), fee_payer.pubkey());

        // 发送交易并确认
        self.send_and_confirm(&[ix], ComputeBudget::Fixed(400_000), false)
            .await // 等待交易完成
            .ok(); // 忽略发送过程中发生的任何错误
    }
}