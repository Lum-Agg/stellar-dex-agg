use stellar_xdr::curr::{Limits, ReadXdr};

/// Decodes a failed user submission: PathPaymentStrictSend with dest_min=0 →
/// op_malformed.
#[test]
fn decode_user_tx_dest_min_zero_is_malformed() {
    let tx = "AAAAAgAAAAA9FUgvRqcn9062kJm9X1xZfXBl5pRdlCYLWBwKvmvGxwABhqADq4v7AAAAZAAAAAAAAAAAAAAAAQAAAAAAAAANAAAAAAAAAAABMS0AAAAAAD0VSC9Gpyf3TraQmb1fXFl9cGXmlF2UJgtYHAq+a8bHAAAAAVVTREMAAAAAO5kROA7+mIugqJAOsc/kTzZvfb6Ua+0HckD39iTfFcUAAAAAAAAAAAAAAAAAAAAAAAAAAb5rxscAAABAMfnkNcNHs07OABUjfX9leHdXr9Y8lhk/F+ZBAJCRsFddTzgrNfuXu8776yov+Ib0xuoma8z+P+Fy7KNC9AIdDw==";
    let env = stellar_xdr::curr::TransactionEnvelope::from_xdr_base64(tx, Limits::none()).unwrap();
    let stellar_xdr::curr::TransactionEnvelope::Tx(v1) = env else {
        panic!("expected v1 tx");
    };
    let op = &v1.tx.operations[0];
    let stellar_xdr::curr::OperationBody::PathPaymentStrictSend(p) = &op.body else {
        panic!("expected PathPaymentStrictSend");
    };
    assert_eq!(p.send_amount, 20_000_000);
    assert_eq!(p.dest_min, 0, "Stellar rejects dest_min <= 0 as MALFORMED");
    assert!(p.path.is_empty());
}
