use bevy::log::trace;
use bevy::prelude::*;
use chrono::Utc;
use crossbeam::channel::{unbounded, Sender};
use shared::logging::logging::{LogData, LogEvent};

/// Channel sender for log events - can be cloned and used from any thread
#[derive(Resource, Clone)]
pub struct LogEventSender(pub Sender<LogEvent>);

/// Extension trait to easily send log events from anywhere
pub trait LogEventSenderExt {
    fn log(&self, data: LogData);
}

impl LogEventSenderExt for LogEventSender {
    fn log(&self, data: LogData) {
        // Also write to tracing (which goes to file when logging feature is enabled)
        trace!("{}", data);

        let event = LogEvent {
            timestamp: Utc::now(),
            data,
        };
        // Ignore send errors - they happen during shutdown or when no receiver is registered.
        let _ = self.0.send(event);
    }
}

/// Utility to create a paired sender/receiver.
/// Callers may ignore the receiver if broadcasting is not needed.
pub fn make_log_channel() -> (LogEventSender, crossbeam::channel::Receiver<LogEvent>) {
    let (sender, receiver) = unbounded::<LogEvent>();
    (LogEventSender(sender), receiver)
}
