use anyhow::Result;

use crate::cli::ChatArgs;
use crate::tui::{ChatLaunchOptions, run_chat_tui};

pub async fn run(args: ChatArgs) -> Result<()> {
    run_chat_tui(ChatLaunchOptions {
        agent: args.agent,
        session: args.session,
        message: args.message,
    })
    .await
}
