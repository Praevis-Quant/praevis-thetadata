//! Request admission shared by clones of a client (P1 PR 4b): the tier's
//! concurrency, a per-minute request limit and retry with jittered
//! exponential backoff on `RESOURCE_EXHAUSTED` and `UNAVAILABLE` only. A new
//! client sizes it from the account's tier in the login response.
use crate::eod::QueryError;
use std::{collections::VecDeque, sync::Arc, time::Duration};
use tokio::{
    sync::{Mutex, OwnedSemaphorePermit, Semaphore},
    time::{Instant, sleep_until},
};

/// A subscription tier, ordered from FREE to PRO.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tier {
    Free,
    Value,
    Standard,
    Pro,
}
impl Tier {
    /// One subscription field of the login response: a number (0 FREE,
    /// 1 VALUE, 2 STANDARD, 3 PRO) or the tier's name. Anything else is `None`.
    ///
    /// Options PRO was observed as 3 and FREE as 0 (2026-11-04); 1 and 2 follow
    /// the order and are not observed.
    pub fn from_subscription(value: &serde_json::Value) -> Option<Self> {
        let tier = |n: u64| match n {
            0 => Some(Self::Free),
            1 => Some(Self::Value),
            2 => Some(Self::Standard),
            3 => Some(Self::Pro),
            _ => None,
        };
        match value {
            serde_json::Value::Number(n) => n.as_u64().and_then(tier),
            serde_json::Value::String(text) => match text.to_ascii_uppercase().as_str() {
                "FREE" => Some(Self::Free),
                "VALUE" => Some(Self::Value),
                "STANDARD" => Some(Self::Standard),
                "PRO" | "PROFESSIONAL" => Some(Self::Pro),
                _ => text.parse().ok().and_then(tier),
            },
            _ => None,
        }
    }
    /// The account's tier for request limits: the highest of its stock,
    /// options and indices subscriptions, since the vendor sets concurrency
    /// account-wide from the highest tier. FREE when none is recognised.
    pub fn of_account(user: &praevis_thetadata_auth::User) -> Self {
        [
            &user.stock_subscription,
            &user.options_subscription,
            &user.indices_subscription,
        ]
        .into_iter()
        .filter_map(Self::from_subscription)
        .max()
        .unwrap_or(Self::Free)
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Free => "FREE",
            Self::Value => "VALUE",
            Self::Standard => "STANDARD",
            Self::Pro => "PRO",
        }
    }
}

/// Limits applied to every `query()` request of a client and its clones.
///
/// A new client uses [`SchedulerPolicy::for_tier`] with the account's tier;
/// [`ThetaClient::with_scheduler`](crate::ThetaClient::with_scheduler)
/// overrides it. The `Default` is the FREE tier's **documented** limits: 1
/// concurrent request and 20 per minute (design decision D5). P0 observed FREE
/// serving 3 at once and no limit at 40 per minute; those are available only
/// as an explicit choice ([`SchedulerPolicy::free_observed`]).
#[derive(Debug, Clone)]
pub struct SchedulerPolicy {
    /// Concurrent requests (1..=64). A request holds its slot until its
    /// stream ends, fails or is cancelled.
    pub concurrency: usize,
    /// Requests started in any 60-second window; `None` for no limit.
    pub per_minute: Option<u32>,
    /// Retries after `RESOURCE_EXHAUSTED` or `UNAVAILABLE` at open.
    pub retries: u32,
    /// First backoff; each retry doubles it, up to `max_backoff`, with full
    /// jitter. Every retry also counts against `per_minute`.
    pub backoff: Duration,
    pub max_backoff: Duration,
}
impl Default for SchedulerPolicy {
    fn default() -> Self {
        Self {
            concurrency: 1,
            per_minute: Some(20),
            retries: 3,
            backoff: Duration::from_secs(1),
            max_backoff: Duration::from_secs(30),
        }
    }
}
impl SchedulerPolicy {
    /// The vendor's documented limits for `tier`: FREE 1 concurrent request
    /// and 20 per minute; VALUE 2, STANDARD 4 and PRO 8 concurrent requests
    /// with no per-minute limit ("not rate-limited in any way", concurrency
    /// only; re-verified 2026-11-05).
    pub fn for_tier(tier: Tier) -> Self {
        let concurrency = match tier {
            Tier::Free => return Self::default(),
            Tier::Value => 2,
            Tier::Standard => 4,
            Tier::Pro => 8,
        };
        Self {
            concurrency,
            per_minute: None,
            ..Self::default()
        }
    }
    /// FREE as observed in P0 (3 concurrent, 40 per minute): faster, but not
    /// guaranteed by the vendor. Opt in explicitly.
    pub fn free_observed() -> Self {
        Self {
            concurrency: 3,
            per_minute: Some(40),
            ..Self::default()
        }
    }
    fn validate(&self) -> Result<(), QueryError> {
        if !(1..=64).contains(&self.concurrency)
            || self.per_minute == Some(0)
            || self.backoff.is_zero()
            || self.max_backoff < self.backoff
        {
            return Err(QueryError::Configuration);
        }
        Ok(())
    }
}

