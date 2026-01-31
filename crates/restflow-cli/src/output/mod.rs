pub mod json;
pub mod table;

use clap::ValueEnum;

#[derive(ValueEnum, Clone, Copy, Debug, Default)]
pub enum OutputFormat {
    #[default]
    Text,
    Json,
}

impl OutputFormat {
    #[allow(dead_code)]
    pub fn is_json(self) -> bool {
        matches!(self, OutputFormat::Json)
    }
}
