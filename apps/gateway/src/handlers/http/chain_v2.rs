//! Chain HTTP Handlers (V2 - Using split services)
//!
//! 🟢 P1 FIX: Migrated to use WalletService, DaoService, and IdentityService.
//! This version separates concerns: wallet operations, DAO governance, and
//! identity.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use gateway::error::GatewayError;
use gateway::middleware::{require_any_role, AuthUser};
use serde::Deserialize;
use serde_json::json;

use crate::AppState;

// ============ Wallet Handlers ============

/// Get wallet info
pub async fn get_wallet_info(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    // 🟢 P1 FIX: Use WalletService instead of ChainService
    let wallet_service = state.wallet_service.as_ref().ok_or_else(|| {
        GatewayError::service_unavailable("wallet", "Wallet service not initialized")
    })?;

    let info = wallet_service
        .get_wallet_info()
        .await
        .map_err(|e| GatewayError::agent(format!("Failed to get wallet info: {}", e)))?;

    Ok(Json(json!({
        "address": format!("0x{}", hex::encode(info.address.as_slice())),
        "chain_id": info.chain_id,
        "balance_wei": info.balance.to_string(),
        "nonce": info.nonce,
    })))
}

/// Transfer native tokens
#[derive(Debug, Deserialize)]
pub struct TransferRequest {
    pub to: String,
    pub amount_wei: String,
}

pub async fn transfer(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(req): Json<TransferRequest>,
) -> Result<impl IntoResponse, GatewayError> {
    require_any_role(&user, &["admin"])?;

    let wallet_service = state.wallet_service.as_ref().ok_or_else(|| {
        GatewayError::service_unavailable("wallet", "Wallet service not initialized")
    })?;

    // Parse address
    let to_address = parse_address(&req.to)?;
    let amount = req
        .amount_wei
        .parse::<u128>()
        .map_err(|_| GatewayError::bad_request("Invalid amount"))?;

    let tx_hash = wallet_service
        .transfer(to_address, beebotos_chain::compat::U256::from(amount))
        .await
        .map_err(|e| GatewayError::agent(format!("Transfer failed: {}", e)))?;

    Ok((
        StatusCode::OK,
        Json(json!({
            "tx_hash": format!("0x{}", hex::encode(tx_hash.as_slice())),
            "status": "submitted",
        })),
    ))
}

// ============ Identity Handlers ============

/// Register agent identity
#[derive(Debug, Deserialize)]
pub struct RegisterIdentityRequest {
    pub agent_id: String,
    pub did: String,
    pub public_key: String,
}

pub async fn register_agent_identity(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(req): Json<RegisterIdentityRequest>,
) -> Result<impl IntoResponse, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    // 🟢 P1 FIX: Use IdentityService instead of ChainService
    let identity_service = state.identity_service.as_ref().ok_or_else(|| {
        GatewayError::service_unavailable("identity", "Identity service not initialized")
    })?;

    // Parse public key
    let public_key = parse_hex_32(&req.public_key)?;

    let request = crate::services::identity_service::RegisterIdentityRequest {
        agent_id: req.agent_id.clone(),
        did: req.did.clone(),
        public_key,
        metadata: crate::services::identity_service::AgentMetadata {
            name: req.agent_id.clone(),
            description: String::new(),
        },
    };

    let tx_hash = identity_service
        .register_identity(request)
        .await
        .map_err(|e| GatewayError::agent(format!("Identity registration failed: {}", e)))?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "tx_hash": format!("0x{}", hex::encode(tx_hash.as_slice())),
            "agent_id": req.agent_id,
            "did": req.did,
            "status": "submitted",
        })),
    ))
}

