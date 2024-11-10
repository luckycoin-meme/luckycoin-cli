use std::str::FromStr;

use colored::*;
use ore_api::consts::MINT_ADDRESS;
use solana_program::pubkey::Pubkey;
use solana_sdk::signature::Signer;
use spl_token::amount_to_ui_amount;

use crate::{
    args::TransferArgs,
    cu_limits::CU_LIMIT_CLAIM,
    send_and_confirm::ComputeBudget,
    utils::{amount_f64_to_u64, ask_confirm},
    Miner,
};

impl Miner {
    // 用于转账操作
    pub async fn transfer(&self, args: TransferArgs) {
        // 获取签名者(矿工公钥)
        let signer = self.signer();
        let pubkey = signer.pubkey(); // 获取签名者公钥
        // 计算发送者的代币账户地址
        let sender_tokens =
            spl_associated_token_account::get_associated_token_address(&pubkey, &MINT_ADDRESS);
        let mut ixs = vec![]; // 存储交易指令的向量

        // 初始化接受者的地址
        let to = Pubkey::from_str(&args.to).expect("Failed to parse recipient wallet address");
        // 计算接受者的代币账户地址
        let recipient_tokens =
            spl_associated_token_account::get_associated_token_address(&to, &MINT_ADDRESS);
        // 检查接受者的代币账户是否存在
        if self
            .rpc_client
            .get_token_account(&recipient_tokens)
            .await
            .is_err()
        {
            // 如果接受者的代币账户不存在，添加创建代币账户的指令
            ixs.push(
                spl_associated_token_account::instruction::create_associated_token_account(
                    &signer.pubkey(), // 账户创建者的公钥
                    &to, // 接受者的公钥
                    &ore_api::consts::MINT_ADDRESS, // 代币的铸造地址
                    &spl_token::id(), // SPL 代币的 ID
                ),
            );
        }

        // 解析转账金额
        let amount = amount_f64_to_u64(args.amount);

        // 确认用户是否继续转账
        if !ask_confirm(
            format!(
                "\nYou are about to transfer {}.\n\nAre you sure you want to continue? [Y/n]",
                format!(
                    "{} ORE", // 格式化金额的可读形式
                    amount_to_ui_amount(amount, ore_api::consts::TOKEN_DECIMALS)
                )
                    .bold(), // 突出显示金额
            )
                .as_str(),
        ) {
            return; //如果用于选择不继续，则返回
        }

        // 创建转账指令并将其添加到指令向量中
        ixs.push(
            spl_token::instruction::transfer(
                &spl_token::id(), //SPL代币的ID
                &sender_tokens, // 发送者的代币账户
                &recipient_tokens, //接受者代币账户
                &pubkey, // 签名者的公钥
                &[&pubkey], // 签名者的公钥，用于授权
                amount, //转账金额
            )
                .unwrap(), // 确保指令创建成功
        );
        // 发送并确认交易
        self.send_and_confirm(&ixs, ComputeBudget::Fixed(CU_LIMIT_CLAIM), false)
            .await
            .ok(); // 忽略可能的错误
    }
}
