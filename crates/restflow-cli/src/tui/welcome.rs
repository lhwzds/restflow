use crossterm::{
    execute,
    terminal::{Clear, ClearType},
};
use std::io::{self, Write};

const CONTENT_WIDTH: usize = 49;

pub fn show_welcome(clear_screen: bool) -> io::Result<()> {
    if clear_screen {
        execute!(io::stdout(), Clear(ClearType::All))?;
    }

    let version = env!("CARGO_PKG_VERSION");
    let border = format!("╭{}╮", "─".repeat(CONTENT_WIDTH + 2));
    let footer = format!("╰{}╯", "─".repeat(CONTENT_WIDTH + 2));
    let print_row = |text: &str| {
        println!(
            "│ {:<CONTENT_WIDTH$} │",
            text,
            CONTENT_WIDTH = CONTENT_WIDTH
        );
    };

    println!();
    println!("{}", border);
    print_row(&format!("[◉─◉] RestFlow (v{})", version));
    print_row("");
    print_row("💡 Quick start:");
    print_row("   Type text  -> Chat with AI");
    print_row("   /list      -> List workflows");
    print_row("   /          -> Show commands");
    println!("{}\n", footer);

    io::stdout().flush()
}
