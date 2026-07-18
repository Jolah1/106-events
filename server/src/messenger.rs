//! Outbound WhatsApp/SMS, behind a port.
//!
//! Only the edge of this module is provider-specific. Everything upstream — who
//! gets reminded, when, what it says, and the guarantee they're not texted
//! twice — is decided before we get here and is tested without credentials, the
//! same way the inbound webhook is.
//!
//! Adding a real provider means one more variant and one more `send` arm: build
//! the request, map the response onto `Sent`/`Failed`. Nothing above changes.

use std::sync::{Arc, Mutex};

use crate::config::Config;

/// How a message went out. WhatsApp is preferred when a guest has a number on
/// it; SMS is the fallback that always works on a Nigerian phone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Whatsapp,
    Sms,
}

impl Channel {
    pub fn as_str(self) -> &'static str {
        match self {
            Channel::Whatsapp => "whatsapp",
            Channel::Sms => "sms",
        }
    }
}

/// The result of one send attempt. A failure is a value, not an error: one
/// guest's bad number must not abort a batch of four hundred.
#[derive(Debug, Clone)]
pub enum Delivery {
    Sent,
    Failed(String),
}

/// One message as it left the system. Captured by [`Messenger::Capture`].
#[derive(Debug, Clone)]
pub struct Outbound {
    pub to: String,
    pub body: String,
    pub channel: Channel,
}

pub enum Messenger {
    /// No provider configured: log what would have gone out. The scheduler,
    /// targeting, quiet hours and idempotency all run for real, so the feature
    /// is demoable and testable end to end without an account.
    DevLog,
    /// Records every message instead of sending it, so tests can assert on what
    /// a guest would actually receive rather than only on counts.
    Capture(Arc<Mutex<Vec<Outbound>>>),
    /// Fails every send with a fixed reason, to exercise the failure path.
    Failing(String),
}

impl Messenger {
    pub fn from_config(_config: &Config) -> Self {
        tracing::warn!(
            "no messaging provider configured: reminders will be logged, not sent. \
             Wire an adapter in messenger.rs before going live."
        );
        Self::DevLog
    }

    pub fn is_dev(&self) -> bool {
        matches!(self, Self::DevLog)
    }

    /// A capturing messenger plus the handle to read what it captured.
    pub fn capturing() -> (Self, Arc<Mutex<Vec<Outbound>>>) {
        let sent = Arc::new(Mutex::new(Vec::new()));
        (Self::Capture(Arc::clone(&sent)), sent)
    }

    pub async fn send(&self, to: &str, body: &str, channel: Channel) -> Delivery {
        match self {
            Self::DevLog => {
                // Deliberately the module's own target, so the default
                // `server=debug` filter shows it. A custom target would need a
                // matching directive and would otherwise log into the void —
                // which is the one thing a dev logger must never do.
                tracing::info!("[{}] to {to}: {body}", channel.as_str());
                Delivery::Sent
            }
            Self::Capture(sent) => {
                sent.lock()
                    .expect("capture lock")
                    .push(Outbound { to: to.into(), body: body.into(), channel });
                Delivery::Sent
            }
            Self::Failing(reason) => Delivery::Failed(reason.clone()),
        }
    }
}
