//! メール（`channels = ["email"]`）。外部の SMTP サーバーへ送る。HTML の本文があれば
//! プレーンテキストと HTML の両方（`multipart/alternative`）を送る

use std::time::Duration;

use aegis_core::{AegisError, Recipient};
use async_trait::async_trait;
use lettre::message::header::ContentType;
use lettre::message::{Mailbox, MultiPart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

use crate::channel::{Delivered, NotificationChannel, NotificationMessage, SendError};
use crate::config::{EmailConfig, SmtpTls};

pub struct EmailChannel {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
    reply_to: Option<Mailbox>,
    staff: Vec<Mailbox>,
}

fn mailbox(value: &str) -> aegis_core::Result<Mailbox> {
    value
        .parse()
        .map_err(|e| AegisError::Config(format!("Invalid email address {:?}: {}", value, e)))
}

impl EmailChannel {
    pub fn new(config: &EmailConfig) -> aegis_core::Result<Self> {
        let host = config.smtp_host.trim();
        let smtp_error =
            |e: lettre::transport::smtp::Error| AegisError::Config(format!("SMTP: {}", e));
        let mut builder = match config.tls {
            SmtpTls::Starttls => {
                AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host).map_err(smtp_error)?
            }
            SmtpTls::Tls => {
                AsyncSmtpTransport::<Tokio1Executor>::relay(host).map_err(smtp_error)?
            }
            SmtpTls::None => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host),
        }
        .port(config.port())
        .timeout(Some(Duration::from_secs(config.timeout_secs.max(1))));
        if !config.username.is_empty() {
            builder =
                builder.credentials(Credentials::new(config.username.clone(), config.password()));
        }
        Ok(Self {
            transport: builder.build(),
            from: mailbox(&config.from)?,
            reply_to: (!config.reply_to.is_empty())
                .then(|| mailbox(&config.reply_to))
                .transpose()?,
            staff: config
                .staff_recipients
                .iter()
                .map(|a| mailbox(a))
                .collect::<aegis_core::Result<_>>()?,
        })
    }

    fn recipients(&self, recipient: &Recipient) -> Result<Vec<Mailbox>, SendError> {
        match recipient {
            Recipient::Person { email, name } => {
                let address = email.trim().parse().map_err(|e| {
                    SendError::permanent(format!("Invalid email address {:?}: {}", email, e))
                })?;
                Ok(vec![Mailbox::new(name.clone(), address)])
            }
            Recipient::Staff => Ok(self.staff.clone()),
        }
    }
}

#[async_trait]
impl NotificationChannel for EmailChannel {
    fn name(&self) -> &str {
        "email"
    }

    fn accepts(&self, recipient: &Recipient) -> bool {
        match recipient {
            Recipient::Person { email, .. } => !email.trim().is_empty(),
            Recipient::Staff => !self.staff.is_empty(),
        }
    }

    async fn send(&self, message: &NotificationMessage) -> Result<Delivered, SendError> {
        let mut builder = Message::builder()
            .from(self.from.clone())
            .subject(message.subject.clone());
        if let Some(reply_to) = &self.reply_to {
            builder = builder.reply_to(reply_to.clone());
        }
        for to in self.recipients(&message.recipient)? {
            builder = builder.to(to);
        }
        let email = match &message.html {
            Some(html) => builder.multipart(MultiPart::alternative_plain_html(
                message.body.clone(),
                html.clone(),
            )),
            None => builder
                .header(ContentType::TEXT_PLAIN)
                .body(message.body.clone()),
        }
        .map_err(|e| SendError::permanent(format!("Could not build the email: {}", e)))?;
        self.transport.send(email).await.map_err(|e| {
            let text = format!("SMTP: {}", e);
            if e.is_permanent() {
                SendError::permanent(text)
            } else {
                SendError::temporary(text)
            }
        })?;
        Ok(Delivered::Sent)
    }
}
