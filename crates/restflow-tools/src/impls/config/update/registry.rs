use serde_json::Value;

use restflow_traits::config_types::ConfigDocument;

use crate::Result;

use super::super::fields;
use super::super::parse::parse_u64;

pub(crate) fn apply(field: &str, value: &Value, config: &mut ConfigDocument) -> Result<()> {
    match field {
        "github_cache_ttl_secs" => {
            config.registry.github_cache_ttl_secs =
                parse_u64(value, "registry.github_cache_ttl_secs")?;
        }
        "marketplace_cache_ttl_secs" => {
            config.registry.marketplace_cache_ttl_secs =
                parse_u64(value, "registry.marketplace_cache_ttl_secs")?;
        }
        _ => {
            return Err(fields::unknown_domain_field(
                "registry",
                field,
                fields::VALID_REGISTRY_FIELDS,
            ));
        }
    }
    Ok(())
}
