use std::time::Duration;
use tokio::time::sleep;

/// Returns a vector of backoff durations for the number of `attempts`.
/// The first entry is the first delay (attempt 1 failure => wait durations[0]).
pub fn backoff_durations(attempts: usize, base: Duration, factor: f64) -> Vec<Duration> {
    let mut out = Vec::with_capacity(attempts);
    for i in 0..attempts {
        let exp = factor.powi(i as i32);
        let millis = (base.as_millis() as f64 * exp).round() as u64;
        out.push(Duration::from_millis(millis));
    }
    out
}

/// Retry an async operation with backoff. `operation` should return a Result.
/// This will attempt the operation up to `attempts` times (including the first).
pub async fn retry_with_backoff<F, Fut, T, E>(
    attempts: usize,
    base: Duration,
    factor: f64,
    mut operation: F,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
{
    if attempts == 0 {
        // No attempts requested
        return operation().await;
    }

    let delays = backoff_durations(attempts - 1, base, factor);

    // First attempt (no delay)
    match operation().await {
        Ok(v) => return Ok(v),
        Err(e) => {
            // Fall through to retries
            let mut last_err = e;

            for d in delays {
                sleep(d).await;
                match operation().await {
                    Ok(v) => return Ok(v),
                    Err(e2) => last_err = e2,
                }
            }
            Err(last_err)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn test_backoff_durations_basic() {
        let d = backoff_durations(4, Duration::from_millis(100), 2.0);
        assert_eq!(d.len(), 4);
        assert_eq!(d[0], Duration::from_millis(100));
        assert_eq!(d[1], Duration::from_millis(200));
        assert_eq!(d[2], Duration::from_millis(400));
        assert_eq!(d[3], Duration::from_millis(800));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_retry_with_backoff_eventual_success() {
        // freeze time so sleeps don't actually delay the test wall clock
        tokio::time::pause();

        let attempts = 4;
        let base = Duration::from_millis(10);
        let factor = 2.0;

        let counter = Arc::new(AtomicUsize::new(0));
        let c = counter.clone();

        // operation fails twice then succeeds
        let op = move || {
            let c = c.clone();
            async move {
                let n = c.fetch_add(1, Ordering::SeqCst);
                if n < 2 {
                    Err("fail")
                } else {
                    Ok("ok")
                }
            }
        };

        let fut = tokio::spawn(async move { retry_with_backoff(attempts, base, factor, op).await });

        // advance time enough for two retries: 10ms + 20ms
        tokio::time::advance(Duration::from_millis(10)).await;
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_millis(20)).await;
        tokio::task::yield_now().await;

        let res = fut.await.unwrap();
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), "ok");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_retry_with_backoff_all_fail() {
        tokio::time::pause();

        let attempts = 3;
        let base = Duration::from_millis(5);
        let factor = 2.0;

        let counter = Arc::new(AtomicUsize::new(0));
        let c = counter.clone();

        let op = move || {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Err::<(), _>("still failing")
            }
        };

        let fut = tokio::spawn(async move { retry_with_backoff(attempts, base, factor, op).await });

        // advance time for all retries: 5 + 10
        tokio::time::advance(Duration::from_millis(5)).await;
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_millis(10)).await;
        tokio::task::yield_now().await;

        let res = fut.await.unwrap();
        assert!(res.is_err());
    }
}
