use std::process::Output;
use std::time::Duration;

use tokio::process::Command;

/// Executes `cmd.output()` with `kill_on_drop(true)` and a hard timeout.
pub async fn run_output_with_timeout(
    cmd: &mut Command,
    timeout: Duration,
    label: &str,
) -> std::io::Result<Output> {
    cmd.kill_on_drop(true);
    match tokio::time::timeout(timeout, cmd.output()).await {
        Ok(res) => res,
        Err(_) => Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            format!("{label} timed out after {}s", timeout.as_secs()),
        )),
    }
}
