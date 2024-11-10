use std::time::Duration;

use cached::proc_macro::cached; // 引入缓存宏
use luckycoin_api::consts::PROOF; // 引入常量 PROOF
use solana_client::client_error::{ClientError, ClientErrorKind}; // 引入 Solana 客户端错误类型
use solana_client::nonblocking::rpc_client::RpcClient; // 引入非阻塞的 RPC 客户端
use solana_program::pubkey::Pubkey; // 引入公钥类型
use solana_sdk::hash::Hash; // 引入哈希类型
use tokio::time::sleep; // 引入异步睡眠功能

// 最大重试次数和查询延迟
pub const BLOCKHASH_QUERY_RETRIES: usize = 5; // 查询最新区块哈希的最大重试次数
pub const BLOCKHASH_QUERY_DELAY: u64 = 500; // 查询延迟，单位为毫秒

/// 计算并缓存给定 authority 的证明公钥
#[cached]
pub fn proof_pubkey(authority: Pubkey) -> Pubkey {
    // 根据 authority 计算程序地址
    Pubkey::find_program_address(&[PROOF, authority.as_ref()], &luckycoin_api::ID).0
}

/// 异步获取最新区块哈希，带重试机制
pub async fn get_latest_blockhash_with_retries(
    client: &RpcClient, // RPC 客户端
) -> Result<(Hash, u64), ClientError> { // 返回哈希和槽号，或返回客户端错误
    let mut attempts = 0; // 记录尝试次数

    loop {
        // 尝试获取最新的区块哈希
        if let Ok((hash, slot)) = client
            .get_latest_blockhash_with_commitment(client.commitment()) // 使用当前承诺级别获取区块哈希
            .await
        {
            return Ok((hash, slot)); // 成功获取，返回结果
        }

        // 如果获取失败，进行重试
        sleep(Duration::from_millis(BLOCKHASH_QUERY_DELAY)).await; // 等待指定的延迟
        attempts += 1; // 增加尝试次数

        // 如果达到最大重试次数，返回错误
        if attempts >= BLOCKHASH_QUERY_RETRIES {
            return Err(ClientError {
                request: None,
                kind: ClientErrorKind::Custom(
                    "Max retries reached for latest blockhash query".into(), // 自定义错误信息
                ),
            });
        }
    }
}