use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use std::sync::Arc;
use thiserror::Error;
use zeroize::Zeroizing;

const TURNSTILE_SITEVERIFY_URL: &str = "https://challenges.cloudflare.com/turnstile/v0/siteverify";
pub const TURNSTILE_HOSTNAME: &str = "faucet.ultranetwork.cc";
pub const TURNSTILE_ACTION: &str = "faucet_claim";

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
    expected_hostname: String,
    expected_action: String,
    siteverify_url: String,
}

impl TurnstileVerifier {
    pub fn new(secret: String) -> Result<Self, CaptchaError> {
        Self::with_policy(secret)
    }

    fn with_policy(secret: String) -> Result<Self, CaptchaError> {
        Self::build(
            secret,
            TURNSTILE_HOSTNAME.to_string(),
            TURNSTILE_ACTION.to_string(),
            TURNSTILE_SITEVERIFY_URL.to_string(),
        )
    }

    #[cfg(test)]
    fn with_policy_and_endpoint(
        secret: String,
        expected_hostname: String,
        expected_action: String,
        siteverify_url: String,
    ) -> Result<Self, CaptchaError> {
        Self::build(secret, expected_hostname, expected_action, siteverify_url)
    }

    fn build(
        secret: String,
        expected_hostname: String,
        expected_action: String,
        siteverify_url: String,
    ) -> Result<Self, CaptchaError> {
        if secret.trim().is_empty()
            || expected_hostname.trim().is_empty()
            || expected_action.trim().is_empty()
            || siteverify_url.trim().is_empty()
        {
            return Err(CaptchaError::Misconfigured);
        }
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .map_err(|_| CaptchaError::Misconfigured)?;
        Ok(Self {
            client,
            secret: Zeroizing::new(secret),
            expected_hostname,
            expected_action,
            siteverify_url,
        })
    }

    fn accepts(&self, payload: &TurnstileResponse) -> bool {
        payload.success
            && payload.hostname.as_deref() == Some(self.expected_hostname.as_str())
            && payload.action.as_deref() == Some(self.expected_action.as_str())
    }
}

#[derive(Debug, Deserialize)]
struct TurnstileResponse {
    #[serde(default)]
    success: bool,
    #[serde(default)]
    hostname: Option<String>,
    #[serde(default)]
    action: Option<String>,
    #[serde(rename = "error-codes", default)]
    _error_codes: Vec<String>,
}

fn decode_provider_response(body: &str) -> Result<TurnstileResponse, CaptchaError> {
    serde_json::from_str(body).map_err(|_| CaptchaError::Unavailable)
}

