use anyhow::anyhow;
use gtk::gio;

/// Runs blocking work on a thread of the GIO pool; a panic in it is reported as an error, like any other failure.
pub async fn run<T, E>(work: impl FnOnce() -> Result<T, E> + Send + 'static) -> Result<T, E>
where
    T: Send + 'static,
    E: From<anyhow::Error> + Send + 'static,
{
    gio::spawn_blocking(work).await.unwrap_or_else(|_| Err(anyhow!("background task panicked").into()))
}
