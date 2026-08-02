use {
    anyhow::{Context, Result},
    serde::de::DeserializeOwned,
    std::path::Path,
};

pub mod aggregator;
pub mod arbitrage;

pub fn load<T: DeserializeOwned>(path: impl AsRef<Path>) -> Result<T> {
    let path = path.as_ref();
    let source = std::fs::read_to_string(path).with_context(|| format!("read config {}", path.display()))?;
    toml::from_str(&source).with_context(|| format!("parse config {}", path.display()))
}

fn set(name: &str, value: impl ToString) {
    std::env::set_var(name, value.to_string());
}

fn set_option<T: ToString>(name: &str, value: &Option<T>) {
    if let Some(value) = value {
        set(name, value.to_string());
    }
}

fn set_list(name: &str, values: &Option<Vec<String>>) {
    if let Some(values) = values {
        set(name, values.join(","));
    }
}

#[cfg(test)]
mod tests {
    use crate::aggregator::AggregatorConfig;

    const EMBEDDED: &str = r#"
        [network]
        rpc_url = "https://rpc.example.com"
    "#;

    const INDEXER: &str = r#"
        [network]
        rpc_url = "https://rpc.example.com"

        [api]
        aggregator_contract = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM"

        [indexer]
        db_path = "./data/indexer.db"
    "#;

    #[test]
    fn embedded_config_does_not_require_redis() {
        let config: AggregatorConfig = toml::from_str(EMBEDDED).unwrap();
        assert!(config.validate_embedded().is_ok());
    }

    #[test]
    fn cluster_config_requires_redis() {
        let config: AggregatorConfig = toml::from_str(EMBEDDED).unwrap();
        assert_eq!(
            config.validate_cluster().unwrap_err().to_string(),
            "redis section is required for cluster mode"
        );
    }

    #[test]
    fn unknown_keys_are_rejected() {
        let source = format!("{EMBEDDED}\nunknown = true\n");
        assert!(toml::from_str::<AggregatorConfig>(&source).is_err());
    }

    #[test]
    fn indexer_config_is_optional_but_required_for_indexer_validation() {
        let embedded: AggregatorConfig = toml::from_str(EMBEDDED).unwrap();
        assert_eq!(
            embedded.validate_indexer().unwrap_err().to_string(),
            "api.aggregator_contract is required for the indexer"
        );

        let indexer: AggregatorConfig = toml::from_str(INDEXER).unwrap();
        assert!(indexer.validate_indexer().is_ok());
        assert_eq!(indexer.indexer.unwrap().page_limit, 10_000);
    }

    #[test]
    fn indexer_mode_is_validated() {
        let source = format!("{INDEXER}\nmode = \"invalid\"\n");
        let config: AggregatorConfig = toml::from_str(&source).unwrap();
        assert_eq!(
            config.validate_indexer().unwrap_err().to_string(),
            "indexer.mode must be events, envelope, or both"
        );
    }
}
