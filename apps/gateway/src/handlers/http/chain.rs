//! Chain HTTP Handlers
//!
//! REST API handlers for blockchain operations including:
//! - Agent on-chain identity management
//! - DAO governance operations
//! - Skill NFT marketplace
//!
//! 🔒 P0 FIX: Chain module integration for Gateway.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use gateway::error::GatewayError;
use gateway::middleware::{require_any_role, AuthUser};
use serde::Deserialize;
use serde_json::json;

use crate::handlers::common::check_ownership;
use crate::AppState;

/// Register agent on-chain identity
pub async fn register_agent_identity(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<RegisterIdentityRequest>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    // Verify ownership
    let agent = state
        .agent_service
        .get_agent(&id)
        .await?
        .ok_or_else(|| GatewayError::not_found("Agent", &id))?;

    check_ownership(&user, &agent)?;

    let chain_service = state
        .chain_service
        .as_ref()
        .ok_or_else(|| GatewayError::service_unavailable("Chain", "Chain service not available"))?;

    let public_key_bytes = hex::decode(&req.public_key)
        .map_err(|_| GatewayError::bad_request("Invalid public key format"))?;

    if public_key_bytes.len() != 32 {
        return Err(GatewayError::bad_request("Public key must be 32 bytes"));
    }

    let mut public_key = [0u8; 32];
    public_key.copy_from_slice(&public_key_bytes);

    let tx_hash = chain_service
        .register_agent_identity(&id, &req.did, public_key)
        .await?;

    Ok(Json(json!({
        "agent_id": id,
        "did": req.did,
        "transaction_hash": tx_hash,
        "status": "pending",
        "message": "Identity registration submitted",
    })))
}

/// Get agent on-chain identity
pub async fn get_agent_identity(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    // Verify ownership
    let agent = state
        .agent_service
        .get_agent(&id)
        .await?
        .ok_or_else(|| GatewayError::not_found("Agent", &id))?;

    check_ownership(&user, &agent)?;

    let chain_service = state
        .chain_service
        .as_ref()
        .ok_or_else(|| GatewayError::service_unavailable("Chain", "Chain service not available"))?;

    let identity = chain_service.get_agent_identity(&id).await?;

    Ok(Json(json!({
        "agent_id": id,
        "on_chain_id": format!("0x{}", hex::encode(identity.agent_id)),
        "owner": format!("0x{}", hex::encode(identity.owner)),
        "did": identity.did,
        "is_active": identity.is_active,
        "reputation": identity.reputation.to_string(),
        "created_at": identity.created_at.to_string(),
    })))
}

/// Check if agent has on-chain identity
pub async fn has_agent_identity(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    let has_identity = if let Some(chain_service) = &state.chain_service {
        chain_service.has_agent_identity(&id).await
    } else {
        false
    };

    Ok(Json(json!({
        "agent_id": id,
        "has_identity": has_identity,
    })))
}

/// Create DAO proposal
pub async fn create_dao_proposal(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(req): Json<CreateProposalRequest>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let chain_service = state
        .chain_service
        .as_ref()
        .ok_or_else(|| GatewayError::service_unavailable("DAO", "DAO service not available"))?;

    let proposal_type = match req.proposal_type.as_str() {
        "funding" => beebotos_chain::dao::ProposalType::Standard,
        "upgrade" => beebotos_chain::dao::ProposalType::Emergency,
        "parameter" => beebotos_chain::dao::ProposalType::FastTrack,
        "general" => beebotos_chain::dao::ProposalType::Standard,
        _ => return Err(GatewayError::bad_request("Invalid proposal type")),
    };

    let proposal_id = chain_service
        .create_dao_proposal(req.title, req.description, proposal_type)
        .await?;

    Ok(Json(json!({
        "proposal_id": proposal_id.to_string(),
        "status": "created",
        "message": "DAO proposal created successfully",
    })))
}

