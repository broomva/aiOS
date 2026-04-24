//! Per-tenant usage & invoicing DTOs (served by lagod via lago-billing).

use crate::ids::SessionId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct TenantId(pub String);

#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct UsageRecord {
    pub tenant: TenantId,
    pub session: SessionId,
    pub unit: UsageUnit,
    pub quantity: u64,
    pub recorded_at: chrono::DateTime<chrono::Utc>,
    #[serde(default)]
    pub attributes: serde_json::Value,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum UsageUnit {
    CpuMs,
    MemKb,
    EgressBytes,
    Tokens,
    ToolCalls,
    Custom,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct BillingPeriod {
    pub start: chrono::DateTime<chrono::Utc>,
    pub end: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Invoice {
    pub tenant: TenantId,
    pub period: BillingPeriod,
    pub line_items: Vec<InvoiceLine>,
    pub subtotal: f64,
    pub currency: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct InvoiceLine {
    pub unit: UsageUnit,
    pub quantity: u64,
    pub unit_price: f64,
    pub extended_price: f64,
}
