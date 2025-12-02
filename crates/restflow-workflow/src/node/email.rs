use crate::engine::context::ExecutionContext;
use crate::models::{EmailOutput, NodeOutput, NodeType};
use crate::node::registry::NodeExecutor;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use lettre::message::{Mailbox, Message, SinglePart, header::ContentType};
use lettre::transport::smtp::{authentication::Credentials, response::Response};
use lettre::{AsyncSmtpTransport, AsyncTransport, Tokio1Executor};
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct EmailExecutor;

impl EmailExecutor {
    /// Extract and interpolate a required string field from config
    fn get_required_string_field(
        context: &ExecutionContext,
        config: &Value,
        field: &str,
    ) -> Result<String> {
        let value = context.interpolate_value(&config[field]);
        value
            .as_str()
            .map(String::from)
            .ok_or_else(|| anyhow!("{} field must be a string", field))
    }

    /// Parse comma-separated email addresses
    fn parse_recipients(addresses: &str) -> Result<Vec<Mailbox>> {
        addresses
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| {
                s.parse::<Mailbox>()
                    .map_err(|e| anyhow!("Invalid email address '{}': {}", s, e))
            })
            .collect()
    }

    /// Build email message
    fn build_message(
        from: &Mailbox,
        to_addresses: Vec<Mailbox>,
        cc_addresses: Vec<Mailbox>,
        bcc_addresses: Vec<Mailbox>,
        subject: &str,
        body: &str,
        is_html: bool,
    ) -> Result<Message> {
        let mut message_builder = Message::builder().from(from.clone()).subject(subject);

        // Add TO recipients
        for to in to_addresses {
            message_builder = message_builder.to(to);
        }

        // Add CC recipients
        for cc in cc_addresses {
            message_builder = message_builder.cc(cc);
        }

        // Add BCC recipients
        for bcc in bcc_addresses {
            message_builder = message_builder.bcc(bcc);
        }

        // Build message body based on content type
        let message = if is_html {
            message_builder.singlepart(
                SinglePart::builder()
                    .header(ContentType::TEXT_HTML)
                    .body(body.to_string()),
            )?
        } else {
            message_builder.body(body.to_string())?
        };

        Ok(message)
    }

    /// Extract the provider-supplied identifier from an SMTP response.
    /// Falls back to returning the entire textual response if no ID pattern is detected.
    fn extract_message_identifier(response: &Response) -> Option<String> {
        let message = response
            .message()
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string();

        if message.is_empty() {
            return None;
        }

        let message_lower = message.to_lowercase();
        const QUEUED_AS: &str = "queued as";
        if let Some(idx) = message_lower.find(QUEUED_AS) {
            let remainder = message[idx + QUEUED_AS.len()..].trim();
            if let Some(raw_id) = remainder.split_whitespace().next() {
                let cleaned =
                    raw_id.trim_matches(|c: char| matches!(c, '<' | '>' | '"' | '\'' | ';' | '.'));
                if !cleaned.is_empty() {
                    return Some(cleaned.to_string());
                }
            }
        }

        if let Some(start) = message.find('<')
            && let Some(end_rel) = message[start + 1..].find('>')
        {
            let candidate = &message[start + 1..start + 1 + end_rel];
            let cleaned = candidate.trim();
            if !cleaned.is_empty() {
                return Some(cleaned.to_string());
            }
        }

        Some(message)
    }
}

#[async_trait]
impl NodeExecutor for EmailExecutor {
    async fn execute(
        &self,
        _node_type: &NodeType,
        config: &Value,
        context: &mut ExecutionContext,
    ) -> Result<NodeOutput> {
        // Get SMTP configuration from individual fields
        let smtp_server = config["smtp_server"]
            .as_str()
            .ok_or_else(|| anyhow!("smtp_server is required"))?
            .to_string();

        let smtp_port = config["smtp_port"]
            .as_u64()
            .ok_or_else(|| anyhow!("smtp_port is required"))? as u16;

        let smtp_username = config["smtp_username"]
            .as_str()
            .ok_or_else(|| anyhow!("smtp_username is required"))?
            .to_string();

        let smtp_use_tls = config
            .get("smtp_use_tls")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        // Get password from config (Direct or Secret)
        let password_config = &config["smtp_password_config"];
        let smtp_password =
            if let Some(config_type) = password_config.get("type").and_then(|v| v.as_str()) {
                match config_type {
                    "direct" => {
                        // Direct password mode
                        password_config["value"]
                            .as_str()
                            .ok_or_else(|| anyhow!("Direct password value is required"))?
                            .to_string()
                    }
                    "secret" => {
                        // Secret reference mode
                        let secret_name = password_config["value"]
                            .as_str()
                            .ok_or_else(|| anyhow!("Secret name is required"))?;

                        let secret_storage = context
                            .secret_storage
                            .as_ref()
                            .ok_or_else(|| anyhow!("Secret storage not available"))?;

                        secret_storage.get_secret(secret_name)?.ok_or_else(|| {
                            anyhow!("SMTP password secret '{}' not found", secret_name)
                        })?
                    }
                    _ => {
                        return Err(anyhow!(
                            "Invalid smtp_password_config type: {}",
                            config_type
                        ));
                    }
                }
            } else {
                return Err(anyhow!("smtp_password_config is required"));
            };

        // Resolve templated fields using context
        let to_str = Self::get_required_string_field(context, config, "to")?;

        let cc_str = config
            .get("cc")
            .and_then(|v| context.interpolate_value(v).as_str().map(String::from));

        let bcc_str = config
            .get("bcc")
            .and_then(|v| context.interpolate_value(v).as_str().map(String::from));

        let subject_str = Self::get_required_string_field(context, config, "subject")?;
        let body_str = Self::get_required_string_field(context, config, "body")?;

        let is_html = config
            .get("html")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Parse recipients
        let to_addresses = Self::parse_recipients(&to_str)?;
        if to_addresses.is_empty() {
            return Err(anyhow!("At least one recipient is required"));
        }

        let cc_addresses = cc_str
            .as_deref()
            .map(Self::parse_recipients)
            .transpose()?
            .unwrap_or_default();

        let bcc_addresses = bcc_str
            .as_deref()
            .map(Self::parse_recipients)
            .transpose()?
            .unwrap_or_default();

        // Collect all recipients for output
        let all_recipients: Vec<String> = to_addresses
            .iter()
            .chain(cc_addresses.iter())
            .chain(bcc_addresses.iter())
            .map(|m| m.to_string())
            .collect();

        // Build sender mailbox
        let from: Mailbox = smtp_username
            .parse()
            .map_err(|e| anyhow!("Invalid sender email '{}': {}", smtp_username, e))?;

        // Build email message
        let message = Self::build_message(
            &from,
            to_addresses,
            cc_addresses,
            bcc_addresses,
            &subject_str,
            &body_str,
            is_html,
        )?;

        // Build SMTP transport
        let creds = Credentials::new(smtp_username.clone(), smtp_password.clone());

        let mailer = if smtp_use_tls {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&smtp_server)?
                .port(smtp_port)
                .credentials(creds)
                .build()
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&smtp_server)
                .port(smtp_port)
                .credentials(creds)
                .build()
        };

        // Send email
        let response = mailer
            .send(message)
            .await
            .map_err(|e| anyhow!("Failed to send email: {}", e))?;

        // Get current timestamp
        let sent_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        // Extract message ID (provider response text) when available
        let message_id = Self::extract_message_identifier(&response);

        Ok(NodeOutput::Email(EmailOutput {
            sent_at,
            message_id,
            recipients: all_recipients,
            subject: subject_str.to_string(),
            is_html,
        }))
    }
}
