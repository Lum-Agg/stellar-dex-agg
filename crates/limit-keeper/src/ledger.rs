use {
    crate::book::{OpenOrder, OpenOrderBook},
    anyhow::{Context, Result},
    serde::{Deserialize, Serialize},
    std::{fs, path::Path},
};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct KeeperCheckpoint {
    pub cursor: u32,
    pub orders: Vec<OpenOrder>,
}

impl KeeperCheckpoint {
    pub fn capture(cursor: u32, book: &OpenOrderBook) -> Self {
        Self {
            cursor,
            orders: book.iter().cloned().collect(),
        }
    }

    pub fn into_parts(self) -> (u32, OpenOrderBook) {
        (self.cursor, OpenOrderBook::from_orders(self.orders))
    }
}

pub fn load_checkpoint(path: impl AsRef<Path>) -> Result<Option<KeeperCheckpoint>> {
    let path = path.as_ref();
    match fs::read_to_string(path) {
        Ok(value) => {
            Ok(Some(serde_json::from_str(&value).with_context(|| {
                format!("parse keeper checkpoint {}", path.display())
            })?))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("read keeper checkpoint {}", path.display())),
    }
}

pub fn save_checkpoint(path: impl AsRef<Path>, checkpoint: &KeeperCheckpoint) -> Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent).with_context(|| format!("create checkpoint directory {}", parent.display()))?;
    }
    let tmp = path.with_extension("tmp");
    let json = serde_json::to_vec(checkpoint).context("serialize keeper checkpoint")?;
    fs::write(&tmp, json).with_context(|| format!("write keeper checkpoint {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("replace keeper checkpoint {}", path.display()))
}

#[cfg(test)]
mod tests {
    use {
        super::{load_checkpoint, save_checkpoint, KeeperCheckpoint},
        crate::book::{OpenOrder, OpenOrderBook, OrderKind},
    };

    #[test]
    fn saves_and_loads_cursor_and_open_orders() {
        let path = std::env::temp_dir().join(format!("limit-keeper-{}.cursor", std::process::id()));
        let book = OpenOrderBook::from_orders([OpenOrder {
            kind: OrderKind::Dca,
            order_id: 7,
            owner: "owner".into(),
            token_in: "in".into(),
            token_out: "out".into(),
            amount_in_remaining: 500,
            limit_out_per_in_e7: 0,
            expires_ledger: 999,
            chunk_amount: Some(100),
            next_executable_ledger: Some(200),
        }]);

        save_checkpoint(&path, &KeeperCheckpoint::capture(123, &book)).unwrap();
        let (cursor, restored) = load_checkpoint(&path).unwrap().unwrap().into_parts();

        assert_eq!(cursor, 123);
        assert_eq!(restored.get(OrderKind::Dca, 7), book.get(OrderKind::Dca, 7));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_legacy_cursor_without_order_book() {
        let path = std::env::temp_dir().join(format!("limit-keeper-legacy-{}.cursor", std::process::id()));
        std::fs::write(&path, "456\n").unwrap();

        let result = load_checkpoint(&path);

        assert!(result.is_err());
        std::fs::remove_file(path).unwrap();
    }
}
