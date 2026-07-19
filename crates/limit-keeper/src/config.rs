//! Environment configuration for the limit keeper.

use {
    anyhow::{anyhow, Result},
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
    pub fn from_env() -> Result<Self> {
        let dry_run = enabled("KEEPER_DRY_RUN");
        Ok(Self {
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
