use std::io::{self, IsTerminal, Write};
use std::sync::RwLock;

use crate::app_config::MultiAppConfig;
use crate::cli::i18n::texts;
use crate::error::AppError;
use crate::store::AppState;

pub fn get_state() -> Result<AppState, AppError> {
    let config = MultiAppConfig::load()?;
    Ok(AppState {
        config: RwLock::new(config),
    })
}

pub fn clear_screen() {
    if !io::stdout().is_terminal() {
        return;
    }

    let term = console::Term::stdout();
    let _ = term.clear_screen();
    let _ = io::stdout().flush();
}

pub fn pause() {
    print!("{} ", texts::press_enter());
    let _ = io::stdout().flush();
    let mut input = String::new();
    let _ = io::stdin().read_line(&mut input);
}
