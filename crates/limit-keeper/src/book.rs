use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenOrder {
    pub order_id: u64,
    pub owner: String,
    pub token_in: String,
    pub token_out: String,
    pub amount_in_remaining: i128,
    pub limit_out_per_in_e7: i128,
    pub expires_ledger: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrderEvent {
    Created(OpenOrder),
    Filled {
        order_id: u64,
        amount_in_remaining: i128,
    },
    Cancelled {
        order_id: u64,
    },
    Expired {
        order_id: u64,
    },
}

#[derive(Debug, Default)]
pub struct OpenOrderBook {
    orders: BTreeMap<u64, OpenOrder>,
}

impl OpenOrderBook {
    pub fn get(&self, order_id: u64) -> Option<&OpenOrder> {
        self.orders.get(&order_id)
    }

    pub fn apply(&mut self, event: OrderEvent) {
        match event {
            OrderEvent::Created(order) => self.apply_created(order),
            OrderEvent::Filled {
                order_id,
                amount_in_remaining,
            } => self.apply_filled(order_id, amount_in_remaining),
            OrderEvent::Cancelled { order_id } => self.apply_cancelled(order_id),
            OrderEvent::Expired { order_id } => self.apply_expired(order_id),
        }
    }

    pub fn apply_created(&mut self, order: OpenOrder) {
        self.orders.insert(order.order_id, order);
    }

    pub fn apply_filled(&mut self, order_id: u64, amount_in_remaining: i128) {
        if amount_in_remaining == 0 {
            self.orders.remove(&order_id);
        } else if let Some(order) = self.orders.get_mut(&order_id) {
            order.amount_in_remaining = amount_in_remaining;
        }
    }

    pub fn apply_cancelled(&mut self, order_id: u64) {
        self.orders.remove(&order_id);
    }

    pub fn apply_expired(&mut self, order_id: u64) {
        self.orders.remove(&order_id);
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
            order_id,
            owner: "owner".into(),
            token_in: "in".into(),
            token_out: "out".into(),
            amount_in_remaining: 500,
            limit_out_per_in_e7: 20_000_000,
            expires_ledger: 999,
        }
    }

    #[test]
    fn lifecycle_updates_open_orders() {
        let mut book = OpenOrderBook::default();
        book.apply_created(order(7));
        book.apply_filled(7, 300);
        assert_eq!(book.get(7).unwrap().amount_in_remaining, 300);

        book.apply_cancelled(7);
        assert!(book.get(7).is_none());
    }

    #[test]
    fn filled_or_expired_orders_are_removed() {
        let mut book = OpenOrderBook::default();
        book.apply_created(order(7));
        book.apply_filled(7, 0);
        assert!(book.get(7).is_none());

        book.apply_created(order(8));
        book.apply_expired(8);
        assert!(book.get(8).is_none());
    }
}

