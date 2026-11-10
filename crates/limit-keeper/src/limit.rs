pub use lumagg_contract_types::required_min_out;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_min_out_scales() {
        assert_eq!(required_min_out(5_000_000, 20_000_000), Some(10_000_000));
    }
}
