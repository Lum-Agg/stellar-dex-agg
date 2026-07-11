use {
    axum::{
        extract::{ConnectInfo, Request, State},
        http::StatusCode,
        middleware::Next,
        response::{IntoResponse, Response},
    },
    std::{
        collections::{HashMap, HashSet, VecDeque},
        net::{IpAddr, SocketAddr},
        sync::{Arc, Mutex},
        time::{Duration, Instant},
    },
};

#[derive(Clone)]
struct SlidingWindowLimiter<K>
where
    K: Eq + std::hash::Hash + Clone,
{
    inner: Arc<Mutex<HashMap<K, VecDeque<Instant>>>>,
    limit: usize,
    window: Duration,
}

impl<K> SlidingWindowLimiter<K>
where
    K: Eq + std::hash::Hash + Clone,
{
    fn new(limit: usize, window: Duration) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            limit,
            window,
        }
    }

    fn allow_now(&self, key: K, now: Instant) -> bool {
        let mut store = self.inner.lock().expect("rate limiter mutex poisoned");
        let entries = store.entry(key).or_default();
        while let Some(ts) = entries.front() {
            if now.duration_since(*ts) >= self.window {
                entries.pop_front();
            } else {
                break;
            }
        }
        if entries.len() >= self.limit {
            return false;
        }
        entries.push_back(now);
        true
    }
}

pub type IpRateLimiter = SlidingWindowLimiter<IpAddr>;
type PartnerRateLimiter = SlidingWindowLimiter<String>;

#[derive(Clone)]
pub struct RateLimitState {
    ip: IpRateLimiter,
    partner: PartnerRateLimiter,
    partner_keys: Arc<HashSet<String>>,
}

impl RateLimitState {
    pub fn from_env() -> Self {
        let partner_keys: HashSet<String> = std::env::var("LUMAGG_PARTNER_API_KEYS")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        Self {
            ip: IpRateLimiter::new(10, Duration::from_secs(1)),
            partner: PartnerRateLimiter::new(60, Duration::from_secs(1)),
            partner_keys: Arc::new(partner_keys),
        }
    }

    fn is_partner_key(&self, key: &str) -> bool {
        self.partner_keys.contains(key)
    }

    fn partner_keys_enabled(&self) -> bool {
        !self.partner_keys.is_empty()
    }
}

pub async fn rate_limit_middleware(State(state): State<RateLimitState>, request: Request, next: Next) -> Response {
    let now = Instant::now();
    if let Some(api_key) = request
        .headers()
        .get("X-API-Key")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if state.is_partner_key(api_key) {
            if !state.partner.allow_now(api_key.to_string(), now) {
                return (
                    StatusCode::TOO_MANY_REQUESTS,
                    "rate limit exceeded: max 60 requests/second per partner API key",
                )
                    .into_response();
            }
            return next.run(request).await;
        }
        if state.partner_keys_enabled() {
            return (StatusCode::UNAUTHORIZED, "invalid X-API-Key").into_response();
        }
    }

    if let Some(ConnectInfo(addr)) = request.extensions().get::<ConnectInfo<SocketAddr>>() {
        if !state.ip.allow_now(addr.ip(), now) {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                "rate limit exceeded: max 10 requests/second per IP (use X-API-Key for higher limits)",
            )
                .into_response();
        }
    }
    next.run(request).await
}

#[cfg(test)]
mod tests {
    use {super::*, std::net::Ipv4Addr};

    #[test]
    fn limits_requests_per_window() {
        let limiter = IpRateLimiter::new(2, Duration::from_secs(1));
        let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let now = Instant::now();
        assert!(limiter.allow_now(ip, now));
        assert!(limiter.allow_now(ip, now));
        assert!(!limiter.allow_now(ip, now));
        assert!(limiter.allow_now(ip, now + Duration::from_secs(1)));
    }

    #[test]
    fn partner_keys_parse_from_env_style_list() {
        let state = RateLimitState {
            ip: IpRateLimiter::new(10, Duration::from_secs(1)),
            partner: PartnerRateLimiter::new(60, Duration::from_secs(1)),
            partner_keys: Arc::new(["key-a", "key-b"].into_iter().map(str::to_string).collect()),
        };
        assert!(state.is_partner_key("key-a"));
        assert!(!state.is_partner_key("key-c"));
    }
}
