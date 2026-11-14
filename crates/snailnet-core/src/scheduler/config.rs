//! Scheduler configuration: what the rules say, validated, plus the live
//! handle the service uses to change it without stopping traffic (PLAN.md
//! §9.6, §9.7).

use std::sync::Arc;

use arc_swap::ArcSwap;

use super::shares::{OTHER_FLOOR_BP, TOTAL_BP};

/// Who goes first (PLAN.md D8). Priority decides dequeue order, never
/// long-run bandwidth: a class can only spend credit its share has earned.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    Low,
    #[default]
    Normal,
    High,
}

impl Priority {
    /// Flow weight inside a class (PLAN.md §9.3): High 4 : Normal 2 : Low 1.
    pub(crate) fn flow_weight(self) -> i64 {
        match self {
            Self::High => 4,
            Self::Normal => 2,
            Self::Low => 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassConfig {
    /// Unique within a config. Live reconfiguration matches classes by name,
    /// so a renamed rule starts with fresh state.
    pub name: String,
    /// Guaranteed share in basis points. 0 means "only when free": the class
    /// gets a token weight and Low priority (PLAN.md §9.7).
    pub share_bp: u32,
    pub prio: Priority,
    /// Optional hard cap (PLAN.md D2).
    pub ceil_bytes_per_sec: Option<u64>,
}

impl ClassConfig {
    /// A Normal-priority class with no ceiling.
    pub fn new(name: impl Into<String>, share_bp: u32) -> Self {
        Self {
            name: name.into(),
            share_bp,
            prio: Priority::Normal,
            ceil_bytes_per_sec: None,
        }
    }
}

/// One direction's tree (PLAN.md §9.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchedulerConfig {
    /// Shaped rate of the whole direction.
    pub rate_bytes_per_sec: u64,
    /// Root token bucket depth: how much may leave back to back.
    pub burst_bytes: u64,
    /// Rules in order, then "Everything else" **last**. The last class is
    /// where unclassified traffic and packets of removed rules go. Shares
    /// must form a valid share list (see [`super::shares`]).
    pub classes: Vec<ClassConfig>,
}

impl SchedulerConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        let Some(other) = self.classes.last() else {
            return Err(ConfigError::NoClasses);
        };
        if self.rate_bytes_per_sec == 0 {
            return Err(ConfigError::ZeroRate);
        }
        for (i, c) in self.classes.iter().enumerate() {
            if self.classes[..i].iter().any(|p| p.name == c.name) {
                return Err(ConfigError::DuplicateName(c.name.clone()));
            }
            if c.ceil_bytes_per_sec == Some(0) {
                return Err(ConfigError::ZeroCeil(c.name.clone()));
            }
        }
        let sum: u64 = self.classes.iter().map(|c| c.share_bp as u64).sum();
        if sum != TOTAL_BP as u64 {
            return Err(ConfigError::SharesDontSum(sum));
        }
        if other.share_bp < OTHER_FLOOR_BP {
            return Err(ConfigError::OtherBelowFloor(other.share_bp));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    NoClasses,
    ZeroRate,
    ZeroCeil(String),
    DuplicateName(String),
    SharesDontSum(u64),
    OtherBelowFloor(u32),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoClasses => write!(f, "at least one class (Everything else) is required"),
            Self::ZeroRate => write!(f, "rate must be greater than zero"),
            Self::ZeroCeil(n) => write!(f, "class {n:?} has a ceiling of 0"),
            Self::DuplicateName(n) => write!(f, "class name {n:?} is used twice"),
            Self::SharesDontSum(s) => write!(f, "shares add up to {s} bp, not {TOTAL_BP}"),
            Self::OtherBelowFloor(s) => write!(
                f,
                "Everything else has {s} bp, below the {OTHER_FLOOR_BP} bp floor"
            ),
        }
    }
}

impl std::error::Error for ConfigError {}

/// The current config for one direction, shared between the thread that
/// edits rules and the scheduler thread. Publishing is lock-free; the
/// scheduler picks the change up on its next [`super::Scheduler::sync`].
#[derive(Debug)]
pub struct LiveConfig {
    current: ArcSwap<SchedulerConfig>,
}

impl LiveConfig {
    pub fn new(config: SchedulerConfig) -> Result<Self, ConfigError> {
        config.validate()?;
        Ok(Self {
            current: ArcSwap::from_pointee(config),
        })
    }

    /// Replace the config. Invalid configs are rejected here, so the
    /// scheduler thread never sees one.
    pub fn publish(&self, config: SchedulerConfig) -> Result<(), ConfigError> {
        config.validate()?;
        self.current.store(Arc::new(config));
        Ok(())
    }

    pub fn load(&self) -> Arc<SchedulerConfig> {
        self.current.load_full()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(classes: Vec<ClassConfig>) -> SchedulerConfig {
        SchedulerConfig {
            rate_bytes_per_sec: 1,
            burst_bytes: 0,
            classes,
        }
    }

    #[test]
    fn validation() {
        assert_eq!(cfg(vec![]).validate(), Err(ConfigError::NoClasses));
        assert_eq!(cfg(vec![ClassConfig::new("o", 10_000)]).validate(), Ok(()));
        assert_eq!(
            cfg(vec![
                ClassConfig::new("a", 5000),
                ClassConfig::new("o", 4000)
            ])
            .validate(),
            Err(ConfigError::SharesDontSum(9000))
        );
        assert_eq!(
            cfg(vec![
                ClassConfig::new("a", 9600),
                ClassConfig::new("o", 400)
            ])
            .validate(),
            Err(ConfigError::OtherBelowFloor(400))
        );
        assert_eq!(
            cfg(vec![
                ClassConfig::new("a", 5000),
                ClassConfig::new("a", 5000)
            ])
            .validate(),
            Err(ConfigError::DuplicateName("a".into()))
        );
        let mut capped = ClassConfig::new("o", 10_000);
        capped.ceil_bytes_per_sec = Some(0);
        assert_eq!(
            cfg(vec![capped]).validate(),
            Err(ConfigError::ZeroCeil("o".into()))
        );
        let mut zero_rate = cfg(vec![ClassConfig::new("o", 10_000)]);
        zero_rate.rate_bytes_per_sec = 0;
        assert_eq!(zero_rate.validate(), Err(ConfigError::ZeroRate));
    }

    #[test]
    fn live_config_rejects_invalid_and_keeps_old() {
        let live = LiveConfig::new(cfg(vec![ClassConfig::new("o", 10_000)])).unwrap();
        let before = live.load();
        assert!(live.publish(cfg(vec![])).is_err());
        assert!(Arc::ptr_eq(&before, &live.load()));
        live.publish(cfg(vec![
            ClassConfig::new("a", 5000),
            ClassConfig::new("o", 5000),
        ]))
        .unwrap();
        assert_eq!(live.load().classes.len(), 2);
    }

    #[test]
    fn priority_order() {
        assert!(Priority::High > Priority::Normal && Priority::Normal > Priority::Low);
    }
}
