use colored::*;
use solana_sdk::{pubkey::Pubkey, signer::Signer};
use spl_token::amount_to_ui_amount;

use crate::{
    cu_limits::CU_LIMIT_UPGRADE,
    send_and_confirm::ComputeBudget,
    utils::{amount_f64_to_u64_v1, ask_confirm},
    Miner, UpgradeArgs,
};

impl Miner {
    // 用于升级矿工的代币账户
    pub async fn upgrade(&self, args: UpgradeArgs) {
        // 获取签名者(矿工的公钥)
        let signer = &self.signer();
        // 获取或初始化关联代币账户（ATA）
        let beneficiary = self.get_or_initialize_ata().await;
        // 获取 v1 代币账户地址和余额
        let (sender, sender_balance) = self.get_ata_v1().await;
        // 解析要升级的金额
        let amount_f64 = match args.amount {
            Some(f64) => f64, // 如果提供了金额，则使用改金额
            None => {
                // 如果没有提供金额，则默认使用v1代币账户中的最大金额
                println!(
                    "Defaulting to max amount of v1 Ore token in wallet: {}",
                    sender_balance
                );
                sender_balance
            }
        };
        // 将浮点金额转换为无符号整数
        let amount = amount_f64_to_u64_v1(amount_f64);
        // 将金额转换为可读格式
        let amount_ui = amount_to_ui_amount(amount, ore_api::consts::TOKEN_DECIMALS_V1);
        // 确认用户是否继续升级
        if !ask_confirm(
            format!(
                "\n You are about to upgrade {}. \n\nAre you sure you want to continue? [Y/n]",
                format!("{} ORE", amount_ui).bold(), // 突出显示余额
            )
                .as_str(),
        ) {
            return; // 如果用户选择不继续，则返回
        }
        // 创建升级指令
        let ix = ore_api::instruction::upgrade(signer.pubkey(), beneficiary, sender, amount);
        // 发送并确认交易
        match self
            .send_and_confirm(&[ix], ComputeBudget::Fixed(CU_LIMIT_UPGRADE), false)
            .await
        {
            Ok(_sig) => {} //如果发送成功，什么都不做
            Err(err) => {
                // 如果发送失败，打印错误信息
                println!("error: {}", err);
            }
        }
    }

    // 确保代币账户存在并获取余额
    async fn get_ata_v1(&self) -> (Pubkey, f64) {
        // 获取签名者
        let signer = self.signer();
        // 克隆RPC客户端
        let client = self.rpc_client.clone();

        // 计算 v1 代币账户的关联地址
        let token_account_pubkey_v1 = spl_associated_token_account::get_associated_token_address(
            &signer.pubkey(),
            &ore_api::consts::MINT_V1_ADDRESS,
        );

        // 获取代币中户余额
        let balance = match client.get_token_account(&token_account_pubkey_v1).await {
            Ok(None) => {
                // 如果代币账户不存在，抛出异常
                panic!("v1 token account doesn't exist")
            }
            Ok(Some(token_account)) => match token_account.token_amount.ui_amount {
                Some(ui_amount) => ui_amount, //返回可读余额
                None => {
                    // 如果解析失败，抛出异常
                    panic!(
                        "Error parsing token account UI amount: {}",
                        token_account.token_amount.amount
                    )
                }
            },
            Err(err) => {
                // 如果获取失败，抛出异常
                panic!("Error fetching token account: {}", err)
            }
        };

        // 返回v1代币账户地址和余额
        (token_account_pubkey_v1, balance)
    }

    // 确保关联代币账户存在，如果不存在则初始化
    async fn get_or_initialize_ata(&self) -> Pubkey {
        // 获取签名者
        let signer = self.signer();
        // 克隆RPC客户端
        let client = self.rpc_client.clone();

        // 计算关联代币账户的地址
        let token_account_pubkey = spl_associated_token_account::get_associated_token_address(
            &signer.pubkey(),
            &ore_api::consts::MINT_ADDRESS,
        );

        // 检查关联代币账户是否存在，如果不存在则初始化
        if let Err(_err) = client.get_token_account(&token_account_pubkey).await {
            println!("Initializing v2 token account...");
            // 创建关联代币账户的指令
            let ix = spl_associated_token_account::instruction::create_associated_token_account(
                &signer.pubkey(), //账户创建者公钥
                &signer.pubkey(), // 接收者的公钥
                &ore_api::consts::MINT_ADDRESS, // 代币的铸造地址
                &spl_token::id(),
            );
            // 发送并确认创建账户的交易
            self.send_and_confirm(&[ix], ComputeBudget::Fixed(500_000), false)
                .await
                .ok();
        }

        // 返回关联代币账户的地址
        token_account_pubkey
    }
}
