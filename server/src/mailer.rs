use anyhow::{Context, Result};
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::{Mailbox, header::ContentType},
};

use crate::config::Config;

pub enum Mailer {
    /// No SMTP configured: log the link. The login endpoint also returns it
    /// so the flow stays fully usable in development.
    DevLog,
    Smtp {
        transport: AsyncSmtpTransport<Tokio1Executor>,
        from: Mailbox,
    },
}

impl Mailer {
    pub fn from_config(config: &Config) -> Result<Self> {
        match &config.smtp_url {
            None => {
                tracing::warn!("SMTP_URL not set: magic links will be logged, not emailed");
                Ok(Self::DevLog)
            }
            Some(url) => Ok(Self::Smtp {
                transport: AsyncSmtpTransport::<Tokio1Executor>::from_url(url)
                    .context("invalid SMTP_URL")?
                    .build(),
                from: config
                    .email_from
                    .parse()
                    .context("invalid EMAIL_FROM address")?,
            }),
        }
    }

    pub fn is_dev(&self) -> bool {
        matches!(self, Self::DevLog)
    }

    pub async fn send_magic_link(&self, to: &str, link: &str) -> Result<()> {
        match self {
            Self::DevLog => {
                tracing::info!("magic link for {to}: {link}");
                Ok(())
            }
            Self::Smtp { transport, from } => {
                let email = Message::builder()
                    .from(from.clone())
                    .to(to.parse().context("invalid recipient address")?)
                    .subject("Sign in to 106 Events")
                    .header(ContentType::TEXT_PLAIN)
                    .body(format!(
                        "Tap the link below to sign in to 106 Events.\n\n{link}\n\n\
                         This link expires in 15 minutes. If you didn't request it, ignore this email."
                    ))
                    .context("building email")?;
                transport.send(email).await.context("sending email")?;
                Ok(())
            }
        }
    }
}
