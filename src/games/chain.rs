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

/// One AppChain transaction signed by the owner, as the node's address history reports it. The history
/// carries no payload, so a transaction can be told apart by its time and status, never by its root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnerTx {
    pub hash: String,
    pub timestamp: u64,
    pub pending: bool,
}

/// What the owner's address history says about its AppChain transactions since a moment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnerActivity {
    /// The owner's AppChain transactions (recipient `appchain`) at or after `since`, newest first.
    pub transactions: Vec<OwnerTx>,
    /// The history reaches back to `since` **and** the node listed its pending transactions, so nothing
    /// in the window can be missing. When false, absence of a transaction proves nothing.
    pub complete: bool,
    /// The *confirmed* history reaches back to `since`, whatever the node did with its pending list.
    /// Confirmed transactions come from the chain itself, so with this set none is missing from the window.
    pub confirmed_complete: bool,
    /// The nonce after the newest transaction the owner has **sent** that is confirmed (`nonce + 1`), whatever
    /// its recipient, or `None` if none is visible. The list always shows the newest confirmed entries, so this is
    /// right even when the list is full. The account's own nonce counts pending transactions too, so
    /// `account nonce − this` is exactly the number of pending transactions, hidden or not.
    pub confirmed_next_nonce: Option<u64>,
}

/// One transaction as the node reports it by hash.
/// A transfer as the ledger shows it: who sent how much to whom, and whether a block has confirmed it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferView {
    pub sender: String,
    pub recipient: String,
    pub amount: u64,
    pub pending: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TxView {
    pub sender: String,
    pub recipient: String,
    pub timestamp: u64,
    pub pending: bool,
}

#[async_trait]
pub trait GamesChain: Send + Sync {
    /// The owner's AppChain transactions, confirmed and pending, since `since`. See [`OwnerActivity`].
    async fn owner_appchain_activity(
        &self,
        owner: &str,
        since: u64,
    ) -> Result<OwnerActivity, ChainError>;

    /// The account's nonce as the node reports it. **On the real node this counts pending transactions**: it is the
    /// nonce the next transaction will carry, which is why a second signer call while one is pending is accepted.
    async fn account_nonce(&self, address: &str) -> Result<u64, ChainError>;

    /// One transaction by hash, or `None` if the node has neither mined nor pending one.
    async fn transaction(&self, hash: &str) -> Result<Option<TxView>, ChainError>;

    /// Every anchor of `chain_id` whose block timestamp is at or after `since`, oldest first. An error,
    /// never a partial list, if the ledger cannot be read back that far.
    async fn anchors_since(
        &self,
        chain_id: u32,
        since: u64,
    ) -> Result<Vec<AnchorPoint>, ChainError>;

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

    /// One transfer by hash with its amount, or `None` if the node has neither mined nor pending one.
    async fn transfer(&self, hash: &str) -> Result<Option<TransferView>, ChainError>;

    /// The current L1 owner of `chain_id`, or `None` if it is not registered. Read live, never cached: game
    /// administration follows this address.
    async fn chain_owner(&self, chain_id: u32) -> Result<Option<String>, ChainError>;

    /// Whether `chain_id` is registered on the ledger. `Ok(false)` only when the node answers that it
    /// is not; a node that cannot be read is an error, never "not registered".
    async fn appchain_registered(&self, chain_id: u32) -> Result<bool, ChainError>;
}

const PAGE: u64 = 50;
/// How far back `find_anchor` looks: a day's anchor is among the newest few.
const FIND_PAGES: u64 = 3;
/// How far back `anchors_since` will read before it gives up rather than return a partial answer.
const SINCE_PAGES: u64 = 40;
/// The node lists at most this many transactions for an address, and omits the pending ones when the
/// confirmed history alone fills the list.
const HISTORY_LIMIT: usize = 100;

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

/// A 2xx answer for `GET /api/appchain/ledger/{id}` means the chain exists only if it says it is that
/// chain. Anything else (a different id, no id) is an answer this code cannot trust: an error, never a
/// quiet `false` that would be reported as "not registered".
pub fn registered_chain_matches(body: &Value, chain_id: u32) -> Result<bool, ChainError> {
    match body["chain"]["id"].as_u64() {
        Some(id) if id == u64::from(chain_id) => Ok(true),
        _ => Err(ChainError::Invalid),
    }
}

