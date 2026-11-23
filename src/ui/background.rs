use anyhow::{Result, anyhow};
use gtk::gio;

/// Runs blocking work on a thread of the GIO pool; a panic in it is reported as an error, like any other failure.
pub async fn run<T: Send + 'static>(work: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    gio::spawn_blocking(work).await.unwrap_or_else(|_| Err(anyhow!("background task panicked")))
}
