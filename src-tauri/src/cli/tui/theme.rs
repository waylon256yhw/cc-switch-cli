use ratatui::style::Color;

use crate::app_config::AppType;

#[derive(Debug, Clone)]
pub struct Theme {
    pub accent: Color,
    pub ok: Color,
    pub warn: Color,
    pub err: Color,
    pub dim: Color,
    pub no_color: bool,
}

pub fn no_color() -> bool {
    std::env::var("NO_COLOR").is_ok()
}

pub fn theme_for(app: &AppType) -> Theme {
    let no_color = no_color();
    let accent = if no_color {
        Color::Reset
    } else {
        match app {
            AppType::Codex => Color::LightGreen,
            AppType::Claude => Color::LightCyan,
            AppType::Gemini => Color::LightMagenta,
        }
    };

    Theme {
        accent,
        ok: if no_color { Color::Reset } else { Color::Green },
        warn: if no_color {
            Color::Reset
        } else {
            Color::Yellow
        },
        err: if no_color { Color::Reset } else { Color::Red },
        dim: if no_color {
            Color::Reset
        } else {
            Color::DarkGray
        },
        no_color,
    }
}