const WINDOW: Duration = Duration::from_secs(60);

pub(crate) struct Scheduler {
    pub(crate) policy: SchedulerPolicy,
    slots: Arc<Semaphore>,
    /// Start times of the requests in the current window, oldest first.
    starts: Mutex<VecDeque<Instant>>,
    seed: std::sync::atomic::AtomicU64,
}

impl Scheduler {
    pub(crate) fn new(policy: SchedulerPolicy) -> Result<Arc<Self>, QueryError> {
        policy.validate()?;
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0x9e37_79b9, |d| d.as_nanos() as u64)
            | 1;
        Ok(Arc::new(Self {
            slots: Arc::new(Semaphore::new(policy.concurrency)),
            starts: Mutex::new(VecDeque::new()),
            seed: seed.into(),
            policy,
        }))
    }

    /// Wait for a concurrency slot, then for room in the per-minute window,
    /// both before `deadline`. The slot is returned to the caller.
    pub(crate) async fn admit(
        &self,
        deadline: Instant,
    ) -> Result<OwnedSemaphorePermit, QueryError> {
        let slot = tokio::time::timeout_at(deadline, self.slots.clone().acquire_owned())
            .await
            .map_err(|_| QueryError::Deadline)?
            .map_err(|_| QueryError::Cancelled)?;
        if let Some(limit) = self.policy.per_minute {
            loop {
                let mut starts = self.starts.lock().await;
                let now = Instant::now();
                while starts
                    .front()
                    .is_some_and(|t| now.duration_since(*t) >= WINDOW)
                {
                    starts.pop_front();
                }
                if starts.len() < limit as usize {
                    starts.push_back(now);
                    break;
                }
                let free_at = *starts.front().expect("the window is full") + WINDOW;
                drop(starts);
                if free_at >= deadline {
                    return Err(QueryError::Deadline);
                }
                sleep_until(free_at).await;
            }
        }
        Ok(slot)
    }

    /// Backoff before retry `attempt` (1-based): full jitter over the doubled
    /// base, capped. A small xorshift avoids a random-number dependency.
    pub(crate) fn backoff(&self, attempt: u32) -> Duration {
        let cap = self
            .policy
            .backoff
            .saturating_mul(1 << attempt.saturating_sub(1).min(16))
            .min(self.policy.max_backoff);
        let mut x = self.seed.load(std::sync::atomic::Ordering::Relaxed);
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.seed.store(x, std::sync::atomic::Ordering::Relaxed);
        cap.mul_f64((x % 10_000) as f64 / 10_000.0)
    }
}

