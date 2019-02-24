//! Logging utilities

use std::collections::VecDeque;
use std::sync::Mutex;

/// Saves all log records in a global deque.
///
/// Uses a `DebugView` to access it.
struct CursiveLogger;

static LOGGER: CursiveLogger = CursiveLogger;

/// A log record.
pub struct Record {
    /// Log level used for this record
    pub level: log::Level,
    /// Time this message was logged
    pub time: chrono::DateTime<chrono::Utc>,
    /// Message content
    pub message: String,
}

lazy_static! {
    /// Circular buffer for logs. Use it to implement `DebugView`.
    pub static ref LOGS: Mutex<VecDeque<Record>> =
        Mutex::new(VecDeque::new());
}

impl log::Log for CursiveLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        let mut logs = LOGS.lock().unwrap();
        // TODO: customize the format? Use colors? Save more info?
        if logs.len() == logs.capacity() {
            logs.pop_front();
        }
        logs.push_back(Record {
            level: record.level(),
            message: format!("{}", record.args()),
            time: chrono::Utc::now(),
        });
    }

    fn flush(&self) {}
}

/// Initialize the Cursive logger.
///
/// Make sure this is the only logger your are using.
///
/// Use a [`::views::DebugView`] to see the logs, or use [`::Cursive::toggle_debug_console()`].
pub fn init() {
    // TODO: Configure the deque size?
    LOGS.lock().unwrap().reserve(1_000);

    // This will panic if `set_logger` was already called.
    log::set_logger(&LOGGER).unwrap();

    // TODO: read the level from env variable? From argument?
    log::set_max_level(log::LevelFilter::Trace);
}
