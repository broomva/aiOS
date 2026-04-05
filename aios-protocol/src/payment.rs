//! Payment port — canonical interface for agent financial operations.
//!
//! This module defines the `PaymentPort` trait and associated types that
//! form the boundary between the Agent OS kernel and payment implementations
//! (x402, MPP, etc.). Implementations live in the Haima project.

use crate::error::KernelResult;
use crate::ids::SessionId;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// A payment authorization request from the agent runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentAuthorizationRequest {
    /// Session requesting the payment.
    pub session_id: SessionId,
    /// URL of the resource requiring payment.
    pub resource_url: String,
    /// Amount in micro-credits (1 credit = 1,000,000 micro-credits).
    pub amount_micro_credits: i64,
    /// Token symbol (e.g., "USDC").
    pub token: String,
    /// Chain identifier in CAIP-2 format (e.g., "eip155:8453").
    pub chain: String,
    /// Recipient address.
    pub recipient: String,
    /// Human-readable description.
    pub description: Option<String>,
}

/// The result of a payment authorization evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum PaymentAuthorizationDecision {
    /// Payment approved — proceed with signing and settlement.
    Approved {
        /// Micro-credits that will be deducted.
        amount_micro_credits: i64,
    },
    /// Payment requires human approval via ApprovalPort.
    RequiresApproval {
        /// Why approval is needed.
        reason: String,
    },
    /// Payment denied by policy.
    Denied {
        /// Why the payment was denied.
        reason: String,
    },
}

/// Settlement receipt from an on-chain payment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentSettlementReceipt {
    /// Transaction hash on the target chain.
    pub tx_hash: String,
    /// Chain where settlement occurred.
    pub chain: String,
    /// Amount in micro-credits.
    pub amount_micro_credits: i64,
    /// Settlement latency in milliseconds.
    pub latency_ms: u64,
}

/// Agent wallet balance information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletBalanceInfo {
    /// Wallet address.
    pub address: String,
    /// Chain identifier.
    pub chain: String,
    /// On-chain balance in micro-credits.
    pub on_chain_micro_credits: i64,
    /// Internal ledger balance in micro-credits.
    pub internal_micro_credits: i64,
}

/// The canonical payment port — the only allowed boundary between the kernel
/// and payment implementations.
///
/// Implementations:
/// - `haima-x402`: x402 protocol (Coinbase/Cloudflare)
/// - Future: MPP (Stripe/Tempo), direct fiat rails
#[async_trait]
pub trait PaymentPort: Send + Sync {
    /// Evaluate whether a payment should be authorized given current policy
    /// and economic state.
    async fn authorize(
        &self,
        request: PaymentAuthorizationRequest,
    ) -> KernelResult<PaymentAuthorizationDecision>;

    /// Execute a pre-authorized payment (sign and submit to facilitator).
    async fn settle(
        &self,
        request: PaymentAuthorizationRequest,
    ) -> KernelResult<PaymentSettlementReceipt>;

    /// Query the agent's wallet balance.
    async fn balance(&self) -> KernelResult<WalletBalanceInfo>;
}
