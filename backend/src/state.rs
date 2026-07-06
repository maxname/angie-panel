use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use sqlx::SqlitePool;
use tokio::sync::Semaphore;

use crate::config::PanelConfig;

/// Fixed-window rate limiter state.
pub struct Window {
    count: u32,
    start: Instant,
}

pub struct RateLimiter {
    windows: Mutex<HashMap<IpAddr, Window>>,
    max: u32,
    period: Duration,
}

impl RateLimiter {
    pub fn new(max: u32, period: Duration) -> Self {
        Self {
            windows: Mutex::new(HashMap::new()),
            max,
            period,
        }
    }

    /// Register an attempt; returns false when the caller is over the limit.
    pub fn check(&self, ip: IpAddr) -> bool {
        let mut windows = self.windows.lock().unwrap();
        let now = Instant::now();
        // Opportunistic cleanup so the map cannot grow unboundedly.
        if windows.len() > 1024 {
            let period = self.period;
            windows.retain(|_, w| now.duration_since(w.start) < period);
        }
        let w = windows.entry(ip).or_insert(Window {
            count: 0,
            start: now,
        });
        if now.duration_since(w.start) >= self.period {
            w.count = 0;
            w.start = now;
        }
        w.count += 1;
        w.count <= self.max
    }
}

pub struct AppState {
    pub cfg: PanelConfig,
    pub cfg_path: PathBuf,
    pub db: SqlitePool,
    /// Bounds concurrent argon2 hashing (memory-hard; see PLAN.md §6).
    pub argon_sem: Semaphore,
    pub login_limiter: RateLimiter,
    pub setup_limiter: RateLimiter,
    /// Hostnames accepted in the Host header; empty set = check disabled.
    pub allowed_hostnames: HashSet<String>,
    /// Set after the first configtest attempt through systemd (true = polkit
    /// authorized the unit start); None until then.
    pub polkit_ok: Mutex<Option<bool>>,
    pub http_client: reqwest::Client,
}

impl AppState {
    pub fn new(cfg: PanelConfig, cfg_path: PathBuf, db: SqlitePool) -> Self {
        let mut allowed: HashSet<String> = HashSet::new();
        let unspecified = matches!(cfg.bind_addr.as_str(), "0.0.0.0" | "::");
        if !unspecified {
            allowed.insert(cfg.bind_addr.clone());
            allowed.insert("localhost".into());
            allowed.insert("127.0.0.1".into());
        }
        for h in &cfg.allowed_hosts {
            allowed.insert(h.clone());
        }
        if allowed.is_empty() {
            tracing::warn!(
                "bind_addr is unspecified and allowed_hosts is empty: \
                 Host-header checking is DISABLED (DNS-rebinding protection off)"
            );
        }
        Self {
            cfg,
            cfg_path,
            db,
            argon_sem: Semaphore::new(2),
            login_limiter: RateLimiter::new(10, Duration::from_secs(15 * 60)),
            setup_limiter: RateLimiter::new(10, Duration::from_secs(15 * 60)),
            allowed_hostnames: allowed,
            polkit_ok: Mutex::new(None),
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(3))
                .build()
                .expect("http client"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limiter_blocks_after_max() {
        let rl = RateLimiter::new(3, Duration::from_secs(60));
        let ip: IpAddr = "10.0.0.1".parse().unwrap();
        assert!(rl.check(ip));
        assert!(rl.check(ip));
        assert!(rl.check(ip));
        assert!(!rl.check(ip));
        // A different IP is unaffected.
        assert!(rl.check("10.0.0.2".parse().unwrap()));
    }
}