/// Cast vote on DAO proposal
pub async fn cast_vote(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(proposal_id): Path<String>,
    Json(req): Json<CastVoteRequest>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let chain_service = state
        .chain_service
        .as_ref()
        .ok_or_else(|| GatewayError::service_unavailable("DAO", "DAO service not available"))?;

    let proposal_id = proposal_id
        .parse::<u64>()
        .map_err(|_| GatewayError::bad_request("Invalid proposal ID"))?;

    let vote_type = match req.vote.as_str() {
        "for" => beebotos_chain::dao::VoteType::For,
        "against" => beebotos_chain::dao::VoteType::Against,
        "abstain" => beebotos_chain::dao::VoteType::Abstain,
        _ => return Err(GatewayError::bad_request("Invalid vote type")),
    };

    let tx_hash = chain_service.cast_vote(proposal_id, vote_type).await?;

    Ok(Json(json!({
        "proposal_id": proposal_id,
        "vote": req.vote,
        "transaction_hash": tx_hash,
        "status": "submitted",
    })))
}

/// Get proposal details
pub async fn get_proposal(
    State(state): State<Arc<AppState>>,
    Path(proposal_id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    let chain_service = state
        .chain_service
        .as_ref()
        .ok_or_else(|| GatewayError::service_unavailable("DAO", "DAO service not available"))?;

    let proposal_id = proposal_id
        .parse::<u64>()
        .map_err(|_| GatewayError::bad_request("Invalid proposal ID"))?;

    let proposal = chain_service.get_proposal(proposal_id).await?;

    let status_str = format!("{:?}", proposal.status).to_lowercase();
    let votes_for = proposal.votes_for.parse::<u64>().unwrap_or(0);
    let votes_against = proposal.votes_against.parse::<u64>().unwrap_or(0);

    Ok(Json(json!({
        "id": proposal_id.to_string(),
        "title": proposal.title,
        "description": proposal.description,
        "status": status_str,
        "proposer": "",
        "created_at": "",
        "ends_at": "",
        "votes_for": votes_for,
        "votes_against": votes_against,
        "user_voted": None::<bool>,
    })))
}

/// List active DAO proposals
pub async fn list_proposals(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<serde_json::Value>>, GatewayError> {
    let chain_service = state
        .chain_service
        .as_ref()
        .ok_or_else(|| GatewayError::service_unavailable("DAO", "DAO service not available"))?;

    let proposals = chain_service.list_active_proposals().await?;

    let proposal_list: Vec<_> = proposals
        .into_iter()
        .map(|p| {
            let status_str = format!("{:?}", p.status).to_lowercase();
            let votes_for = p.votes_for.parse::<u64>().unwrap_or(0);
            let votes_against = p.votes_against.parse::<u64>().unwrap_or(0);
            json!({
                "id": p.id.to_string(),
                "title": p.title,
                "description": p.description,
                "status": status_str,
                "proposer": "",
                "created_at": "",
                "ends_at": "",
                "votes_for": votes_for,
                "votes_against": votes_against,
                "user_voted": None::<bool>,
            })
        })
        .collect();

    Ok(Json(proposal_list))
}

/// Get DAO summary
pub async fn get_dao_summary(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    let chain_service = state
        .chain_service
        .as_ref()
        .ok_or_else(|| GatewayError::service_unavailable("DAO", "DAO service not available"))?;

    let proposal_count = chain_service.get_proposal_count().await.unwrap_or(0);

    Ok(Json(json!({
        "member_count": 0u64,
        "total_voting_power": 0u64,
        "user_voting_power": 0u64,
        "active_proposals": proposal_count as u32,
        "total_proposals": proposal_count as u32,
        "token_symbol": "BEE",
        "token_balance": 0u64,
    })))
}

/// Get Chain service status
pub async fn get_chain_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    let status = if let Some(chain_service) = &state.chain_service {
        let service_status = chain_service.get_status();
        json!({
            "enabled": true,
            "chain_id": service_status.chain_id,
            "rpc_url": service_status.rpc_url,
            "has_wallet": service_status.has_wallet,
            "has_identity_registry": service_status.has_identity_registry,
            "has_dao": service_status.has_dao,
        })
    } else {
        json!({
            "enabled": false,
            "message": "Chain service not initialized",
        })
    };

    Ok(Json(status))
}

// Request/Response types

#[derive(Debug, Deserialize)]
pub struct RegisterIdentityRequest {
    pub did: String,
    pub public_key: String, // Hex-encoded 32-byte public key
}

#[derive(Debug, Deserialize)]
pub struct CreateProposalRequest {
    pub title: String,
    pub description: String,
    pub proposal_type: String, // "funding", "upgrade", "parameter", "general"
}

#[derive(Debug, Deserialize)]
pub struct CastVoteRequest {
    pub vote: String, // "for", "against", "abstain"
}
