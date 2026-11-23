use std::future::Future;
use std::time::Duration;

/// `ETXTBSY` on Linux and macOS.
const TEXT_FILE_BUSY: i32 = 26;
const ATTEMPTS: u32 = 20;
const PAUSE: Duration = Duration::from_millis(25);

/// Runs `op`, trying again while exec answers "Text file busy": a binary just
/// written can still be open for writing in a thread that forked at the wrong
/// moment, and the window closes within milliseconds. Any other error, and the
/// last busy one after about half a second, is returned as it is.
pub(super) async fn retry_busy<T, F, Fut>(mut op: F) -> std::io::Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = std::io::Result<T>>,
{
    let mut attempt = 1;
    loop {
        match op().await {
            Err(e) if e.raw_os_error() == Some(TEXT_FILE_BUSY) && attempt < ATTEMPTS => {
                attempt += 1;
                tokio::time::sleep(PAUSE).await;
            }
            other => return other,
        }
    }
}
