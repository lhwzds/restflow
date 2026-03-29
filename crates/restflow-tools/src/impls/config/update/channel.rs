use serde_json::Value;

use restflow_traits::config_types::ConfigDocument;

use crate::Result;

use super::super::fields;
use super::super::parse::{parse_u32, parse_u64};

pub(crate) fn apply(field: &str, value: &Value, config: &mut ConfigDocument) -> Result<()> {
    match field {
        "telegram_api_timeout_secs" => {
            config.channel.telegram_api_timeout_secs =
                parse_u64(value, "channel.telegram_api_timeout_secs")?;
        }
        "telegram_polling_timeout_secs" => {
            config.channel.telegram_polling_timeout_secs =
                parse_u32(value, "channel.telegram_polling_timeout_secs")?;
        }
        _ => {
            return Err(fields::unknown_domain_field(
                "channel",
                field,
                fields::VALID_CHANNEL_FIELDS,
            ));
        }
    }
    Ok(())
}
