#[derive(Debug, Clone)]
pub enum SlashCommand {
    Help,
    Clear,
    Exit,
    Agent(String),
    Model(String),
    Session(String),
    New,
    History,
    Memory(String),
    Export,
    Think(bool),
    Verbose(bool),
}

impl SlashCommand {
    pub fn parse(input: &str) -> Option<Self> {
        let input = input.trim();
        if !input.starts_with('/') {
            return None;
        }

        let mut parts = input[1..].splitn(2, ' ');
        let cmd = parts.next().unwrap_or("").to_lowercase();
        let arg = parts.next().map(|s| s.trim().to_string());

        match cmd.as_str() {
            "help" | "h" | "?" => Some(Self::Help),
            "clear" | "cls" => Some(Self::Clear),
            "exit" | "quit" | "q" => Some(Self::Exit),
            "agent" | "a" => arg.map(Self::Agent),
            "model" | "m" => arg.map(Self::Model),
            "session" | "s" => arg.map(Self::Session),
            "new" | "n" => Some(Self::New),
            "history" => Some(Self::History),
            "memory" => arg.map(Self::Memory),
            "export" => Some(Self::Export),
            "think" => Some(Self::Think(arg.map(|a| a == "on").unwrap_or(true))),
            "verbose" => Some(Self::Verbose(arg.map(|a| a == "on").unwrap_or(true))),
            _ => None,
        }
    }
}
