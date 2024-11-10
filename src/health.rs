use solana_sdk::{signature::Signer, transaction::Transaction};

use crate::Miner;
impl Miner {
    pub async fn health(&self) {
        let blockhash = self.rpc_client.get_latest_blockhash().await.unwrap();  // 获取最新的区块哈希
        let ix = luckycoin_api::sdk::health(self.signer().pubkey()); // 创建初始化指令

        let transaction = Transaction::new_signed_with_payer(
            &[ix],
            Some(&self.signer().pubkey()),  // 交易的支付者
            &[&self.signer()],  // 签名者
            blockhash,
        );
        match self.rpc_client.send_and_confirm_transaction(&transaction).await {
            Ok(_) => {
                println!("Transaction sent, health status updated!!!!");
            }
            Err(e) => {
                eprintln!("Failed to send transaction: {:?}", e); // 打印错误信息
            }
        }
    }
}