//! Environment configuration for the limit keeper.

use {
    anyhow::{anyhow, Result},
    serde::Deserialize,
    soroban_client::network::{NetworkPassphrase, Networks},
};

#[derive(Debug, Clone)]
pub struct KeeperConfig {
    pub rpc_url: String,
    pub secret: String,
    pub network: String,
    pub escrow_contract: String,
    pub aggregator_contract: String,
    pub quote_api_url: String,
    pub poll_secs: u64,
    pub cursor_path: String,
    pub dry_run: bool,
    pub max_fill: Option<i128>,
    pub reclaim: bool,
}

impl KeeperConfig {
    pub fn from_file(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let path = path.as_ref();
        let source = std::fs::read_to_string(path)
            .map_err(|e| anyhow!("read keeper config {}: {e}", path.display()))?;
        let file: KeeperFileConfig = toml::from_str(&source)
            .map_err(|e| anyhow!("parse keeper config {}: {e}", path.display()))?;
        Self::from_file_config(file)
    }

    pub fn from_env() -> Result<Self> {
        let dry_run = enabled("KEEPER_DRY_RUN");
        Self::validate(Self {
            rpc_url: required("KEEPER_RPC_URL")?,
            // A dry-run never signs or submits, so it must be runnable
            // without placing an operational signing key in the environment.
            secret: if dry_run {
                std::env::var("KEEPER_SECRET").unwrap_or_default()
            } else {
                required("KEEPER_SECRET")?
            },
            network: network_passphrase(&required("KEEPER_NETWORK")?)?.to_string(),
            escrow_contract: required("ESCROW_CONTRACT")?,
            aggregator_contract: required("AGGREGATOR_CONTRACT")?,
            quote_api_url: required("QUOTE_API_URL")?,
            poll_secs: optional_parse("KEEPER_POLL_SECS")?.unwrap_or(10),
            cursor_path: std::env::var("KEEPER_CURSOR_PATH").unwrap_or_else(|_| "keeper.cursor".into()),
            dry_run,
            max_fill: optional_parse("KEEPER_MAX_FILL")?,
            reclaim: enabled("KEEPER_RECLAIM"),
        })
    }

    fn from_file_config(file: KeeperFileConfig) -> Result<Self> {
        let dry_run = file.dry_run;
        let secret = if dry_run {
            String::new()
        } else {
            match (file.secret, file.secret_file) {
                (Some(secret), _) if !secret.trim().is_empty() => secret.trim().to_string(),
                (_, Some(path)) => std::fs::read_to_string(&path)
                    .map_err(|e| anyhow!("read keeper secret file {path}: {e}"))?
                    .trim()
                    .to_string(),
                _ => String::new(),
            }
        };
        Self::validate(Self {
            rpc_url: file.rpc_url,
            secret,
            network: network_passphrase(&file.network)?.to_string(),
            escrow_contract: file.escrow_contract,
            aggregator_contract: file.aggregator_contract,
            quote_api_url: file.quote_api_url,
            poll_secs: file.poll_secs,
            cursor_path: file.cursor_path,
            dry_run,
            max_fill: file.max_fill,
            reclaim: file.reclaim,
        })
    }

    fn validate(config: Self) -> Result<Self> {
        if !config.dry_run && config.secret.trim().is_empty() {
            return Err(anyhow!("keeper secret is required unless dry_run = true"));
        }
        if config.poll_secs == 0 {
            return Err(anyhow!("poll_secs must be greater than zero"));
        }
        Ok(config)
    }
}

#[derive(Debug, Deserialize)]
struct KeeperFileConfig {
    rpc_url: String,
    network: String,
    escrow_contract: String,
    aggregator_contract: String,
    quote_api_url: String,
    #[serde(default = "default_poll_secs")]
    poll_secs: u64,
    #[serde(default = "default_cursor_path")]
    cursor_path: String,
    #[serde(default)]
    dry_run: bool,
    #[serde(default)]
    max_fill: Option<i128>,
    #[serde(default)]
    reclaim: bool,
    #[serde(default)]
    secret: Option<String>,
    #[serde(default)]
    secret_file: Option<String>,
}

fn default_poll_secs() -> u64 {
    10
}

fn default_cursor_path() -> String {
    "keeper.cursor".into()
}

fn required(name: &str) -> Result<String> {
    std::env::var(name).map_err(|_| anyhow!("{name} must be set"))
}

fn optional_parse<T: std::str::FromStr>(name: &str) -> Result<Option<T>>
where
    T::Err: std::fmt::Display,
{
    std::env::var(name)
        .ok()
        .map(|value| value.parse().map_err(|error| anyhow!("invalid {name}: {error}")))
        .transpose()
}

fn enabled(name: &str) -> bool {
    matches!(std::env::var(name).as_deref(), Ok("1") | Ok("true") | Ok("TRUE"))
}

fn network_passphrase(network: &str) -> Result<&'static str> {
    match network {
        "public" => Ok(Networks::public()),
        "testnet" => Ok(Networks::testnet()),
        other => Err(anyhow!("unsupported KEEPER_NETWORK {other:?}; use public or testnet")),
    }
}

#[cfg(test)]
mod tests {
    use super::network_passphrase;

    #[test]
    fn resolves_testnet_network_name_to_its_passphrase() {
        assert_eq!(
            network_passphrase("testnet").unwrap(),
            "Test SDF Network ; September 2015"
        );
    }
}
