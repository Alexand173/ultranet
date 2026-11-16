//! What the games service reads from the chain, behind a trait so the rules can be tested without one.
//!
//! Three questions, all answered from the node's public API: is this address real, what was the
//! chain's last anchor before a given moment (a Grid day's seed), and did an anchor I submitted land.

use async_trait::async_trait;
use serde_json::Value;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum ChainError {
    #[error("the chain API is unavailable")]
    Unavailable,
    #[error("the chain API returned something unexpected")]
    Invalid,
}

/// One owner-signed anchor as the ledger reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnchorPoint {
    pub sequence: u64,
    pub root: [u8; 32],
    pub block_timestamp: u64,
    pub tx_hash: String,
}

#[async_trait]
pub trait GamesChain: Send + Sync {
    /// Whether the address has a balance or any transaction history. A fresh random address has neither.
    async fn address_active(&self, address: &str) -> Result<bool, ChainError>;

    /// The newest anchor of `chain_id` whose block timestamp is **before** `before`, if any. A chain
    /// that is not registered has no anchors, which is `Ok(None)`, not an error.
    async fn latest_anchor_before(
        &self,
        chain_id: u32,
        before: u64,
    ) -> Result<Option<AnchorPoint>, ChainError>;

    /// The anchor of `chain_id` carrying exactly this root, if one is on the ledger.
    async fn find_anchor(
        &self,
        chain_id: u32,
        root: &[u8; 32],
    ) -> Result<Option<AnchorPoint>, ChainError>;
}

const PAGE: u64 = 50;
/// How far back `find_anchor` looks: a day's anchor is among the newest few.
const FIND_PAGES: u64 = 3;

pub struct HttpChain {
    client: reqwest::Client,
    base_url: String,
}

impl HttpChain {
    pub fn new(base_url: &str) -> Result<Self, ChainError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|_| ChainError::Invalid)?;
        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
        })
    }

    /// `Ok(None)` for a 404, which for a chain means "not registered".
    async fn get(&self, path: &str) -> Result<Option<Value>, ChainError> {
        let response = self
            .client
            .get(format!("{}{}", self.base_url, path))
            .send()
            .await
            .map_err(|_| ChainError::Unavailable)?;
        match response.status().as_u16() {
            404 => Ok(None),
            200..=299 => response
                .json::<Value>()
                .await
                .map(Some)
                .map_err(|_| ChainError::Invalid),
            500..=599 => Err(ChainError::Unavailable),
            _ => Err(ChainError::Invalid),
        }
    }

    async fn anchor_count(&self, chain_id: u32) -> Result<Option<u64>, ChainError> {
        Ok(self
            .get(&format!("/api/appchain/ledger/{chain_id}"))
            .await?
            .and_then(|body| body["chain"]["anchor_count"].as_u64()))
    }

    async fn anchors(&self, chain_id: u32, from: u64) -> Result<Vec<AnchorPoint>, ChainError> {
        let body = self
            .get(&format!(
                "/api/appchain/ledger/{chain_id}/anchors?from={from}&limit={PAGE}"
            ))
            .await?
            .ok_or(ChainError::Invalid)?;
        parse_anchors(&body)
    }
}

pub fn parse_anchors(body: &Value) -> Result<Vec<AnchorPoint>, ChainError> {
    let list = body["anchors"].as_array().ok_or(ChainError::Invalid)?;
    let mut points = Vec::with_capacity(list.len());
    for row in list {
        let root_hex = row["state_root"].as_str().ok_or(ChainError::Invalid)?;
        let root: [u8; 32] = hex::decode(root_hex)
            .ok()
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or(ChainError::Invalid)?;
        points.push(AnchorPoint {
            sequence: row["sequence"].as_u64().ok_or(ChainError::Invalid)?,
            root,
            block_timestamp: row["block_timestamp"].as_u64().ok_or(ChainError::Invalid)?,
            tx_hash: row["tx_hash"].as_str().unwrap_or_default().to_string(),
        });
    }
    points.sort_by_key(|point| point.sequence);
    Ok(points)
}

#[async_trait]
impl GamesChain for HttpChain {
    async fn address_active(&self, address: &str) -> Result<bool, ChainError> {
        let body = self
            .get(&format!("/api/account/{address}"))
            .await?
            .ok_or(ChainError::Invalid)?;
        let data = &body["data"];
        let balance = data["balance_base_units"]
            .as_u64()
            .or_else(|| data["balance"].as_u64());
        let nonce = data["nonce"].as_u64();
        match (balance, nonce) {
            (None, None) => Err(ChainError::Invalid),
            (balance, nonce) => Ok(balance.unwrap_or(0) > 0 || nonce.unwrap_or(0) > 0),
        }
    }

    async fn latest_anchor_before(
        &self,
        chain_id: u32,
        before: u64,
    ) -> Result<Option<AnchorPoint>, ChainError> {
        let Some(count) = self.anchor_count(chain_id).await? else {
            return Ok(None);
        };
        // Walk back from the newest anchors in pages until one predates `before`.
        let mut from = count.saturating_sub(PAGE - 1).max(1);
        loop {
            let page = if count == 0 {
                Vec::new()
            } else {
                self.anchors(chain_id, from).await?
            };
            if let Some(found) = page
                .iter()
                .rev()
                .find(|point| point.block_timestamp < before)
            {
                return Ok(Some(found.clone()));
            }
            if from <= 1 {
                return Ok(None);
            }
            from = from.saturating_sub(PAGE).max(1);
        }
    }

    async fn find_anchor(
        &self,
        chain_id: u32,
        root: &[u8; 32],
    ) -> Result<Option<AnchorPoint>, ChainError> {
        let Some(count) = self.anchor_count(chain_id).await? else {
            return Ok(None);
        };
        if count == 0 {
            return Ok(None);
        }
        let mut from = count.saturating_sub(PAGE - 1).max(1);
        for _ in 0..FIND_PAGES {
            let page = self.anchors(chain_id, from).await?;
            if let Some(found) = page.iter().rev().find(|point| &point.root == root) {
                return Ok(Some(found.clone()));
            }
            if from <= 1 {
                break;
            }
            from = from.saturating_sub(PAGE).max(1);
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn anchors_parse_in_sequence_order_with_their_roots() {
        let body = json!({"anchors": [
            {"sequence": 2, "state_root": "bb".repeat(32), "block_timestamp": 200, "tx_hash": "t2"},
            {"sequence": 1, "state_root": "aa".repeat(32), "block_timestamp": 100, "tx_hash": "t1"},
        ]});
        let points = parse_anchors(&body).unwrap();
        assert_eq!(
            points.iter().map(|p| p.sequence).collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(points[0].root, [0xAA; 32]);
        assert_eq!(points[1].block_timestamp, 200);
    }

    #[test]
    fn a_malformed_anchor_is_refused_not_guessed() {
        for bad in [
            json!({"anchors": [{"sequence": 1, "state_root": "zz", "block_timestamp": 1}]}),
            json!({"anchors": [{"sequence": 1, "state_root": "aa".repeat(31), "block_timestamp": 1}]}),
            json!({"anchors": [{"state_root": "aa".repeat(32), "block_timestamp": 1}]}),
            json!({"nope": []}),
        ] {
            assert_eq!(parse_anchors(&bad), Err(ChainError::Invalid), "{bad}");
        }
    }
}
