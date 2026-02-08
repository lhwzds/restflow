use colored::Colorize;

pub fn handle_error(err: anyhow::Error) -> ! {
    eprintln!("{} {}", "Error:".red().bold(), err);

    let msg = err.to_string().to_lowercase();

    if msg.contains("api key not found") {
        eprintln!("\n{}", "Suggestion:".yellow().bold());
        eprintln!("  Set your API key with:");
        eprintln!(
            "  {} restflow secret set ANTHROPIC_API_KEY <value>",
            "$".dimmed()
        );
    }

    if msg.contains("agent not found") {
        eprintln!("\n{}", "Suggestion:".yellow().bold());
        eprintln!("  List available agents with:");
        eprintln!("  {} restflow agent list", "$".dimmed());
    }

    if msg.contains("connection refused") || msg.contains("network") {
        eprintln!("\n{}", "Suggestion:".yellow().bold());
        eprintln!("  Check your internet connection and try again.");
    }

    std::process::exit(1);
}
