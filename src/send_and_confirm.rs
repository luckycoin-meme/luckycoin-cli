use std::time::Duration;

use chrono::Local;
use colored::*;
use indicatif::ProgressBar;
use luckycoin_api::error::LuckycoinError;
use solana_client::{
    client_error::{ClientError, ClientErrorKind, Result as ClientResult},
    rpc_config::RpcSendTransactionConfig,
};
use solana_program::{
    instruction::Instruction,
    native_token::{lamports_to_sol, sol_to_lamports},
};
use solana_rpc_client::spinner;
use solana_sdk::{
    commitment_config::CommitmentLevel,
    compute_budget::ComputeBudgetInstruction,
    signature::{Signature, Signer},
    transaction::Transaction,
};
use solana_transaction_status::{TransactionConfirmationStatus, UiTransactionEncoding};

use crate::utils::get_latest_blockhash_with_retries;
use crate::Miner;

const MIN_SOL_BALANCE: f64 = 0.005;

const RPC_RETRIES: usize = 0;
const _SIMULATION_RETRIES: usize = 4;
const GATEWAY_RETRIES: usize = 150;
const CONFIRM_RETRIES: usize = 8;

const CONFIRM_DELAY: u64 = 500;
const GATEWAY_DELAY: u64 = 0;

pub enum ComputeBudget {
    #[allow(dead_code)]
    Dynamic,
    Fixed(u32),
}

