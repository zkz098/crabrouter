use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const MAX_FAILED_ATTEMPTS: u32 = 5;
const WINDOW: Duration = Duration::from_secs(60);

struct AttemptBucket {
    failures: u32,
    window_start: Instant,
}

static LOGIN_ATTEMPTS: OnceLock<Mutex<HashMap<String, AttemptBucket>>> = OnceLock::new();

fn table() -> &'static Mutex<HashMap<String, AttemptBucket>> {
    LOGIN_ATTEMPTS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Returns `Ok(())` if the username is not currently rate-limited, or `Err(retry_after_secs)`.
pub fn check_login_allowed(key: &str) -> Result<(), u64> {
    let mut guard = table()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let now = Instant::now();
    guard.retain(|_, v| now.duration_since(v.window_start) < WINDOW);

    if let Some(bucket) = guard.get(key)
        && bucket.failures >= MAX_FAILED_ATTEMPTS
    {
        let elapsed = now.duration_since(bucket.window_start);
        let remaining = WINDOW.saturating_sub(elapsed).as_secs().max(1);
        return Err(remaining);
    }
    Ok(())
}

/// Records a failed login attempt for `key`.
pub fn record_login_failure(key: &str) {
    let mut guard = table()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let now = Instant::now();
    let entry = guard.entry(key.to_string()).or_insert(AttemptBucket {
        failures: 0,
        window_start: now,
    });
    if now.duration_since(entry.window_start) >= WINDOW {
        entry.failures = 1;
        entry.window_start = now;
    } else {
        entry.failures = entry.failures.saturating_add(1);
    }
}

/// Clears failed login attempts for `key` after a successful authentication.
pub fn clear_login_failures(key: &str) {
    let mut guard = table()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    guard.remove(key);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limiter_blocks_after_five_failures_and_clears() {
        let user = "rate_limit_test_user";
        clear_login_failures(user);
        for _ in 0..5 {
            assert!(check_login_allowed(user).is_ok());
            record_login_failure(user);
        }
        assert!(check_login_allowed(user).is_err());
        clear_login_failures(user);
        assert!(check_login_allowed(user).is_ok());
    }
}