fn provider_status_is_usable(status: StatusCode) -> Result<(), CaptchaError> {
    if status.is_success() {
        Ok(())
    } else {
        Err(CaptchaError::Unavailable)
    }
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
            .post(&self.siteverify_url)
            .form(&form)
            .send()
            .await
            .map_err(|_| CaptchaError::Unavailable)?;
        provider_status_is_usable(response.status())?;
        let body = response
            .text()
            .await
            .map_err(|_| CaptchaError::Unavailable)?;
        let payload = decode_provider_response(&body)?;
        Ok(self.accepts(&payload))
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    fn verifier() -> TurnstileVerifier {
        TurnstileVerifier::new("test-secret".into()).unwrap()
    }

    fn payload(hostname: Option<&str>, action: Option<&str>, success: bool) -> TurnstileResponse {
        TurnstileResponse {
            success,
            hostname: hostname.map(ToOwned::to_owned),
            action: action.map(ToOwned::to_owned),
            _error_codes: Vec::new(),
        }
    }

    fn mock_siteverify(status: &'static str, body: &'static str) -> String {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buffer = [0u8; 1024];
            let body_start = loop {
                let count = stream.read(&mut buffer).unwrap();
                if count == 0 {
                    break None;
                }
                request.extend_from_slice(&buffer[..count]);
                if let Some(index) = request.windows(4).position(|window| window == b"\r\n\r\n") {
                    break Some(index + 4);
                }
            };
            if let Some(body_start) = body_start {
                let headers = String::from_utf8_lossy(&request[..body_start]);
                let content_length = headers
                    .lines()
                    .find_map(|line| line.strip_prefix("Content-Length: "))
                    .and_then(|value| value.trim().parse::<usize>().ok())
                    .unwrap_or_default();
                while request.len() < body_start + content_length {
                    let count = stream.read(&mut buffer).unwrap();
                    if count == 0 {
                        break;
                    }
                    request.extend_from_slice(&buffer[..count]);
                }
            }
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        format!("http://{address}")
    }

    fn verifier_at(endpoint: String) -> TurnstileVerifier {
        TurnstileVerifier::with_policy_and_endpoint(
            "test-secret".into(),
            TURNSTILE_HOSTNAME.into(),
            TURNSTILE_ACTION.into(),
            endpoint,
        )
        .unwrap()
    }

    #[test]
    fn policy_uses_the_canonical_faucet_context() {
        let verifier = verifier();
        assert_eq!(verifier.expected_hostname, TURNSTILE_HOSTNAME);
        assert_eq!(verifier.expected_action, TURNSTILE_ACTION);
        assert_eq!(verifier.siteverify_url, TURNSTILE_SITEVERIFY_URL);
    }

    #[test]
    fn accepts_only_success_with_exact_hostname_and_action() {
        let verifier = verifier();
        assert!(verifier.accepts(&payload(
            Some(TURNSTILE_HOSTNAME),
            Some(TURNSTILE_ACTION),
            true,
        )));
        assert!(!verifier.accepts(&payload(
            Some("www.ultranetwork.cc"),
            Some(TURNSTILE_ACTION),
            true,
        )));
        assert!(!verifier.accepts(&payload(Some(TURNSTILE_HOSTNAME), Some("login"), true,)));
        assert!(!verifier.accepts(&payload(
            Some(TURNSTILE_HOSTNAME),
            Some(TURNSTILE_ACTION),
            false,
        )));
    }

    #[test]
    fn missing_context_fields_are_rejected() {
        let verifier = verifier();
        assert!(!verifier.accepts(&payload(None, Some(TURNSTILE_ACTION), true)));
        assert!(!verifier.accepts(&payload(Some(TURNSTILE_HOSTNAME), None, true)));
    }

    #[test]
    fn provider_failure_is_not_accepted() {
        let verifier = verifier();
        let response =
            decode_provider_response(r#"{"success":false,"error-codes":["timeout-or-duplicate"]}"#)
                .unwrap();
        assert!(!verifier.accepts(&response));
    }

    #[test]
    fn malformed_provider_response_is_unavailable() {
        assert!(matches!(
            decode_provider_response("{not-json"),
            Err(CaptchaError::Unavailable)
        ));
    }

    #[test]
    fn non_success_provider_status_is_unavailable() {
        assert!(provider_status_is_usable(StatusCode::OK).is_ok());
        assert!(matches!(
            provider_status_is_usable(StatusCode::BAD_GATEWAY),
            Err(CaptchaError::Unavailable)
        ));
    }

    #[test]
    fn policy_must_not_be_empty() {
        assert!(matches!(
            TurnstileVerifier::with_policy_and_endpoint(
                "secret".into(),
                "".into(),
                TURNSTILE_ACTION.into(),
                TURNSTILE_SITEVERIFY_URL.into()
            ),
            Err(CaptchaError::Misconfigured)
        ));
        assert!(matches!(
            TurnstileVerifier::with_policy_and_endpoint(
                "secret".into(),
                TURNSTILE_HOSTNAME.into(),
                "".into(),
                TURNSTILE_SITEVERIFY_URL.into()
            ),
            Err(CaptchaError::Misconfigured)
        ));
        assert!(matches!(
            TurnstileVerifier::with_policy_and_endpoint(
                "secret".into(),
                TURNSTILE_HOSTNAME.into(),
                TURNSTILE_ACTION.into(),
                "".into()
            ),
            Err(CaptchaError::Misconfigured)
        ));
    }

    #[tokio::test]
    async fn verify_accepts_exact_context_from_siteverify() {
        let endpoint = mock_siteverify(
            "200 OK",
            r#"{"success":true,"hostname":"faucet.ultranetwork.cc","action":"faucet_claim"}"#,
        );
        let verifier = verifier_at(endpoint);
        assert!(verifier
            .verify("test-token", Some("203.0.113.10"))
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn verify_rejects_context_mismatch_and_missing_context() {
        for body in [
            r#"{"success":true,"hostname":"www.ultranetwork.cc","action":"faucet_claim"}"#,
            r#"{"success":true,"hostname":"faucet.ultranetwork.cc","action":"login"}"#,
            r#"{"success":true}"#,
        ] {
            let endpoint = mock_siteverify("200 OK", body);
            let verifier = verifier_at(endpoint);
            assert!(!verifier.verify("test-token", None).await.unwrap());
        }
    }

    #[tokio::test]
    async fn verify_maps_provider_failure_and_malformed_json() {
        let unavailable = verifier_at(mock_siteverify("503 Service Unavailable", "{}"));
        assert!(matches!(
            unavailable.verify("test-token", None).await,
            Err(CaptchaError::Unavailable)
        ));

        let malformed = verifier_at(mock_siteverify("200 OK", "not-json"));
        assert!(matches!(
            malformed.verify("test-token", None).await,
            Err(CaptchaError::Unavailable)
        ));
    }
}
