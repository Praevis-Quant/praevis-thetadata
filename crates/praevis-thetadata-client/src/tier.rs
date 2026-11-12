//! The account's subscription tier, read from the login response. It is
//! information: the library applies no account-wide scheduling, rate limit or
//! retry (ADR-0021). A job's default concurrency is the tier's documented
//! limit.

/// A subscription tier, ordered from FREE to PRO.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
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
    /// The vendor's documented concurrent-request limit for this tier:
    /// FREE 1, VALUE 2, STANDARD 4 and PRO 8. It is only the default of a
    /// job's own concurrency ([`RequestOptions::concurrency`](crate::RequestOptions)),
    /// so one job alone stays within the plan; the library does not
    /// coordinate requests across calls or callers.
    pub fn concurrency(self) -> usize {
        match self {
            Self::Free => 1,
            Self::Value => 2,
            Self::Standard => 4,
            Self::Pro => 8,
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_documented_concurrency_follows_the_tier() {
        let limits: Vec<_> = [Tier::Free, Tier::Value, Tier::Standard, Tier::Pro]
            .into_iter()
            .map(Tier::concurrency)
            .collect();
        assert_eq!(limits, [1, 2, 4, 8]);
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
        // `User` is non-exhaustive: build it the way the auth crate does, from
        // the login response's JSON.
        let user = |stock, options, indices| -> praevis_thetadata_auth::User {
            serde_json::from_value(json!({
                "email": "",
                "stockSubscription": stock,
                "optionsSubscription": options,
                "indicesSubscription": indices,
            }))
            .unwrap()
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
    }
}
