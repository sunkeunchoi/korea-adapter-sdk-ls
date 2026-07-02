//! Order-identity chaining across KRX modify's new-order-number chaining (R9).
//!
//! KRX modify/cancel return a **new** `OrdNo` whose parent is `PrntOrdNo`; the
//! original order keeps its nautilus `ClientOrderId`. This map tracks
//! `ClientOrderId ↔ {OrdNo₀, OrdNo₁, …}` so an SC execution event keyed on **any**
//! chained OrdNo resolves back to the right order — including a fill racing a modify
//! ack.
//!
//! v1 wires only [`OrderChain::register`] (on submit accept); `append_child` /
//! `resolve` / `forget` are **staged for the live fill + modify/cancel wave**, when
//! the OrderEvent (SC) lane is subscribed (unobservable on bare paper today).

use std::collections::HashMap;

use nautilus_model::identifiers::ClientOrderId;

/// Bidirectional map between a nautilus [`ClientOrderId`] and its chain of LS order
/// numbers.
#[derive(Debug, Default)]
pub struct OrderChain {
    by_client: HashMap<ClientOrderId, Vec<String>>,
    by_ordno: HashMap<String, ClientOrderId>,
}

impl OrderChain {
    /// A fresh, empty chain map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register the initial OrdNo for a newly-accepted order.
    pub fn register(&mut self, client_order_id: ClientOrderId, ord_no: impl Into<String>) {
        let ord_no = ord_no.into();
        self.by_client
            .entry(client_order_id)
            .or_default()
            .push(ord_no.clone());
        self.by_ordno.insert(ord_no, client_order_id);
    }

    /// Append a modify/cancel's **new** OrdNo to the chain of the order identified
    /// by `parent_ord_no` (`PrntOrdNo`). If the parent is unknown, the new OrdNo is
    /// left unmapped and `false` is returned (the caller can reconcile).
    pub fn append_child(&mut self, parent_ord_no: &str, new_ord_no: impl Into<String>) -> bool {
        let new_ord_no = new_ord_no.into();
        match self.by_ordno.get(parent_ord_no).copied() {
            Some(client) => {
                self.by_client.entry(client).or_default().push(new_ord_no.clone());
                self.by_ordno.insert(new_ord_no, client);
                true
            }
            None => false,
        }
    }

    /// Resolve any chained OrdNo back to its owning [`ClientOrderId`].
    pub fn resolve(&self, ord_no: &str) -> Option<ClientOrderId> {
        self.by_ordno.get(ord_no).copied()
    }

    /// The full OrdNo chain for a client order id (oldest first).
    pub fn chain_of(&self, client_order_id: &ClientOrderId) -> Option<&[String]> {
        self.by_client.get(client_order_id).map(Vec::as_slice)
    }

    /// Forget an order (e.g. on terminal Filled/Canceled) — drops both directions.
    pub fn forget(&mut self, client_order_id: &ClientOrderId) {
        if let Some(chain) = self.by_client.remove(client_order_id) {
            for ord_no in chain {
                self.by_ordno.remove(&ord_no);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_on_modified_ordno_resolves_to_original_client_id() {
        let mut chain = OrderChain::new();
        let client = ClientOrderId::from("O-ABC-1");
        chain.register(client, "1001");
        // A modify creates OrdNo 1002 with parent 1001.
        assert!(chain.append_child("1001", "1002"));
        // A fill can arrive on EITHER the original or the modified OrdNo.
        assert_eq!(chain.resolve("1001"), Some(client));
        assert_eq!(chain.resolve("1002"), Some(client));
        assert_eq!(chain.chain_of(&client).unwrap(), &["1001", "1002"]);
    }

    #[test]
    fn unknown_parent_is_not_chained() {
        let mut chain = OrderChain::new();
        assert!(!chain.append_child("9999", "1002"), "unknown parent leaves the child unmapped");
        assert!(chain.resolve("1002").is_none());
    }

    #[test]
    fn forget_drops_both_directions() {
        let mut chain = OrderChain::new();
        let client = ClientOrderId::from("O-ABC-2");
        chain.register(client, "2001");
        chain.append_child("2001", "2002");
        chain.forget(&client);
        assert!(chain.resolve("2001").is_none());
        assert!(chain.resolve("2002").is_none());
        assert!(chain.chain_of(&client).is_none());
    }
}