impl Miner {
    /*
     * 用于发送并确认交易。
     */
    pub async fn send_and_confirm(&self, ixs: &[Instruction], compute_budget: ComputeBudget, skip_confirm: bool) -> ClientResult<Signature> {
        println!("开始发送并确认交易。。。。。。");
        let progress_bar = spinner::new_progress_bar();
        let signer = self.signer();
        let client = self.rpc_client.clone();
        let fee_payer = self.fee_payer();
        let mut send_client = self.rpc_client.clone();

        // 如果余额为零，则返回错误
        self.check_balance().await;

        // 创建一个空的向量，用于存储最终的指令预算指令
        let mut final_ixs = vec![];
        // 根据计算预算的类型执行不同的逻辑
        match compute_budget {
            // 如果计算预算是动态的
            ComputeBudget::Dynamic => {
                // TODO:在这里模拟交易逻辑
                todo!("simulate tx")
            }
            // 如果计算预算是固定的
            ComputeBudget::Fixed(cus) => {
                // 添加设置计算单位限制的指令到最终指令向量中
                final_ixs.push(ComputeBudgetInstruction::set_compute_unit_limit(cus))
            }
        }

        // 将设置计算单位价格的指令添加到final_ixs向量中
        final_ixs.push(ComputeBudgetInstruction::set_compute_unit_price(
            // 获取优先费用，如果未设置则默认为0
            self.priority_fee.unwrap_or(0),
        ));

        // 添加用户指令
        final_ixs.extend_from_slice(ixs);

        // 配置发送交易时的参数
        let send_cfg = RpcSendTransactionConfig {
            // 跳过预检查步骤，直接发送交易
            skip_preflight: true,
            // 设置预检确认级别为已确认
            preflight_commitment: Some(CommitmentLevel::Confirmed),
            // 设置交易编码格式为Base64
            encoding: Some(UiTransactionEncoding::Base64),
            // 设置最大重试次数
            max_retries: Some(RPC_RETRIES),
            // 设置最小上下文插槽为 None（不限制）
            min_context_slot: None,
        };
        // 根据最终指令和费用支付者的公钥创建新的交易对象
        let mut tx = Transaction::new_with_payer(&final_ixs, Some(&fee_payer.pubkey()));

        // 提交交易
        let mut attempts = 0;
        loop {
            progress_bar.set_message(format!("Submitting transaction... (attempt {})", attempts, ));
            if attempts % 10 == 0 { // 每10次尝试进行重新签名
                println!("开始尝试进行重新签名......!");
                if self.dynamic_fee { //检查是否使用动态费用

                    let fee = match self.dynamic_fee().await {
                        Ok(fee) => {
                            // 打印获取到的优先费用
                            progress_bar.println(format!("  Priority fee: {} microlamports", fee));
                            // 返回获取到的动态费用
                            fee
                        }
                        Err(err) => {
                            // 如果获取动态费用失败，使用静态费用值
                            let fee = self.priority_fee.unwrap_or(0);
                            log_warning(&progress_bar, &format!("{} Falling back to static value: {} microlamports", err, fee));
                            // 返回静态费用值
                            fee
                        }
                    };
                    // 更新计算单位价格指令
                    final_ixs.remove(1); // 移除原有计算单位的指令
                    final_ixs.insert(1, ComputeBudgetInstruction::set_compute_unit_price(fee)); // 添加新的计算单位价格指令
                    tx = Transaction::new_with_payer(&final_ixs, Some(&fee_payer.pubkey())); // 重新创建交易对象，以更新指令
                }

                // 重新签名交易
                let (hash, _slot) = get_latest_blockhash_with_retries(&client).await?;
                // 根据费用支付者的公钥决定签名
                if signer.pubkey() == fee_payer.pubkey() {
                    tx.sign(&[&signer], hash); //使用签名者进行签名
                } else {
                    tx.sign(&[&signer, &fee_payer], hash); // 同时使用签名者和费用支付者签名
                }
            }

            // 发送交易
            attempts += 1;
            match send_client.send_transaction_with_config(&tx, send_cfg).await {
                Ok(sig) => {
                    if skip_confirm { // 如果跳过确认，直接打印发送成功的消息并返回签名
                        progress_bar.finish_with_message(format!("Sent: {}", sig));
                        return Ok(sig);
                    }
                    'confirm: for _ in 0..CONFIRM_RETRIES { // 确认交易状态
                        tokio::time::sleep(Duration::from_millis(CONFIRM_DELAY)).await; //暂停指定的确认延迟时间
                        // 获取签名状态
                        println!("进入确认状态!!!!!!");
                        match client.get_signature_statuses(&[sig]).await {
                            Ok(signature_statuses) => {
                                println!("开始确认交易!!!!!!");
                                for status in signature_statuses.value {
                                    if let Some(status) = status {
                                        if let Some(err) = status.err {
                                            match err {
                                                // Instruction error
                                                solana_sdk::transaction::TransactionError::InstructionError(_, err) => {
                                                    match err {
                                                        // Custom instruction error, parse into OreError
                                                        solana_program::instruction::InstructionError::Custom(err_code) => {
                                                            match err_code {
                                                                e if e == LuckycoinError::NeedsReset as u32 => {
                                                                    attempts = 0;
                                                                    log_error(&progress_bar, "Needs reset. Retrying...", false);
                                                                    break 'confirm;
                                                                }
                                                                _ => {
                                                                    log_error(&progress_bar, &err.to_string(), true);
                                                                    return Err(ClientError {
                                                                        request: None,
                                                                        kind: ClientErrorKind::Custom(err.to_string()),
                                                                    });
                                                                }
                                                            }
                                                        }

                                                        // Non custom instruction error, return
                                                        _ => {
                                                            log_error(&progress_bar, &err.to_string(), true);
                                                            return Err(ClientError {
                                                                request: None,
                                                                kind: ClientErrorKind::Custom(err.to_string()),
                                                            });
                                                        }
                                                    }
                                                }

                                                // Non instruction error, return
                                                _ => {
                                                    log_error(&progress_bar, &err.to_string(), true);
                                                    return Err(ClientError {
                                                        request: None,
                                                        kind: ClientErrorKind::Custom(err.to_string()),
                                                    });
                                                }
                                            }
                                        } else if let Some(confirmation) =
                                            status.confirmation_status
                                        {
                                            match confirmation {
                                                TransactionConfirmationStatus::Processed => {}
                                                TransactionConfirmationStatus::Confirmed
                                                | TransactionConfirmationStatus::Finalized => {
                                                    let now = Local::now();
                                                    let formatted_time =
                                                        now.format("%Y-%m-%d %H:%M:%S").to_string();
                                                    progress_bar.println(format!(
                                                        "  Timestamp: {}",
                                                        formatted_time
                                                    ));
                                                    progress_bar.finish_with_message(format!(
                                                        "{} {}",
                                                        "OK".bold().green(),
                                                        sig
                                                    ));
                                                    return Ok(sig);
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            // Handle confirmation errors
                            Err(err) => {
                                log_error(&progress_bar, &err.kind().to_string(), false);
                            }
                        }
                    }
                }
                // Handle submit errors
                Err(err) => {
                    println!("提交交易时发生错误: {:?}", err); // 额外的错误打印
                    log_error(&progress_bar, &err.kind().to_string(), false);
                }
            }

            // Retry
            tokio::time::sleep(Duration::from_millis(GATEWAY_DELAY)).await;
            if attempts > GATEWAY_RETRIES {
                log_error(&progress_bar, "Max retries", true);
                return Err(ClientError {
                    request: None,
                    kind: ClientErrorKind::Custom("Max retries".into()),
                });
            }
        }
    }

    pub async fn check_balance(&self) {
        println!("检查余额......");
        if let Ok(balance) = self.rpc_client.get_balance(&self.fee_payer().pubkey()).await
        {
            // 打印当前余额
            println!("当前余额: {} SOL", lamports_to_sol(balance));
            if balance <= sol_to_lamports(MIN_SOL_BALANCE) {
                panic!("{} Insufficient balance: {} SOL\nPlease top up with at least {} SOL",
                       "ERROR".bold().red(), lamports_to_sol(balance), MIN_SOL_BALANCE);
            }
        } else {
            match self.rpc_client.get_balance(&self.fee_payer().pubkey()).await {
                Err(e) => println!("无法获取余额，错误信息: {:?}", e),
                _ => {}
            }
        }
    }
}

fn log_error(progress_bar: &ProgressBar, err: &str, finish: bool) {
    if finish {
        progress_bar.finish_with_message(format!("{} {}", "ERROR".bold().red(), err));
    } else {
        progress_bar.println(format!("  {} {}", "ERROR".bold().red(), err));
    }
}

fn log_warning(progress_bar: &ProgressBar, msg: &str) {
    progress_bar.println(format!("  {} {}", "WARNING".bold().yellow(), msg));
}
