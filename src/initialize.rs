use luckycoin_api::consts::TREASURY_ADDRESS;
use solana_sdk::{signature::Signer, transaction::Transaction};

use crate::Miner;
impl Miner {
    /// 初始化 Miner 账户。如果已经初始化，则不会进行任何操作。
    pub async fn initialize(&self) {
        print!("初始化Miner账户!!!!!");
        // 如果程序已经初始化，提前返回
        if self.rpc_client.get_account(&TREASURY_ADDRESS).await.is_ok() {
            return;  // 账户存在，表示已经初始化
        }

        // 提交初始化交易
        let blockhash = self.rpc_client.get_latest_blockhash().await.unwrap();  // 获取最新的区块哈希
        let ix = luckycoin_api::sdk::initialize(self.signer().pubkey()); // 创建初始化指令
        // 创建一个新的交易，包含初始化指令，设置支付者为当前签名者
        let tx = Transaction::new_signed_with_payer(
            &[ix],  // 包含的指令
            Some(&self.signer().pubkey()),  // 交易的支付者
            &[&self.signer()],  // 签名者
            blockhash,  // 最新的区块哈希
        );
        print!("发送交易并确认。。。。。。");
        // 发送交易并确认
        let res = self.rpc_client.send_and_confirm_transaction(&tx).await;  // 发送交易并等待确认
        // 打印交易结果
        println!("{:?}", res);  // 输出交易的结果以便于调试
    }
}