use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use std::sync::Arc;
use thiserror::Error;
use zeroize::Zeroizing;

#[derive(Debug, Error)]
pub enum CaptchaError {
    #[error("captcha token was rejected")]
    Invalid,
    #[error("captcha provider is unavailable")]
    Unavailable,
    #[error("captcha provider is misconfigured")]
    Misconfigured,
}

#[async_trait]
pub trait CaptchaVerifier: Send + Sync {
    async fn verify(&self, token: &str, client_ip: Option<&str>) -> Result<bool, CaptchaError>;
}

pub struct TurnstileVerifier {
    client: Client,
    secret: Zeroizing<String>,
}

impl TurnstileVerifier {
    pub fn new(secret: String) -> Result<Self, CaptchaError> {
        if secret.trim().is_empty() {
            return Err(CaptchaError::Misconfigured);
        }
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .map_err(|_| CaptchaError::Misconfigured)?;
        Ok(Self {
            client,
            secret: Zeroizing::new(secret),
        })
    }
}

#[derive(Debug, Deserialize)]
struct TurnstileResponse {
    #[serde(default)]
    success: bool,
}

#[async_trait]
impl CaptchaVerifier for TurnstileVerifier {
    async fn verify(&self, token: &str, client_ip: Option<&str>) -> Result<bool, CaptchaError> {
        if token.trim().is_empty() || token.len() > 4096 {
            return Ok(false);
        }
        let mut form = vec![("secret", self.secret.as_str()), ("response", token)];
        if let Some(client_ip) = client_ip {
            if client_ip.len() <= 64 && client_ip.is_ascii() {
                form.push(("remoteip", client_ip));
            }
        }
        let response = self
            .client
            .post("https://challenges.cloudflare.com/turnstile/v0/siteverify")
            .form(&form)
            .send()
            .await
            .map_err(|_| CaptchaError::Unavailable)?;
        if !response.status().is_success() {
            return Err(CaptchaError::Unavailable);
        }
        let payload = response
            .json::<TurnstileResponse>()
            .await
            .map_err(|_| CaptchaError::Unavailable)?;
        Ok(payload.success)
    }
}

#[derive(Clone)]
pub struct StaticCaptchaVerifier {
    accept: bool,
}

impl StaticCaptchaVerifier {
    pub fn new(accept: bool) -> Arc<Self> {
        Arc::new(Self { accept })
    }
}

#[async_trait]
impl CaptchaVerifier for StaticCaptchaVerifier {
    async fn verify(&self, token: &str, _client_ip: Option<&str>) -> Result<bool, CaptchaError> {
        Ok(self.accept && !token.trim().is_empty())
    }
}
