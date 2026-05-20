use std::path::Path;

use dex_adapters::PoolCache;

fn load_pool_cache() -> PoolCache {
    let cache_path = Path::new("../../data/pool_cache.json");
    let cache = PoolCache::load(&cache_path).unwrap();
    cache
}

#[tokio::test]
async fn test_quote() {
    let cache = load_pool_cache();
    assert!(cache.sources.len() > 0);

    let pairs = cache
        .get_pairs("CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA")
        .unwrap_or_default();
    dbg!(&pairs);
}
