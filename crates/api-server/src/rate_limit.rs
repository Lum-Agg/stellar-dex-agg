use {
    axum::{
        extract::{ConnectInfo, Request, State},
        http::StatusCode,
        middleware::Next,
        response::{IntoResponse, Response},
    },
    std::{
        collections::{HashMap, VecDeque},
        net::{IpAddr, SocketAddr},
        sync::{Arc, Mutex},
        time::{Duration, Instant},
    },
};

#[derive(Clone)]
pub struct IpRateLimiter {
    inner: Arc<Mutex<HashMap<IpAddr, VecDeque<Instant>>>>,
    limit: usize,
    window: Duration,
}

impl IpRateLimiter {
    pub fn new(limit: usize, window: Duration) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            limit,
            window,
        }
    }

    fn allow_now(&self, ip: IpAddr, now: Instant) -> bool {
        let mut store = self.inner.lock().expect("ip rate limiter mutex poisoned");
        let entries = store.entry(ip).or_default();
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

pub async fn ip_rate_limit_middleware(State(limiter): State<IpRateLimiter>, request: Request, next: Next) -> Response {
    if let Some(ConnectInfo(addr)) = request.extensions().get::<ConnectInfo<SocketAddr>>() {
        if !limiter.allow_now(addr.ip(), Instant::now()) {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                "rate limit exceeded: max 10 requests/second per IP",
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
}
