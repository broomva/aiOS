//! Finance DTOs — wire-facing types served by haimad.
//!
//! These types are the **HTTP API surface** for the finance subsystem:
//! wallet operations, payment authorization, settlement, transaction history,
//! and usage reporting (x402, insurance, outcome billing).
//!
//! They are intentionally leaner than `haima-core`'s internal projection
//! types. `haima-core` carries rich internal state (`WalletAddress` with
//! per-chain structs, micro-credit accounting, `PaymentDecision` with
//! `WalletAddress` payloads, etc.). The types here are what callers of
//! `haimad` endpoints receive over the wire, and what `haima-api-schema`
//! re-exports so `life-kernel-facade` depends only on `aios-protocol`.

use crate::ids::{AgentId, SessionId};
use serde::{Deserialize, Serialize};

/// Wire-facing wallet manifest returned by `GET /wallet/{owner}` on haimad.
///
/// Consumers that do not recognize a chain string should treat it as opaque
/// (additive-only schema — `#[non_exhaustive]`).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct WalletManifest {
    /// Session that owns this wallet.
    pub owner: SessionId,
    /// On-chain address string (hex for EVM, base58 for Solana).
    pub address: String,
    /// Chain identifier in CAIP-2 format (e.g. `"eip155:8453"` for Base).
    pub chain: String,
    /// Current balance in the token's smallest denomination (wei / lamports).
    pub balance_wei: u128,
    /// Active payment policy governing this wallet.
    pub policy: WalletPolicy,
}

/// Payment policy thresholds (all amounts in wei / smallest token unit).
///
/// `None` means "no limit" for that dimension.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub struct WalletPolicy {
    /// Payments below this amount are auto-approved without human review.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_approve_under: Option<u128>,
    /// Payments at or above this amount require explicit human approval.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub require_approval_over: Option<u128>,
    /// Hard cap — payments above this amount are always denied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deny_over: Option<u128>,
}

/// Request to authorize an outbound payment.
///
/// Sent by callers (Arcan, life-kernel-facade) before committing any
/// on-chain transaction. haimad evaluates the request against the active
/// `WalletPolicy` and returns a `PaymentAuthorization` or an error.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct PaymentAuthRequest {
    /// Session initiating the payment.
    pub from: SessionId,
    /// Recipient on-chain address.
    pub to_address: String,
    /// Amount in the token's smallest denomination (wei / lamports).
    pub amount_wei: u128,
    /// Token symbol (e.g. `"USDC"`).
    pub currency: String,
    /// Optional compute-resource cost hint from the kernel budget gate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_hint: Option<crate::budget::ResourceBudget>,
    /// Human-readable reason for this payment (audit trail).
    pub reason: String,
}

/// Authorization issued by haimad after a successful `PaymentAuthRequest`.
///
/// The authorization is time-limited (`expires_at`) and optionally carries
/// a wallet signature that the x402 facilitator can verify on settlement.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct PaymentAuthorization {
    /// Stable ID for this authorization (used to correlate with settlement).
    pub auth_id: String,
    /// Originating session.
    pub from: SessionId,
    /// Recipient on-chain address.
    pub to_address: String,
    /// Amount in the token's smallest denomination.
    pub amount_wei: u128,
    /// Token symbol (e.g. `"USDC"`).
    pub currency: String,
    /// Authorization expiry — callers MUST NOT settle after this time.
    pub expires_at: chrono::DateTime<chrono::Utc>,
    /// Optional wallet signature (EIP-3009 / EIP-712 signed payload).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

/// Receipt produced after on-chain settlement of a `PaymentAuthorization`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SettlementReceipt {
    /// Authorization that was settled.
    pub auth_id: String,
    /// On-chain transaction hash (None if not yet confirmed or unavailable).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tx_hash: Option<String>,
    /// When settlement was finalized.
    pub settled_at: chrono::DateTime<chrono::Utc>,
    /// Gross amount transferred (wei).
    pub gross_wei: u128,
    /// Net amount received by recipient after fees (wei).
    pub net_wei: u128,
    /// Protocol / facilitator fee retained (wei).
    pub fee_wei: u128,
}

/// A single on-chain or off-chain transaction record.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TransactionRecord {
    /// On-chain transaction hash (or internal ID for off-chain records).
    pub tx_hash: String,
    /// Authorization that produced this transaction, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_id: Option<String>,
    /// Originating session.
    pub from: SessionId,
    /// Recipient on-chain address.
    pub to_address: String,
    /// Amount in the token's smallest denomination (wei).
    pub amount_wei: u128,
    /// Token symbol.
    pub currency: String,
    /// When the transaction was submitted (or recorded for off-chain).
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Current lifecycle status.
    pub status: TransactionStatus,
}

/// Lifecycle status of a `TransactionRecord`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TransactionStatus {
    /// Submitted to the network but not yet included in a block.
    Pending,
    /// Included in a block with sufficient confirmations.
    Confirmed,
    /// Rejected by the network (gas, nonce, etc.).
    Failed,
    /// Included but execution reverted (EVM).
    Reverted,
}

/// Filter for `FinancePort::list_transactions`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TransactionFilter {
    /// Narrow to a specific lifecycle status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<TransactionStatus>,
    /// Include only transactions at or after this timestamp.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<chrono::DateTime<chrono::Utc>>,
    /// Include only transactions strictly before this timestamp.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<chrono::DateTime<chrono::Utc>>,
    /// Maximum number of records to return.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

/// A closed time window used for usage reporting.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TimeWindow {
    /// Inclusive start of the window.
    pub start: chrono::DateTime<chrono::Utc>,
    /// Exclusive end of the window.
    pub end: chrono::DateTime<chrono::Utc>,
}