/// A transaction answer read as a transfer. `Ok(None)` when the node says there is no such transaction; an
/// error when the answer is not one this code can trust. The status must be exactly `pending` or `confirmed`.
pub fn transfer_from_body(body: &Value) -> Result<Option<TransferView>, ChainError> {
    if body["success"] != Value::Bool(true) {
        return Ok(None);
    }
    let data = &body["data"];
    let status = data["status"].as_str().ok_or(ChainError::Invalid)?;
    if status != "pending" && status != "confirmed" {
        return Err(ChainError::Invalid);
    }
    Ok(Some(TransferView {
        sender: data["sender"].as_str().ok_or(ChainError::Invalid)?.to_string(),
        recipient: data["recipient"].as_str().ok_or(ChainError::Invalid)?.to_string(),
        amount: data["amount"].as_u64().ok_or(ChainError::Invalid)?,
        pending: status == "pending",
    }))
}

/// The owner a ledger answer names for `chain_id`: only if the answer is about that chain and the owner is a
/// well-formed address. Anything else is an answer this code cannot trust, never a guessed owner.
pub fn chain_owner_from_body(body: &Value, chain_id: u32) -> Result<String, ChainError> {
    let chain = &body["chain"];
    if chain["id"].as_u64() != Some(u64::from(chain_id)) {
        return Err(ChainError::Invalid);
    }
    let owner = chain["owner"].as_str().ok_or(ChainError::Invalid)?;
    if owner.len() != 64 || !owner.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
        return Err(ChainError::Invalid);
    }
    Ok(owner.to_string())
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

/// Parse the node's address history into the owner's AppChain transactions since `since`.
pub fn parse_owner_activity(
    body: &Value,
    owner: &str,
    since: u64,
) -> Result<OwnerActivity, ChainError> {
    let list = body["transactions"].as_array().ok_or(ChainError::Invalid)?;
    let mut transactions = Vec::new();
    let mut oldest = u64::MAX;
    let mut confirmed_next_nonce: Option<u64> = None;
    for row in list {
        let timestamp = row["timestamp"].as_u64().ok_or(ChainError::Invalid)?;
        oldest = oldest.min(timestamp);
        let status = row["status"].as_str().ok_or(ChainError::Invalid)?;
        if status != "pending" && status != "confirmed" {
            return Err(ChainError::Invalid);
        }
        if status == "confirmed" && row["sender"].as_str() == Some(owner) {
            let nonce = row["nonce"].as_u64().ok_or(ChainError::Invalid)?;
            confirmed_next_nonce = Some(confirmed_next_nonce.map_or(nonce + 1, |known| known.max(nonce + 1)));
        }
        if row["recipient"].as_str() == Some("appchain")
            && row["sender"].as_str() == Some(owner)
            && timestamp >= since
        {
            let hash = row["hash"].as_str().ok_or(ChainError::Invalid)?;
            if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(ChainError::Invalid);
            }
            transactions.push(OwnerTx {
                hash: hash.to_ascii_lowercase(),
                timestamp,
                pending: status == "pending",
            });
        }
    }
    // The node fills its list with confirmed history first and adds pending transactions only if there is
    // room. A full list therefore may be missing pending ones, and a list that stops short of `since` may
    // be missing confirmed ones: either way an empty answer is not evidence.
    let room_for_pending = list.len() < HISTORY_LIMIT;
    let reaches_back = list.len() < HISTORY_LIMIT || oldest < since;
    Ok(OwnerActivity {
        transactions,
        complete: room_for_pending && reaches_back,
        confirmed_complete: reaches_back,
        confirmed_next_nonce,
    })
}

#[async_trait]
impl GamesChain for HttpChain {
    async fn owner_appchain_activity(
        &self,
        owner: &str,
        since: u64,
    ) -> Result<OwnerActivity, ChainError> {
        let body = self
            .get(&format!(
                "/api/address/{owner}/transactions?limit={HISTORY_LIMIT}"
            ))
            .await?
            .ok_or(ChainError::Invalid)?;
        parse_owner_activity(&body, owner, since)
    }

    async fn account_nonce(&self, address: &str) -> Result<u64, ChainError> {
        let body = self
            .get(&format!("/api/account/{address}"))
            .await?
            .ok_or(ChainError::Invalid)?;
        body["data"]["nonce"].as_u64().ok_or(ChainError::Invalid)
    }