/// Get agent identity
pub async fn get_agent_identity(
    State(state): State<Arc<AppState>>,
    _user: AuthUser,
    Path(agent_id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    let identity_service = state.identity_service.as_ref().ok_or_else(|| {
        GatewayError::service_unavailable("identity", "Identity service not initialized")
    })?;

    let info = identity_service
        .get_identity(&agent_id)
        .await
        .map_err(|e| match e {
            crate::error::AppError::NotFound(_) => {
                GatewayError::not_found("agent identity", &agent_id)
            }
            _ => GatewayError::agent(format!("Failed to get identity: {}", e)),
        })?;

    Ok(Json(json!({
        "agent_id": agent_id,
        "did": info.did,
        "public_key": format!("0x{}", hex::encode(info.public_key.as_slice())),
        "is_active": info.is_active,
        "created_at": info.created_at,
    })))
}

/// Check if agent has identity
pub async fn has_agent_identity(
    State(state): State<Arc<AppState>>,
    _user: AuthUser,
    Path(agent_id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    let identity_service = state.identity_service.as_ref().ok_or_else(|| {
        GatewayError::service_unavailable("identity", "Identity service not initialized")
    })?;

    let has_identity = identity_service
        .has_identity(&agent_id)
        .await
        .map_err(|e| GatewayError::agent(format!("Failed to check identity: {}", e)))?;

    Ok(Json(json!({
        "agent_id": agent_id,
        "has_identity": has_identity,
    })))
}

// ============ DAO Handlers ============

/// Create DAO proposal
#[derive(Debug, Deserialize)]
pub struct CreateProposalRequest {
    pub title: String,
    pub description: String,
    pub proposal_type: String,
    pub voting_period_secs: u64,
}

pub async fn create_dao_proposal(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(req): Json<CreateProposalRequest>,
) -> Result<impl IntoResponse, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    // 🟢 P1 FIX: Use DaoService instead of ChainService
    let dao_service = state
        .dao_service
        .as_ref()
        .ok_or_else(|| GatewayError::service_unavailable("dao", "DAO service not initialized"))?;

    let proposal_type = match req.proposal_type.as_str() {
        "emergency" => beebotos_chain::dao::ProposalType::Emergency,
        "fast_track" => beebotos_chain::dao::ProposalType::FastTrack,
        _ => beebotos_chain::dao::ProposalType::Standard,
    };

    let request = crate::services::dao_service::CreateProposalRequest {
        title: req.title.clone(),
        description: req.description.clone(),
        proposal_type,
        action: beebotos_chain::dao::proposal::ProposalAction {
            target: beebotos_chain::compat::Address::ZERO,
            value: beebotos_chain::compat::U256::ZERO,
            data: beebotos_chain::compat::Bytes::new(),
            signature: None,
        },
        voting_period_secs: req.voting_period_secs,
    };

    let proposal_id = dao_service
        .create_proposal(request)
        .await
        .map_err(|e| GatewayError::agent(format!("Failed to create proposal: {}", e)))?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "proposal_id": format!("0x{}", hex::encode(proposal_id.to_le_bytes())),
            "title": req.title,
            "status": "created",
        })),
    ))
}

/// Cast vote on proposal
#[derive(Debug, Deserialize)]
pub struct CastVoteRequest {
    pub proposal_id: String,
    pub vote_type: String, // "for", "against", "abstain"
}

pub async fn cast_vote(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(req): Json<CastVoteRequest>,
) -> Result<impl IntoResponse, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let dao_service = state
        .dao_service
        .as_ref()
        .ok_or_else(|| GatewayError::service_unavailable("dao", "DAO service not initialized"))?;

    let proposal_id = req
        .proposal_id
        .parse::<u64>()
        .map_err(|_| GatewayError::bad_request("Invalid proposal ID"))?;

    let vote_type = match req.vote_type.as_str() {
        "for" => beebotos_chain::dao::VoteType::For,
        "against" => beebotos_chain::dao::VoteType::Against,
        "abstain" => beebotos_chain::dao::VoteType::Abstain,
        _ => return Err(GatewayError::bad_request("Invalid vote type")),
    };

    let request = crate::services::dao_service::CastVoteRequest {
        proposal_id,
        vote_type,
        voting_power: None,
    };

    let tx_hash = dao_service
        .cast_vote(request)
        .await
        .map_err(|e| GatewayError::agent(format!("Failed to cast vote: {}", e)))?;

    Ok((
        StatusCode::OK,
        Json(json!({
            "tx_hash": format!("0x{}", hex::encode(tx_hash.as_slice())),
            "status": "submitted",
        })),
    ))
}

