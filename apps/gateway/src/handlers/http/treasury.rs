//! Treasury HTTP Handlers
//!
//! Provides treasury overview and transfer operations.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use gateway::error::GatewayError;
use gateway::middleware::{require_any_role, AuthUser};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::AppState;

/// Treasury overview response
#[derive(Debug, Serialize)]
pub struct TreasuryResponse {
    pub total_balance: String,
    pub token_symbol: String,
    pub assets: Vec<AssetResponse>,
    pub recent_transactions: Vec<TransactionResponse>,
}

#[derive(Debug, Serialize)]
pub struct AssetResponse {
    pub token: String,
    pub balance: String,
    pub value_usd: f64,
}

#[derive(Debug, Serialize)]
pub struct TransactionResponse {
    pub id: String,
    pub tx_type: String,
    pub amount: String,
    pub token: String,
    pub from: String,
    pub to: String,
    pub timestamp: String,
    pub status: String,
}

/// Get treasury info
pub async fn get_treasury(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
) -> Result<Json<TreasuryResponse>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let wallet_service = state.wallet_service.as_ref().ok_or_else(|| {
        GatewayError::service_unavailable("wallet", "Wallet service not initialized")
    })?;

    let info = wallet_service
        .get_wallet_info()
        .await
        .map_err(|e| GatewayError::agent(format!("Failed to get wallet info: {}", e)))?;

    let balance_str = info.balance.to_string();
    let address = format!("0x{}", hex::encode(info.address.as_slice()));

    // Query recent transfers from database (if any are tracked)
    let txs: Vec<TransferRow> = sqlx::query_as(
        "SELECT tx_hash, to_address, amount, status, created_at
         FROM chain_transactions WHERE tx_type = 'transfer' ORDER BY created_at DESC LIMIT 10",
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let transactions = txs
        .into_iter()
        .map(|tx| TransactionResponse {
            id: tx.tx_hash,
            tx_type: "transfer".to_string(),
            amount: tx.amount,
            token: "ETH".to_string(),
            from: address.clone(),
            to: tx.to_address,
            timestamp: tx.created_at,
            status: tx.status,
        })
        .collect();

    Ok(Json(TreasuryResponse {
        total_balance: balance_str.clone(),
        token_symbol: "ETH".to_string(),
        assets: vec![AssetResponse {
            token: "ETH".to_string(),
            balance: balance_str,
            value_usd: 0.0,
        }],
        recent_transactions: transactions,
    }))
}

/// Transfer request
#[derive(Debug, Deserialize)]
pub struct TransferRequest {
    pub to: String,
    pub amount: String,
    #[serde(default)]
    pub token: Option<String>,
}

/// Transfer tokens from treasury
pub async fn transfer(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(req): Json<TransferRequest>,
) -> Result<impl IntoResponse, GatewayError> {
    require_any_role(&user, &["admin"])?;

    let wallet_service = state.wallet_service.as_ref().ok_or_else(|| {
        GatewayError::service_unavailable("wallet", "Wallet service not initialized")
    })?;

    let to_address = parse_address(&req.to)?;
    let amount = req
        .amount
        .parse::<u128>()
        .map_err(|_| GatewayError::bad_request("Invalid amount"))?;

    let tx_hash = wallet_service
        .transfer(
            to_address.into(),
            beebotos_chain::compat::U256::from(amount),
        )
        .await
        .map_err(|e| GatewayError::agent(format!("Transfer failed: {}", e)))?;

    let tx_hash_hex = format!("0x{}", hex::encode(tx_hash.as_slice()));

    // Record in database
    let _ = sqlx::query(
        "INSERT INTO chain_transactions (tx_hash, tx_type, to_address, amount, status, created_at)
         VALUES (?1, 'transfer', ?2, ?3, 'pending', datetime('now'))",
    )
    .bind(&tx_hash_hex)
    .bind(&req.to)
    .bind(&req.amount)
    .execute(&state.db)
    .await;

    Ok((
        StatusCode::OK,
        Json(json!({
            "tx_hash": tx_hash_hex,
            "status": "submitted",
            "message": "Transfer submitted",
        })),
    ))
}

fn parse_address(addr: &str) -> Result<[u8; 20], GatewayError> {
    let addr = addr.strip_prefix("0x").unwrap_or(addr);
    let bytes =
        hex::decode(addr).map_err(|_| GatewayError::bad_request("Invalid address format"))?;
    if bytes.len() != 20 {
        return Err(GatewayError::bad_request("Address must be 20 bytes"));
    }
    let mut arr = [0u8; 20];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

#[derive(sqlx::FromRow)]
struct TransferRow {
    tx_hash: String,
    to_address: String,
    amount: String,
    status: String,
    created_at: String,
}
