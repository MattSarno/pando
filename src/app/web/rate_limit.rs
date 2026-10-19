use axum::http::HeaderMap;
use std::{
    collections::HashMap,
    net::IpAddr,
    sync::Mutex,
    time::{Duration, Instant},
};

const FLY_CLIENT_IP_HEADER: &str = "Fly-Client-IP";
const MAX_TRACKED_IPS: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Bucket {
    Register,
    Token,
    Authorize,
    PassphraseFailure,
}

#[derive(Debug)]
pub struct RateLimitConfig {
    pub window: Duration,
    pub register_max: u32,
    pub token_max: u32,
    pub authorize_max: u32,
    pub passphrase_failure_max: u32,
    pub passphrase_lockout: Duration,
}

#[derive(Debug)]
pub struct RateLimiter {
    config: RateLimitConfig,
    windows: Mutex<HashMap<(Bucket, IpAddr), Window>>,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            windows: Mutex::new(HashMap::new()),
        }
    }

    pub fn check(&self, bucket: Bucket, ip: IpAddr) -> bool {
        let (max_requests, window_duration) = self.limit_for(bucket);
        let mut windows = self
            .windows
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let key = (bucket, ip);
        let now = Instant::now();

        if windows.len() >= MAX_TRACKED_IPS {
            windows.retain(|(bucket, _), window| {
                let (_, duration) = self.limit_for(*bucket);
                now.duration_since(window.start) <= duration
            });

            if windows.len() >= MAX_TRACKED_IPS {
                windows.clear();
            }
        }

        let window = windows.entry(key).or_insert(Window {
            count: 0,
            start: now,
        });

        if now.duration_since(window.start) > window_duration {
            window.count = 0;
            window.start = now;
        }

        if window.count < max_requests {
            window.count += 1;
            true
        } else {
            eprintln!("rate_limit: {bucket:?} limit reached for {ip}");
            false
        }
    }

    pub fn is_locked_out(&self, ip: IpAddr) -> bool {
        let (max_failures, lockout) = self.limit_for(Bucket::PassphraseFailure);
        let windows = self
            .windows
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        let locked_out = windows
            .get(&(Bucket::PassphraseFailure, ip))
            .is_some_and(|window| {
                window.start.elapsed() <= lockout && window.count >= max_failures
            });

        if locked_out {
            eprintln!("rate_limit: passphrase lockout active for {ip}");
        }

        locked_out
    }

    pub fn record_passphrase_failure(&self, ip: IpAddr) {
        self.check(Bucket::PassphraseFailure, ip);
    }

    pub fn clear_passphrase_failures(&self, ip: IpAddr) {
        self.windows
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&(Bucket::PassphraseFailure, ip));
    }

    fn limit_for(&self, bucket: Bucket) -> (u32, Duration) {
        match bucket {
            Bucket::Register => (self.config.register_max, self.config.window),
            Bucket::Token => (self.config.token_max, self.config.window),
            Bucket::Authorize => (self.config.authorize_max, self.config.window),
            Bucket::PassphraseFailure => (
                self.config.passphrase_failure_max,
                self.config.passphrase_lockout,
            ),
        }
    }
}

#[derive(Debug)]
struct Window {
    count: u32,
    start: Instant,
}

pub fn client_ip(headers: &HeaderMap) -> IpAddr {
    headers
        .get(FLY_CLIENT_IP_HEADER)
        .and_then(|val| val.to_str().ok())
        .and_then(|str_val| str_val.parse().ok())
        .unwrap_or_else(|| IpAddr::from([127, 0, 0, 1]))
}
