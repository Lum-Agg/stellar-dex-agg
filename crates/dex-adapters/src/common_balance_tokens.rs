//! SAC contracts batch-fetched by `GET /api/v1/balances` as a **fallback**
//! when the quote-engine catalog is empty, and always **merged into** the
//! catalog set (priority hubs).
//!
//! Primary scope is now the full quote-engine token catalog (Soroban
//! `balance` simulates — no Horizon).
//!
//! Sources for this curated subset:
//! - [`crate::classic_dex::CLASSIC_ASSETS`] — XLM / USDC / EURC classic +
//!   Soroban paths
//! - Frontend swap defaults (`TokenSelector` priority row)
//! - [`crate::sushi`] pool-discovery hub list (BLND, yXLM, …)
//! - High-volume mainnet SACs (BTC, etc.)
//!
//! Tokens outside the catalog still use `GET /api/v1/balance` lazily.

/// Mainnet SAC ids queried in one `/api/v1/balances` request.
pub const COMMON_BALANCE_TOKEN_IDS: &[&str] = &[
    // Classic + swap UI priority
    "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA", // XLM
    "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75", // USDC
    "CDTKPWPLOURQA2SGTKTUQOWRCBZEORB4BWBOMJ3D3ZTQQSGE5F6JBQLV", // EURC
    "CAUIKL3IYGMERDRUN6YSCLWVAKIFG5Q4YJHUKM4S4NJZQIA3BAS6OJPK", // AQUA
    // Soroswap / Sushi / Comet routing hubs
    "CAS3FL6TLZKDGGSISDBWGGPXT3NRR4DYTZD7YOD3HMYO6LTJUVGRVEAM", // BLND
    "CBZVSNVB55ANF3LBFTU2LKGD3BJKFMHIGISKND7LBSPHYY3MAQH4AMPR", // yXLM
    "CAUIKL3IYGMERDRUN6YSCLWVAKIFG5Q4YJHUKM4S4MZLQO346H4GQ2O2", // FIDR
    "CCGIMRMF6XGCXBPFY3OIAFAHD24HO5MBNHPFMHBHCNDS2AIMYQCL7PSI", // SHX
    "CAO7DDJNGMOYQPRYDY5JVZ5YEK4UQBSMGLAEWRCUOTRMDSBMGWSAATDZ", // BTC
    "CBHIQPUXLFLC5O44ZJVUTCL5LMZFLVGU5DEIGSYKBSAPFMOGTKOQEPFM", // BTCLN
    "CALLENEEHRW63YKCIOLY2SR7PO3K6Y4DMUCAYA5ZMJ3N5JXFO5R4Y7J2", // AVAX
    "CD25MNVTZDL4Y3XBCPCJXGXATV5WUHHOWMYFF4YBEGU5FCPGMYTVG5JY", // BLND (alt SAC)
    "CACXKRVCW7I6CWX6RS6ANFDKVCOUI2PB6LTDUROL3J3FMJCRZ4ZLQRF6", // AUDD
    "CCT4ZYIYZ3TUO2AWQFEOFGBZ6HQP3GW5TA37CK7CRZVFRDXYTHTYX7KP", // BnUSD
    "CBUJ6F5SPBPFGGTLO4FTD6IMRX5PDEMAEFX7WNPEWGRYMHOQNDJ6J44Y", // AUD
];

pub fn is_common_balance_token(contract_id: &str) -> bool {
    contract_id.starts_with('C') && contract_id.len() == 56 && COMMON_BALANCE_TOKEN_IDS.contains(&contract_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn includes_core_swap_tokens() {
        assert!(is_common_balance_token(
            "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA"
        ));
        assert!(is_common_balance_token(
            "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75"
        ));
    }

    #[test]
    fn list_is_small_and_unique() {
        let mut seen = std::collections::HashSet::new();
        for id in COMMON_BALANCE_TOKEN_IDS {
            assert!(id.starts_with('C') && id.len() == 56);
            assert!(seen.insert(*id), "duplicate common balance token: {id}");
        }
        assert!(COMMON_BALANCE_TOKEN_IDS.len() <= 32);
    }
}
