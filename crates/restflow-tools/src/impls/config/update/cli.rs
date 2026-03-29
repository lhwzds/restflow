use serde_json::Value;

use restflow_traits::config_types::ConfigDocument;

use crate::Result;

use super::super::fields;
use super::super::parse::{
    parse_bool, parse_optional_string, parse_string_list, parse_u32, parse_u64, parse_usize,
};

pub(crate) fn apply(field: &str, value: &Value, config: &mut ConfigDocument) -> Result<()> {
    match field {
        "version" => {
            config.cli.version = parse_u32(value, "cli.version")?;
        }
        "agent" => {
            config.cli.agent = parse_optional_string(value, "cli.agent")?;
        }
        "model" => {
            config.cli.model = parse_optional_string(value, "cli.model")?;
        }
        "sandbox.enabled" => {
            config.cli.sandbox.enabled = parse_bool(value, "cli.sandbox.enabled")?;
        }
        "sandbox.env.isolate" => {
            config.cli.sandbox.env.isolate = parse_bool(value, "cli.sandbox.env.isolate")?;
        }
        "sandbox.env.allow" => {
            config.cli.sandbox.env.allow = parse_string_list(value, "cli.sandbox.env.allow")?;
        }
        "sandbox.env.block" => {
            config.cli.sandbox.env.block = parse_string_list(value, "cli.sandbox.env.block")?;
        }
        "sandbox.limits.timeout_secs" => {
            config.cli.sandbox.limits.timeout_secs =
                parse_u64(value, "cli.sandbox.limits.timeout_secs")?;
        }
        "sandbox.limits.max_output_bytes" => {
            config.cli.sandbox.limits.max_output_bytes =
                parse_usize(value, "cli.sandbox.limits.max_output_bytes")?;
        }
        _ => {
            return Err(fields::unknown_domain_field(
                "cli",
                field,
                fields::VALID_CLI_FIELDS,
            ));
        }
    }
    Ok(())
}
