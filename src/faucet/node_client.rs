use crate::faucet::models::{
    NodeAccountData, NodeEnvelope, NodeFeeEstimateData, NodeTransactionData, SignedTransferRequest,
};
use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NodeClientError {
    #[error("node unavailable")]
    Unavailable,
    #[error("node rejected the faucet transaction")]
    Rejected,
    #[error("node returned an invalid response")]
    InvalidResponse,
    #[error("node did not know the transaction")]
    NotFound,
    #[error("node transaction hash did not match the signed envelope")]
    HashMismatch,
}

#[async_trait]
pub trait NodeApi: Send + Sync {
    async fn account(&self, address: &str) -> Result<NodeAccountData, NodeClientError>;
    async fn estimate(
        &self,
        recipient: &str,
        amount_base_units: u64,
    ) -> Result<NodeFeeEstimateData, NodeClientError>;
    async fn submit(
        &self,
        envelope: &SignedTransferRequest,
    ) -> Result<NodeTransactionData, NodeClientError>;
    async fn transaction_status(&self, hash: &str) -> Result<NodeTransactionData, NodeClientError>;
}

#[derive(Clone)]
pub struct NodeClient {
    client: Client,
    base_url: String,
}

impl NodeClient {
    pub fn new(base_url: String) -> Result<Self, NodeClientError> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|_| NodeClientError::InvalidResponse)?;
        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
        })
    }

    async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, NodeClientError> {
        let response = self
            .client
            .get(format!("{}{}", self.base_url, path))
            .send()
            .await
            .map_err(|_| NodeClientError::Unavailable)?;
        decode_response(response).await
    }
}

#[async_trait]
impl NodeApi for NodeClient {
    async fn account(&self, address: &str) -> Result<NodeAccountData, NodeClientError> {
        self.get(&format!("/api/account/{address}")).await
    }

    async fn estimate(
        &self,
        recipient: &str,
        amount_base_units: u64,
    ) -> Result<NodeFeeEstimateData, NodeClientError> {
        let amount = amount_base_units.to_string();
        let response = self
            .client
            .get(format!("{}/api/transaction/estimate", self.base_url))
            .query(&[("recipient", recipient), ("amount", amount.as_str())])
            .send()
            .await
            .map_err(|_| NodeClientError::Unavailable)?;
        decode_response(response).await
    }

    async fn submit(
        &self,
        envelope: &SignedTransferRequest,
    ) -> Result<NodeTransactionData, NodeClientError> {
        let response = self
            .client
            .post(format!("{}/api/transaction", self.base_url))
            .json(envelope)
            .send()
            .await
            .map_err(|_| NodeClientError::Unavailable)?;
        decode_response_with_rejection(response).await
    }

    async fn transaction_status(&self, hash: &str) -> Result<NodeTransactionData, NodeClientError> {
        self.get(&format!("/api/transaction/{hash}")).await
    }
}

async fn decode_response<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
) -> Result<T, NodeClientError> {
    if !response.status().is_success() {
        return Err(if response.status() == StatusCode::NOT_FOUND {
            NodeClientError::NotFound
        } else if response.status().is_server_error() {
            NodeClientError::Unavailable
        } else {
            NodeClientError::Rejected
        });
    }
    let envelope = response
        .json::<NodeEnvelope<T>>()
        .await
        .map_err(|_| NodeClientError::InvalidResponse)?;
    if !envelope.success {
        return Err(NodeClientError::Rejected);
    }
    envelope.data.ok_or(NodeClientError::InvalidResponse)
}

async fn decode_response_with_rejection<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
) -> Result<T, NodeClientError> {
    let status = response.status();
    if !status.is_success() {
        return Err(match status {
            StatusCode::REQUEST_TIMEOUT
            | StatusCode::TOO_MANY_REQUESTS
            | StatusCode::BAD_GATEWAY
            | StatusCode::SERVICE_UNAVAILABLE
            | StatusCode::GATEWAY_TIMEOUT => NodeClientError::Unavailable,
            StatusCode::NOT_FOUND => NodeClientError::NotFound,
            _ if status.is_server_error() => NodeClientError::Unavailable,
            _ => NodeClientError::Rejected,
        });
    }
    decode_response_from_success(response).await
}

async fn decode_response_from_success<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
) -> Result<T, NodeClientError> {
    let envelope = response
        .json::<NodeEnvelope<T>>()
        .await
        .map_err(|_| NodeClientError::InvalidResponse)?;
    if !envelope.success {
        return Err(NodeClientError::Rejected);
    }
    envelope.data.ok_or(NodeClientError::InvalidResponse)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_client_strips_trailing_slashes() {
        let client = NodeClient::new("http://127.0.0.1:8081///".into()).unwrap();
        assert_eq!(client.base_url, "http://127.0.0.1:8081");
    }
}
