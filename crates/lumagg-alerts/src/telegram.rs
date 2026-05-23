//! Send messages to Telegram Bot API with per-key rate limiting.

use std::time::{Duration, Instant};

use tokio::sync::Mutex;

use anyhow::{Context, Result};
use tracing::{debug, warn};

const DEFAULT_COOLDOWN: Duration = Duration::from_secs(300);

#[derive(Clone, Debug)]
pub struct TelegramConfig {
    pub bot_token: String,
    pub chat_id: String,
}

impl TelegramConfig {
    pub fn from_env() -> Option<Self> {
        Self::from_env_filtered(None)
    }

    /// Like [`from_env`] but only on API instance `LISTEN_ADDR` ending with `:port` (avoids duplicate alerts).
    pub fn from_env_api_primary() -> Option<Self> {
        let port = std::env::var("TELEGRAM_PRIMARY_API_PORT").unwrap_or_else(|_| "3100".into());
        Self::from_env_filtered(Some(port.as_str()))
    }

    fn from_env_filtered(listen_port_suffix: Option<&str>) -> Option<Self> {
        let enabled = std::env::var("TELEGRAM_ALERTS_ENABLED")
            .ok()
            .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
            .unwrap_or(false);
        if !enabled {
            return None;
        }
        if let Some(port) = listen_port_suffix {
            let addr = std::env::var("LISTEN_ADDR").ok()?;
            if !addr.ends_with(&format!(":{port}")) {
                return None;
            }
        }
        let bot_token = std::env::var("TELEGRAM_BOT_TOKEN")
            .ok()
            .filter(|s| !s.is_empty())?;
        let chat_id = std::env::var("TELEGRAM_CHAT_ID")
            .ok()
            .filter(|s| !s.is_empty())?;
        Some(Self { bot_token, chat_id })
    }
}

pub struct TelegramAlerter {
    config: TelegramConfig,
    client: reqwest::Client,
    last_sent: Mutex<std::collections::HashMap<String, Instant>>,
}

impl TelegramAlerter {
    pub fn new(config: TelegramConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .expect("reqwest client"),
            last_sent: Mutex::new(std::collections::HashMap::new()),
        }
    }

    pub fn from_env() -> Option<Self> {
        TelegramConfig::from_env().map(Self::new)
    }

    pub fn from_env_api_primary() -> Option<Self> {
        TelegramConfig::from_env_api_primary().map(Self::new)
    }

    /// Always send (use sparingly for heartbeats).
    pub async fn send(&self, text: &str) -> Result<()> {
        self.send_inner(text).await
    }

    /// Send at most once per `cooldown` per `key`.
    pub async fn send_rate_limited(
        &self,
        key: &str,
        text: &str,
        cooldown: Duration,
    ) -> Result<()> {
        let now = Instant::now();
        let mut guard = self.last_sent.lock().await;
        if let Some(last) = guard.get(key) {
            if now.duration_since(*last) < cooldown {
                debug!(key, "telegram alert suppressed (cooldown)");
                return Ok(());
            }
        }
        guard.insert(key.to_string(), now);
        drop(guard);
        self.send_inner(text).await
    }

    pub async fn alert(&self, key: &str, text: &str) -> Result<()> {
        self.send_rate_limited(key, text, DEFAULT_COOLDOWN).await
    }

    async fn send_inner(&self, text: &str) -> Result<()> {
        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            self.config.bot_token
        );
        let resp = self
            .client
            .post(&url)
            .json(&serde_json::json!({
                "chat_id": self.config.chat_id,
                "text": text,
                "disable_web_page_preview": true,
            }))
            .send()
            .await
            .context("telegram send request")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            warn!(%status, body, "telegram send failed");
            anyhow::bail!("telegram API error: {status} {body}");
        }
        Ok(())
    }
}
