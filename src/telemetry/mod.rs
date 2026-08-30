//! Logging and light telemetry setup.

use tracing_subscriber::{fmt, EnvFilter};

pub fn init(level: &str, quiet: bool, verbose: u8) {
    let filter = if quiet {
        EnvFilter::new("error")
    } else {
        match verbose {
            0 => EnvFilter::new(level),
            1 => EnvFilter::new("info"),
            2 => EnvFilter::new("debug"),
            _ => EnvFilter::new("trace"),
        }
    };

    let _ = fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(false)
        .try_init();
}
