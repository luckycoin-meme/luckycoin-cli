use std::str::FromStr;

use ore_api::consts::TOKEN_DECIMALS;
use solana_program::pubkey::Pubkey;
use solana_sdk::signature::Signer;
use spl_token::amount_to_ui_amount;

use crate::{
    args::ProofArgs,
    utils::{get_proof, proof_pubkey},
    Miner,
};

impl Miner {
    // 用于获取并显示矿工的证明信息
    pub async fn proof(&self, args: ProofArgs) {
        // 获取签名者(矿工的公钥)
        let signer = self.signer();
        // 根据提供的地址参数决定使用哪个地址
        let address = if let Some(address) = args.address {
            // 如果提供了地址，则将其转换为Pubkey格式
            Pubkey::from_str(&address).unwrap()
        } else {
            // 如果没有提供地址，则使用默认的证明公钥
            proof_pubkey(signer.pubkey())
        };
        // 从 RPC 客户端获取指定地址的证明信息
        let proof = get_proof(&self.rpc_client, address).await;
        // 打印地址信息
        println!("Address: {:?}", address);
        // 打印授权信息
        println!("Authority: {:?}", proof.authority);
        // 打印余额信息，转换为可读格式
        println!(
            "Balance: {:?} ORE",
            amount_to_ui_amount(proof.balance, TOKEN_DECIMALS)
        );
        // 打印最后的哈希值
        println!(
            "Last hash: {}",
            solana_sdk::hash::Hash::new_from_array(proof.last_hash).to_string()
        );
        // 打印最后哈希的时间戳
        println!("Last hash at: {:?}", proof.last_hash_at);
        // 打印最后质押的时间戳
        println!("Last stake at: {:?}", proof.last_stake_at);
        // 打印矿工信息
        println!("Miner: {:?}", proof.miner);
        // 打印总哈希次数
        println!("Total hashes: {:?}", proof.total_hashes);
        // 打印总奖励，转换为可读格式
        println!(
            "Total rewards: {:?} ORE",
            amount_to_ui_amount(proof.total_rewards, TOKEN_DECIMALS)
        );
    }
}
