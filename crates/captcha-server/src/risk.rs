//! IP 风控：动态难度调整 + IP 黑白名单。

use std::collections::VecDeque;
use std::net::IpAddr;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use ipnet::IpNet;

/// 风控配置（从 captcha.toml [risk] 段加载）。
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(default)]
pub struct RiskConfig {
    /// 启用动态难度
    pub dynamic_diff_enabled: bool,
    /// 最大额外 diff 增量
    pub dynamic_diff_max_increase: u8,
    /// 滑动窗口大小（最近 N 次请求）
    pub window_size: usize,
    /// 触发加难度的失败率阈值（0.0 ~ 1.0）
    pub fail_rate_threshold: f64,
    /// 黑名单 IP/CIDR（直接拒绝）
    pub blocked_ips: Vec<String>,
    /// 白名单 IP/CIDR（跳过难度调整 + 限流）
    pub allowed_ips: Vec<String>,
}

impl Default for RiskConfig {
    fn default() -> Self {
        Self {
            dynamic_diff_enabled: false,
            dynamic_diff_max_increase: 4,
            window_size: 20,
            fail_rate_threshold: 0.7,
            blocked_ips: Vec::new(),
            allowed_ips: Vec::new(),
        }
    }
}

#[derive(Debug)]
struct IpRecord {
    outcomes: VecDeque<(Instant, bool)>,
}

/// IP 风控追踪器。
pub struct RiskTracker {
    records: DashMap<IpAddr, IpRecord>,
    blocked_nets: Vec<IpNet>,
    allowed_nets: Vec<IpNet>,
    config: RiskConfig,
}

impl RiskTracker {
    pub fn new(config: RiskConfig) -> Self {
        let blocked_nets = parse_nets(&config.blocked_ips);
        let allowed_nets = parse_nets(&config.allowed_ips);
        Self {
            records: DashMap::new(),
            blocked_nets,
            allowed_nets,
            config,
        }
    }

    /// IP 是否在黑名单中。
    pub fn is_blocked(&self, ip: IpAddr) -> bool {
        self.blocked_nets.iter().any(|net| net.contains(&ip))
    }

    /// IP 是否在白名单中。
    pub fn is_allowed(&self, ip: IpAddr) -> bool {
        self.allowed_nets.iter().any(|net| net.contains(&ip))
    }

    /// 根据 IP 历史计算额外 diff 增量。
    pub fn extra_diff(&self, ip: IpAddr) -> u8 {
        if !self.config.dynamic_diff_enabled || self.is_allowed(ip) {
            return 0;
        }

        let Some(entry) = self.records.get(&ip) else {
            return 0;
        };
        let rec = entry.value();
        if rec.outcomes.is_empty() {
            return 0;
        }

        let fails = rec.outcomes.iter().filter(|(_, ok)| !ok).count();
        let rate = fails as f64 / rec.outcomes.len() as f64;

        if rate < self.config.fail_rate_threshold {
            return 0;
        }

        // 失败率越高 → 增量越大（线性映射到 max_increase）
        let severity =
            (rate - self.config.fail_rate_threshold) / (1.0 - self.config.fail_rate_threshold);
        let increase = (severity * self.config.dynamic_diff_max_increase as f64).ceil() as u8;
        increase.min(self.config.dynamic_diff_max_increase)
    }

    /// 记录一次 verify 结果。
    pub fn record_verify(&self, ip: IpAddr, success: bool) {
        if !self.config.dynamic_diff_enabled {
            return;
        }
        let mut entry = self.records.entry(ip).or_insert_with(|| IpRecord {
            outcomes: VecDeque::new(),
        });
        let rec = entry.value_mut();
        rec.outcomes.push_back((Instant::now(), success));
        while rec.outcomes.len() > self.config.window_size {
            rec.outcomes.pop_front();
        }
    }

    /// 清理超过 10 分钟无活动的 IP 记录，防内存泄漏。
    pub fn cleanup_stale(&self) -> usize {
        let cutoff = Instant::now() - Duration::from_secs(600);
        let before = self.records.len();
        self.records.retain(|_, rec| {
            rec.outcomes
                .back()
                .map(|(t, _)| *t > cutoff)
                .unwrap_or(false)
        });
        before.saturating_sub(self.records.len())
    }

    /// 更新配置（热重载时调用）。
    pub fn update_config(&mut self, config: RiskConfig) {
        self.blocked_nets = parse_nets(&config.blocked_ips);
        self.allowed_nets = parse_nets(&config.allowed_ips);
        self.config = config;
    }
}

fn parse_nets(strs: &[String]) -> Vec<IpNet> {
    strs.iter()
        .filter_map(|s| {
            // 尝试解析为 CIDR，失败则当作单个 IP
            s.parse::<IpNet>()
                .or_else(|_| s.parse::<IpAddr>().map(IpNet::from))
                .map_err(|e| tracing::warn!("无法解析 IP/CIDR '{}': {e}", s))
                .ok()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_tracker() -> RiskTracker {
        RiskTracker::new(RiskConfig {
            dynamic_diff_enabled: true,
            dynamic_diff_max_increase: 4,
            window_size: 10,
            fail_rate_threshold: 0.7,
            blocked_ips: vec!["10.0.0.0/8".into(), "192.168.1.100".into()],
            allowed_ips: vec!["127.0.0.1".into()],
        })
    }

    #[test]
    fn blocked_ip() {
        let t = default_tracker();
        assert!(t.is_blocked("10.1.2.3".parse().unwrap()));
        assert!(t.is_blocked("192.168.1.100".parse().unwrap()));
        assert!(!t.is_blocked("8.8.8.8".parse().unwrap()));
    }

    #[test]
    fn allowed_ip() {
        let t = default_tracker();
        assert!(t.is_allowed("127.0.0.1".parse().unwrap()));
        assert!(!t.is_allowed("8.8.8.8".parse().unwrap()));
    }

    #[test]
    fn no_history_no_extra_diff() {
        let t = default_tracker();
        assert_eq!(t.extra_diff("8.8.8.8".parse().unwrap()), 0);
    }

    #[test]
    fn high_fail_rate_increases_diff() {
        let t = default_tracker();
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        for _ in 0..10 {
            t.record_verify(ip, false);
        }
        // 100% fail rate → max increase
        assert_eq!(t.extra_diff(ip), 4);
    }

    #[test]
    fn low_fail_rate_no_increase() {
        let t = default_tracker();
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        for _ in 0..8 {
            t.record_verify(ip, true);
        }
        for _ in 0..2 {
            t.record_verify(ip, false);
        }
        // 20% fail rate < 70% threshold
        assert_eq!(t.extra_diff(ip), 0);
    }

    #[test]
    fn allowed_ip_skips_diff() {
        let t = default_tracker();
        let ip: IpAddr = "127.0.0.1".parse().unwrap();
        for _ in 0..10 {
            t.record_verify(ip, false);
        }
        assert_eq!(t.extra_diff(ip), 0);
    }

    #[test]
    fn window_evicts_old() {
        let t = default_tracker();
        let ip: IpAddr = "5.5.5.5".parse().unwrap();
        // 先 10 次全失败
        for _ in 0..10 {
            t.record_verify(ip, false);
        }
        assert_eq!(t.extra_diff(ip), 4);
        // 再 10 次全成功 → 窗口滑出旧数据
        for _ in 0..10 {
            t.record_verify(ip, true);
        }
        assert_eq!(t.extra_diff(ip), 0);
    }
}