/// Only these remote statuses are retried, and only at open.
pub(crate) fn retryable(error: QueryError) -> bool {
    matches!(
        error,
        QueryError::Remote(tonic::Code::ResourceExhausted | tonic::Code::Unavailable)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn concurrency_and_per_minute_limits_admit_in_order() {
        let scheduler = Scheduler::new(SchedulerPolicy {
            concurrency: 2,
            per_minute: Some(3),
            ..Default::default()
        })
        .unwrap();
        let start = Instant::now();
        let far = start + Duration::from_secs(3600);
        let a = scheduler.admit(far).await.unwrap();
        let b = scheduler.admit(far).await.unwrap();
        // A third waits for a slot.
        assert!(
            tokio::time::timeout(Duration::from_secs(1), scheduler.admit(far))
                .await
                .is_err()
        );
        drop((a, b));
        let c = scheduler.admit(far).await.unwrap();
        // A fourth start inside the window waits for the oldest to age out.
        drop(c);
        let d = scheduler.admit(far).await.unwrap();
        assert!(start.elapsed() >= WINDOW, "{:?}", start.elapsed());
        drop(d);
        // The deadline bounds both waits.
        let now = Instant::now();
        for _ in 0..2 {
            drop(scheduler.admit(far).await.unwrap());
        }
        assert_eq!(
            scheduler.admit(now + Duration::from_secs(1)).await.err(),
            Some(QueryError::Deadline)
        );
    }

    #[test]
    fn backoff_doubles_with_full_jitter_up_to_the_cap() {
        let scheduler = Scheduler::new(SchedulerPolicy {
            backoff: Duration::from_millis(100),
            max_backoff: Duration::from_millis(350),
            ..Default::default()
        })
        .unwrap();
        for (attempt, cap) in [(1, 100), (2, 200), (3, 350), (10, 350)] {
            for _ in 0..50 {
                assert!(scheduler.backoff(attempt) <= Duration::from_millis(cap));
            }
        }
        assert!(retryable(QueryError::Remote(tonic::Code::Unavailable)));
        assert!(retryable(QueryError::Remote(
            tonic::Code::ResourceExhausted
        )));
        for other in [
            QueryError::NoData,
            QueryError::Remote(tonic::Code::PermissionDenied),
            QueryError::Remote(tonic::Code::InvalidArgument),
            QueryError::Deadline,
        ] {
            assert!(!retryable(other));
        }
        for bad in [
            SchedulerPolicy {
                concurrency: 0,
                ..Default::default()
            },
            SchedulerPolicy {
                per_minute: Some(0),
                ..Default::default()
            },
            SchedulerPolicy {
                backoff: Duration::ZERO,
                ..Default::default()
            },
        ] {
            assert!(Scheduler::new(bad).is_err());
        }
        assert_eq!(SchedulerPolicy::free_observed().concurrency, 3);
        assert_eq!(SchedulerPolicy::default().per_minute, Some(20));
    }

    #[test]
    fn the_tier_comes_from_the_highest_subscription() {
        use serde_json::json;
        for (value, tier) in [
            (json!(0), Some(Tier::Free)),
            (json!(1), Some(Tier::Value)),
            (json!(2), Some(Tier::Standard)),
            (json!(3), Some(Tier::Pro)),
            (json!("PRO"), Some(Tier::Pro)),
            (json!("standard"), Some(Tier::Standard)),
            (json!("3"), Some(Tier::Pro)),
            (json!(4), None),
            (json!(-1), None),
            (json!(1.5), None),
            (json!(null), None),
            (json!({"tier": 3}), None),
        ] {
            assert_eq!(Tier::from_subscription(&value), tier, "{value}");
        }
        let user = |stock, options, indices| praevis_thetadata_auth::User {
            email: String::new(),
            stock_subscription: stock,
            options_subscription: options,
            indices_subscription: indices,
        };
        // The observed account: options PRO, stocks and indices FREE.
        assert_eq!(
            Tier::of_account(&user(json!(0), json!(3), json!(0))),
            Tier::Pro
        );
        assert_eq!(
            Tier::of_account(&user(json!(null), json!("odd"), json!(1))),
            Tier::Value
        );
        assert_eq!(
            Tier::of_account(&user(json!(null), json!(null), json!(null))),
            Tier::Free
        );
        let limits = |tier| {
            let p = SchedulerPolicy::for_tier(tier);
            (p.concurrency, p.per_minute)
        };
        assert_eq!(limits(Tier::Free), (1, Some(20)));
        assert_eq!(limits(Tier::Value), (2, None));
        assert_eq!(limits(Tier::Standard), (4, None));
        assert_eq!(limits(Tier::Pro), (8, None));
    }
}
