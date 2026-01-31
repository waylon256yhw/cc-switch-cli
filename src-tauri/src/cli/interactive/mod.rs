mod config;
mod mcp;
mod prompts;
mod provider;
mod settings;
mod skills;
mod utils;

pub mod legacy;

use std::io::IsTerminal;

use crate::app_config::AppType;
use crate::error::AppError;

pub fn run(app: Option<AppType>) -> Result<(), AppError> {
    if std::env::var("CC_SWITCH_LEGACY_TUI").ok().as_deref() == Some("1") {
        return legacy::run(app);
    }

    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return legacy::run(app);
    }

    crate::cli::tui::run(app)
}
