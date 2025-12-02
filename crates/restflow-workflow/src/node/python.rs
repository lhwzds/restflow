use anyhow::Result;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PythonNode {
    pub code: String,
    #[serde(default)]
    pub dependencies: Vec<String>,
}

impl PythonNode {
    pub fn from_config(config: &serde_json::Value) -> Result<Self> {
        let code = config["code"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Code missing in config"))?
            .to_string();

        let dependencies = config["dependencies"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        Ok(Self { code, dependencies })
    }

    /// Build complete Python script with PEP 723 dependency header
    pub fn build_script(&self) -> String {
        if self.dependencies.is_empty() {
            return self.code.clone();
        }

        let deps = self
            .dependencies
            .iter()
            .map(|d| format!("#   \"{}\",", d))
            .collect::<Vec<_>>()
            .join("\n");

        format!(
            "# /// script\n# dependencies = [\n{}\n# ]\n# ///\n\n{}",
            deps, self.code
        )
    }
}