/// Aggregated spend report for a session over a `TimeWindow`.
///
/// Returned by `FinancePort::get_usage_report` and exposed by haimad at
/// `GET /usage/{owner}?start=…&end=…`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct UsageReport {
    /// Session this report covers.
    pub owner: SessionId,
    /// Time window the report covers.
    pub window: TimeWindow,
    /// Total wei spent (all transactions in the window).
    pub total_wei_spent: u128,
    /// Number of transactions in the window.
    pub transactions: u32,
    /// Per-agent breakdown of spend (wei), sorted by agent ID.
    pub by_agent: std::collections::BTreeMap<AgentId, u128>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::SessionId;

    #[test]
    fn wallet_policy_default_all_none() {
        let p = WalletPolicy::default();
        assert!(p.auto_approve_under.is_none());
        assert!(p.require_approval_over.is_none());
        assert!(p.deny_over.is_none());
    }

    #[test]
    fn wallet_policy_default_omits_fields() {
        let p = WalletPolicy::default();
        let json = serde_json::to_string(&p).unwrap();
        assert_eq!(json, "{}");
    }

    #[test]
    fn wallet_manifest_roundtrip() {
        let m = WalletManifest {
            owner: SessionId::from_string("sess-1"),
            address: "0xdeadbeef".into(),
            chain: "eip155:8453".into(),
            balance_wei: 1_000_000,
            policy: WalletPolicy {
                auto_approve_under: Some(100),
                require_approval_over: Some(1_000),
                deny_over: Some(1_000_000),
            },
        };
        let json = serde_json::to_string(&m).unwrap();
        let back: WalletManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.address, "0xdeadbeef");
        assert_eq!(back.balance_wei, 1_000_000);
        assert_eq!(back.policy.auto_approve_under, Some(100));
    }

    #[test]
    fn payment_auth_request_roundtrip() {
        let req = PaymentAuthRequest {
            from: SessionId::from_string("sess-1"),
            to_address: "0xrecipient".into(),
            amount_wei: 500_000,
            currency: "USDC".into(),
            cost_hint: None,
            reason: "tool execution".into(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: PaymentAuthRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.amount_wei, 500_000);
        assert_eq!(back.currency, "USDC");
        assert!(back.cost_hint.is_none());
    }

    #[test]
    fn payment_authorization_roundtrip() {
        let auth = PaymentAuthorization {
            auth_id: "auth-123".into(),
            from: SessionId::from_string("sess-1"),
            to_address: "0xrecipient".into(),
            amount_wei: 500_000,
            currency: "USDC".into(),
            expires_at: "2026-04-24T00:00:00Z".parse().unwrap(),
            signature: Some("0xsig".into()),
        };
        let json = serde_json::to_string(&auth).unwrap();
        let back: PaymentAuthorization = serde_json::from_str(&json).unwrap();
        assert_eq!(back.auth_id, "auth-123");
        assert_eq!(back.signature, Some("0xsig".into()));
    }

    #[test]
    fn settlement_receipt_roundtrip() {
        let r = SettlementReceipt {
            auth_id: "auth-123".into(),
            tx_hash: Some("0xtx".into()),
            settled_at: "2026-04-24T00:01:00Z".parse().unwrap(),
            gross_wei: 500_000,
            net_wei: 495_000,
            fee_wei: 5_000,
        };
        let json = serde_json::to_string(&r).unwrap();
        let back: SettlementReceipt = serde_json::from_str(&json).unwrap();
        assert_eq!(back.net_wei, 495_000);
        assert_eq!(back.fee_wei, 5_000);
    }

    #[test]
    fn transaction_status_serde_snake_case() {
        let s = serde_json::to_string(&TransactionStatus::Confirmed).unwrap();
        assert_eq!(s, "\"confirmed\"");
        let back: TransactionStatus = serde_json::from_str("\"pending\"").unwrap();
        assert_eq!(back, TransactionStatus::Pending);
    }

    #[test]
    fn transaction_filter_default_omits_fields() {
        let f = TransactionFilter::default();
        let json = serde_json::to_string(&f).unwrap();
        assert_eq!(json, "{}");
    }

    #[test]
    fn time_window_roundtrip() {
        let w = TimeWindow {
            start: "2026-04-01T00:00:00Z".parse().unwrap(),
            end: "2026-04-30T23:59:59Z".parse().unwrap(),
        };
        let json = serde_json::to_string(&w).unwrap();
        let back: TimeWindow = serde_json::from_str(&json).unwrap();
        assert_eq!(back.start, w.start);
        assert_eq!(back.end, w.end);
    }

    #[test]
    fn usage_report_roundtrip() {
        let mut by_agent = std::collections::BTreeMap::new();
        by_agent.insert(AgentId::from_string("agent-1"), 100_000u128);
        by_agent.insert(AgentId::from_string("agent-2"), 400_000u128);

        let report = UsageReport {
            owner: SessionId::from_string("sess-1"),
            window: TimeWindow {
                start: "2026-04-01T00:00:00Z".parse().unwrap(),
                end: "2026-04-30T23:59:59Z".parse().unwrap(),
            },
            total_wei_spent: 500_000,
            transactions: 5,
            by_agent,
        };
        let json = serde_json::to_string(&report).unwrap();
        let back: UsageReport = serde_json::from_str(&json).unwrap();
        assert_eq!(back.total_wei_spent, 500_000);
        assert_eq!(back.transactions, 5);
        assert_eq!(back.by_agent.len(), 2);
    }
}