/// Get proposal
pub async fn get_proposal(
    State(state): State<Arc<AppState>>,
    _user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    let dao_service = state
        .dao_service
        .as_ref()
        .ok_or_else(|| GatewayError::service_unavailable("dao", "DAO service not initialized"))?;

    let proposal_id = id
        .parse::<u64>()
        .map_err(|_| GatewayError::bad_request("Invalid proposal ID"))?;

    let info = dao_service
        .get_proposal(proposal_id)
        .await
        .map_err(|e| match e {
            crate::error::AppError::NotFound(_) => GatewayError::not_found("proposal", &id),
            _ => GatewayError::agent(format!("Failed to get proposal: {}", e)),
        })?;

    Ok(Json(json!({
        "proposal_id": id,
        "title": "Proposal", // TODO: Get from info
        "status": "active", // TODO: Map from info
        "votes_for": info.for_votes.to_string(),
        "votes_against": info.against_votes.to_string(),
        "executed": false, // TODO: Get from info
    })))
}

/// List proposals
pub async fn list_proposals(
    State(state): State<Arc<AppState>>,
    _user: AuthUser,
) -> Result<Json<serde_json::Value>, GatewayError> {
    let dao_service = state
        .dao_service
        .as_ref()
        .ok_or_else(|| GatewayError::service_unavailable("dao", "DAO service not initialized"))?;

    let proposals = dao_service
        .list_active_proposals()
        .await
        .map_err(|e| GatewayError::agent(format!("Failed to list proposals: {}", e)))?;

    let proposals_json: Vec<_> = proposals
        .into_iter()
        .map(|p| {
            json!({
                "proposal_id": format!("0x{}", hex::encode(p.id.to_le_bytes())),
                "status": "active", // TODO: Map from p
                "executed": false, // TODO: Get from p
            })
        })
        .collect();

    Ok(Json(json!({
        "proposals": proposals_json,
        "count": proposals_json.len(),
    })))
}

// ============ Helper Functions ============

fn parse_address(addr: &str) -> Result<beebotos_chain::compat::Address, GatewayError> {
    let addr = addr.trim();
    if addr.len() >= 42 && addr.starts_with("0x") {
        hex::decode(&addr[2..])
            .ok()
            .filter(|b| b.len() == 20)
            .map(|b| beebotos_chain::compat::Address::from_slice(&b))
            .ok_or_else(|| GatewayError::bad_request("Invalid address format"))
    } else {
        Err(GatewayError::bad_request("Address must start with 0x"))
    }
}

fn parse_hex_32(hex_str: &str) -> Result<[u8; 32], GatewayError> {
    let hex_str = hex_str.trim();
    let hex_str = if hex_str.starts_with("0x") {
        &hex_str[2..]
    } else {
        hex_str
    };

    let bytes =
        hex::decode(hex_str).map_err(|_| GatewayError::bad_request("Invalid hex format"))?;

    if bytes.len() != 32 {
        return Err(GatewayError::bad_request("Expected 32 bytes"));
    }

    let mut result = [0u8; 32];
    result.copy_from_slice(&bytes);
    Ok(result)
}

// Migration Guide:
//
// 1. Wallet Operations: Use WalletService
//    - get_wallet_info
//    - transfer
//    - get_balance
//
// 2. Identity Operations: Use IdentityService
//    - register_agent_identity
//    - get_agent_identity
//    - has_agent_identity
//
// 3. DAO Operations: Use DaoService
//    - create_proposal
//    - cast_vote
//    - get_proposal
//    - list_proposals
//
// Benefits:
// - Single responsibility per service
// - Easier testing
// - Better code organization
// - Clear separation of concerns
