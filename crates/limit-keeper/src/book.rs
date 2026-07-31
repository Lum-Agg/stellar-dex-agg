use {
    serde::{Deserialize, Serialize},
    std::collections::BTreeMap,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
pub enum OrderKind {
    Limit,
    Dca,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct OpenOrder {
    pub kind: OrderKind,
    pub order_id: u64,
    pub owner: String,
    pub token_in: String,
    pub token_out: String,
    pub amount_in_remaining: i128,
    pub limit_out_per_in_e7: i128,
    pub expires_ledger: u32,
    pub chunk_amount: Option<i128>,
    pub next_executable_ledger: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrderEvent {
    Created(OpenOrder),
    Filled {
        kind: OrderKind,
        order_id: u64,
        amount_in_remaining: i128,
        next_executable_ledger: Option<u32>,
    },
    Cancelled {
        kind: OrderKind,
        order_id: u64,
    },
    Expired {
        kind: OrderKind,
        order_id: u64,
    },
}

#[derive(Debug, Default)]
pub struct OpenOrderBook {
    orders: BTreeMap<(OrderKind, u64), OpenOrder>,
}

impl OpenOrderBook {
    pub fn from_orders(orders: impl IntoIterator<Item = OpenOrder>) -> Self {
        let mut book = Self::default();
        for order in orders {
            book.apply_created(order);
        }
        book
    }

    pub fn get(&self, kind: OrderKind, order_id: u64) -> Option<&OpenOrder> {
        self.orders.get(&(kind, order_id))
    }

    pub fn apply(&mut self, event: OrderEvent) {
        match event {
            OrderEvent::Created(order) => self.apply_created(order),
            OrderEvent::Filled {
                kind,
                order_id,
                amount_in_remaining,
                next_executable_ledger,
            } => self.apply_filled(kind, order_id, amount_in_remaining, next_executable_ledger),
            OrderEvent::Cancelled { kind, order_id } => self.apply_cancelled(kind, order_id),
            OrderEvent::Expired { kind, order_id } => self.apply_expired(kind, order_id),
        }
    }

    pub fn apply_created(&mut self, order: OpenOrder) {
        self.orders.insert((order.kind, order.order_id), order);
    }

    pub fn apply_filled(
        &mut self,
        kind: OrderKind,
        order_id: u64,
        amount_in_remaining: i128,
        next_executable_ledger: Option<u32>,
    ) {
        if amount_in_remaining == 0 {
            self.orders.remove(&(kind, order_id));
        } else if let Some(order) = self.orders.get_mut(&(kind, order_id)) {
            order.amount_in_remaining = amount_in_remaining;
            if next_executable_ledger.is_some() {
                order.next_executable_ledger = next_executable_ledger;
            }
        }
    }

    pub fn apply_cancelled(&mut self, kind: OrderKind, order_id: u64) {
        self.orders.remove(&(kind, order_id));
    }

    pub fn apply_expired(&mut self, kind: OrderKind, order_id: u64) {
        self.orders.remove(&(kind, order_id));
    }

    pub fn iter(&self) -> impl Iterator<Item = &OpenOrder> {
        self.orders.values()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order(order_id: u64) -> OpenOrder {
        OpenOrder {
            kind: OrderKind::Limit,
            order_id,
            owner: "owner".into(),
            token_in: "in".into(),
            token_out: "out".into(),
            amount_in_remaining: 500,
            limit_out_per_in_e7: 20_000_000,
            expires_ledger: 999,
            chunk_amount: None,
            next_executable_ledger: None,
        }
    }

    #[test]
    fn lifecycle_updates_open_orders() {
        let mut book = OpenOrderBook::default();
        book.apply_created(order(7));
        book.apply_filled(OrderKind::Limit, 7, 300, None);
        assert_eq!(book.get(OrderKind::Limit, 7).unwrap().amount_in_remaining, 300);

        book.apply_cancelled(OrderKind::Limit, 7);
        assert!(book.get(OrderKind::Limit, 7).is_none());
    }

    #[test]
    fn filled_or_expired_orders_are_removed() {
        let mut book = OpenOrderBook::default();
        book.apply_created(order(7));
        book.apply_filled(OrderKind::Limit, 7, 0, None);
        assert!(book.get(OrderKind::Limit, 7).is_none());

        book.apply_created(order(8));
        book.apply_expired(OrderKind::Limit, 8);
        assert!(book.get(OrderKind::Limit, 8).is_none());
    }
}