    async fn transaction(&self, hash: &str) -> Result<Option<TxView>, ChainError> {
        if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(ChainError::Invalid);
        }
        let Some(body) = self.get(&format!("/api/transaction/{hash}")).await? else {
            return Ok(None);
        };
        if body["success"] != Value::Bool(true) {
            return Ok(None);
        }
        let data = &body["data"];
        let status = data["status"].as_str().ok_or(ChainError::Invalid)?;
        if status != "pending" && status != "confirmed" {
            return Err(ChainError::Invalid);
        }
        Ok(Some(TxView {
            sender: data["sender"].as_str().ok_or(ChainError::Invalid)?.to_string(),
            recipient: data["recipient"].as_str().ok_or(ChainError::Invalid)?.to_string(),
            timestamp: data["timestamp"].as_u64().ok_or(ChainError::Invalid)?,
            pending: status == "pending",
        }))
    }

    async fn transfer(&self, hash: &str) -> Result<Option<TransferView>, ChainError> {
        if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(ChainError::Invalid);
        }
        let Some(body) = self.get(&format!("/api/transaction/{hash}")).await? else {
            return Ok(None);
        };
        transfer_from_body(&body)
    }

    async fn chain_owner(&self, chain_id: u32) -> Result<Option<String>, ChainError> {
        match self.get(&format!("/api/appchain/ledger/{chain_id}")).await? {
            None => Ok(None),
            Some(body) => chain_owner_from_body(&body, chain_id).map(Some),
        }
    }

    async fn appchain_registered(&self, chain_id: u32) -> Result<bool, ChainError> {
        match self.get(&format!("/api/appchain/ledger/{chain_id}")).await? {
            None => Ok(false),
            Some(body) => registered_chain_matches(&body, chain_id),
        }
    }

    async fn anchors_since(
        &self,
        chain_id: u32,
        since: u64,
    ) -> Result<Vec<AnchorPoint>, ChainError> {
        let Some(count) = self.anchor_count(chain_id).await? else {
            return Ok(Vec::new());
        };
        let mut found: Vec<AnchorPoint> = Vec::new();
        let mut from = count.saturating_sub(PAGE - 1).max(1);
        for _ in 0..SINCE_PAGES {
            if count == 0 {
                return Ok(found);
            }
            let page = self.anchors(chain_id, from).await?;
            let reached_older = page.iter().any(|p| p.block_timestamp < since) || from <= 1;
            found.extend(page.into_iter().filter(|p| p.block_timestamp >= since));
            if reached_older {
                found.sort_by_key(|p| p.sequence);
                found.dedup_by_key(|p| p.sequence);
                return Ok(found);
            }
            from = from.saturating_sub(PAGE).max(1);
        }
        // Read back as far as allowed and still not past `since`: say so rather than guess.
        Err(ChainError::Unavailable)
    }

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

    fn history(rows: &[(&str, &str, u64, &str)]) -> Value {
        json!({"transactions": rows.iter().enumerate().map(|(i, (recipient, status, ts, hash))| json!({
            "sender": OWNER, "recipient": recipient, "status": status, "timestamp": ts, "hash": hash, "nonce": 100 - i as u64,
        })).collect::<Vec<_>>()})
    }

    const OWNER: &str = "7d2cd1cc30c1088e0382b040c8b3832acf9e45feae88624d06c42cd3f9d9388e";

    #[test]
    fn the_owners_history_yields_only_its_appchain_transactions_in_the_window() {
        let h = "ab".repeat(32);
        let body = history(&[
            ("appchain", "pending", 500, &h),
            ("appchain", "confirmed", 400, &h),
            ("appchain", "confirmed", 100, &h),
            ("deadbeef", "confirmed", 450, &h),
        ]);
        let activity = parse_owner_activity(&body, OWNER, 300).unwrap();
        assert_eq!(
            activity.transactions.iter().map(|t| (t.timestamp, t.pending)).collect::<Vec<_>>(),
            vec![(500, true), (400, false)]
        );
        assert!(activity.complete, "a short list that reaches back past the window is complete");
    }

    #[test]
    fn a_full_list_or_one_that_stops_short_of_the_window_proves_nothing() {
        let h = "cd".repeat(32);
        // 100 rows: the node may have left the pending ones out.
        let rows: Vec<(&str, &str, u64, &str)> = (0..100).map(|i| ("appchain", "confirmed", 1_000 + i, h.as_str())).collect();
        assert!(!parse_owner_activity(&history(&rows), OWNER, 500).unwrap().complete);
        // Even if it reached back, a full list cannot rule out omitted pending transactions.
        let rows: Vec<(&str, &str, u64, &str)> = (0..100).map(|i| ("appchain", "confirmed", 100 + i, h.as_str())).collect();
        assert!(!parse_owner_activity(&history(&rows), OWNER, 500).unwrap().complete);
        // A list shorter than the limit is the owner's whole history (confirmed first, then pending), so it is
        // complete even if its oldest entry is newer than the window: there is nothing older to miss.
        let short = history(&[("appchain", "confirmed", 900, &h)]);
        assert!(parse_owner_activity(&short, OWNER, 500).unwrap().complete);
        // An empty history is complete: there is nothing in it at all.
        assert!(parse_owner_activity(&history(&[]), OWNER, 500).unwrap().complete);
        // A full list that reaches back past the window still has a complete CONFIRMED history, though its
        // pending list may be hidden: the two are reported separately.
        let reaching: Vec<(&str, &str, u64, &str)> = (0..100).map(|i| ("appchain", "confirmed", 100 + i, h.as_str())).collect();
        let activity = parse_owner_activity(&history(&reaching), OWNER, 500).unwrap();
        assert!(activity.confirmed_complete && !activity.complete);
        // A full list that does not reach back has neither.
        let not_reaching: Vec<(&str, &str, u64, &str)> = (0..100).map(|i| ("appchain", "confirmed", 1_000 + i, h.as_str())).collect();
        let activity = parse_owner_activity(&history(&not_reaching), OWNER, 500).unwrap();
        assert!(!activity.confirmed_complete && !activity.complete);
    }

    #[test]
    fn the_next_confirmed_nonce_is_the_newest_confirmed_owner_nonce_plus_one_even_when_the_list_is_full() {
        let h = "ab".repeat(32);
        // Newest first, as the node lists them: a pending one (nonce 7), then confirmed 6, 5, ... The pending
        // transaction does not count; only a confirmed one the owner SENT does.
        let body = json!({"transactions": [
            {"sender": OWNER, "recipient": "appchain", "status": "pending", "timestamp": 900, "hash": h, "nonce": 7},
            {"sender": OWNER, "recipient": "appchain", "status": "confirmed", "timestamp": 800, "hash": h, "nonce": 6},
            {"sender": "someone-else", "recipient": OWNER, "status": "confirmed", "timestamp": 850, "hash": h, "nonce": 40},
            {"sender": OWNER, "recipient": "deadbeef", "status": "confirmed", "timestamp": 700, "hash": h, "nonce": 5},
        ]});
        let activity = parse_owner_activity(&body, OWNER, 0).unwrap();
        assert_eq!(activity.confirmed_next_nonce, Some(7), "6 + 1, ignoring the pending one and another sender's");
        // Nothing the owner sent is confirmed: unknown, not zero.
        let none = json!({"transactions": [{"sender": OWNER, "recipient": "appchain", "status": "pending", "timestamp": 900, "hash": h, "nonce": 0}]});
        assert_eq!(parse_owner_activity(&none, OWNER, 0).unwrap().confirmed_next_nonce, None);
        assert_eq!(parse_owner_activity(&history(&[]), OWNER, 0).unwrap().confirmed_next_nonce, None);
        // A confirmed owner transaction without a nonce is refused, not guessed.
        let bad = json!({"transactions": [{"sender": OWNER, "recipient": "appchain", "status": "confirmed", "timestamp": 1, "hash": h}]});
        assert_eq!(parse_owner_activity(&bad, OWNER, 0), Err(ChainError::Invalid));
    }

    #[test]
    fn a_malformed_history_is_refused_not_guessed() {
        let h = "ab".repeat(32);
        for bad in [
            json!({"nope": []}),
            history(&[("appchain", "weird", 5, &h)]),
            history(&[("appchain", "confirmed", 5, "not-a-hash")]),
            json!({"transactions": [{"sender": OWNER, "recipient": "appchain", "status": "confirmed", "hash": h}]}),
        ] {
            assert_eq!(parse_owner_activity(&bad, OWNER, 0), Err(ChainError::Invalid), "{bad}");
        }
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

    #[test]
    fn a_ledger_answer_registers_a_chain_only_when_it_names_that_chain() {
        use serde_json::json;
        assert_eq!(registered_chain_matches(&json!({"chain": {"id": 3, "name": "x"}}), 3).unwrap(), true);
        // Another chain's record, an empty record, or no chain at all is not evidence either way.
        assert!(registered_chain_matches(&json!({"chain": {"id": 2}}), 3).is_err());
        assert!(registered_chain_matches(&json!({"chain": {}}), 3).is_err());
        assert!(registered_chain_matches(&json!({"success": true}), 3).is_err());
        assert!(registered_chain_matches(&json!({"chain": {"id": "3"}}), 3).is_err());
    }
}
