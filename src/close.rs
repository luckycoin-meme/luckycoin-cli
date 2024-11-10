use colored::*;
use solana_sdk::signature::Signer;
use spl_token::amount_to_ui_amount;

use crate::{
    args::ClaimArgs,
    send_and_confirm::ComputeBudget,
    utils::{ask_confirm, get_proof_with_authority},
    Miner,
};

impl Miner {
    // 异步方法，用于关闭矿工账户
    pub async fn close(&self) {
        // 确认证明存在
        let signer = self.signer(); // 获取签名者
        let proof = get_proof_with_authority(&self.rpc_client, signer.pubkey()).await;

        // Confirm the user wants to close.
        if !ask_confirm(
            format!("{} You have {} ORE staked in this account.\nAre you sure you want to {}close this account? [Y/n]",
                    "WARNING".yellow(),
                    amount_to_ui_amount(proof.balance, ore_api::consts::TOKEN_DECIMALS),
                    if proof.balance.gt(&0) { "claim your stake and " } else { "" }
            ).as_str()
        ) {
            return;
        }

        // Claim stake
        if proof.balance.gt(&0) {
            self.claim_from_proof(ClaimArgs {
                amount: None,
                to: None,
                pool_url: None,
            })
                .await;
        }

        // Submit close transaction
        let ix = ore_api::instruction::close(signer.pubkey());
        self.send_and_confirm(&[ix], ComputeBudget::Fixed(500_000), false)
            .await
            .ok();
    }
}
