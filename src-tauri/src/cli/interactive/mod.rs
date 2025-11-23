use std::sync::RwLock;

use inquire::{Confirm, Select};

use crate::app_config::{AppType, MultiAppConfig};
use crate::cli::i18n::{texts, Language, current_language, set_language};
use crate::cli::ui::{create_table, error, highlight, info, success};
use crate::error::AppError;
use crate::services::{McpService, PromptService, ProviderService};
use crate::store::AppState;

pub fn run(app: Option<AppType>) -> Result<(), AppError> {
    let mut app_type = app.unwrap_or(AppType::Claude);

    // Show welcome
    print_welcome(&app_type);

    loop {
        match show_main_menu(&app_type)? {
            MainMenuChoice::ManageProviders => {
                if let Err(e) = manage_providers_menu(&app_type) {
                    println!("\n{}", error(&format!("{}: {}", texts::error_prefix(), e)));
                    pause();
                }
            }
            MainMenuChoice::ManageMCP => {
                if let Err(e) = manage_mcp_menu(&app_type) {
                    println!("\n{}", error(&format!("{}: {}", texts::error_prefix(), e)));
                    pause();
                }
            }
            MainMenuChoice::ManagePrompts => {
                if let Err(e) = manage_prompts_menu(&app_type) {
                    println!("\n{}", error(&format!("{}: {}", texts::error_prefix(), e)));
                    pause();
                }
            }
            MainMenuChoice::ViewCurrentConfig => {
                if let Err(e) = view_current_config(&app_type) {
                    println!("\n{}", error(&format!("{}: {}", texts::error_prefix(), e)));
                    pause();
                }
            }
            MainMenuChoice::SwitchApp => {
                if let Ok(new_app) = select_app() {
                    app_type = new_app;
                    print_welcome(&app_type);
                }
            }
            MainMenuChoice::Settings => {
                if let Err(e) = settings_menu() {
                    println!("\n{}", error(&format!("{}: {}", texts::error_prefix(), e)));
                    pause();
                }
            }
            MainMenuChoice::Exit => {
                println!("\n{}", success(texts::goodbye()));
                break;
            }
        }
    }

    Ok(())
}

#[derive(Debug, Clone)]
enum MainMenuChoice {
    ManageProviders,
    ManageMCP,
    ManagePrompts,
    ViewCurrentConfig,
    SwitchApp,
    Settings,
    Exit,
}

impl std::fmt::Display for MainMenuChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ManageProviders => write!(f, "{}", texts::menu_manage_providers()),
            Self::ManageMCP => write!(f, "{}", texts::menu_manage_mcp()),
            Self::ManagePrompts => write!(f, "{}", texts::menu_manage_prompts()),
            Self::ViewCurrentConfig => write!(f, "{}", texts::menu_view_config()),
            Self::SwitchApp => write!(f, "{}", texts::menu_switch_app()),
            Self::Settings => write!(f, "{}", texts::menu_settings()),
            Self::Exit => write!(f, "{}", texts::menu_exit()),
        }
    }
}

fn print_welcome(app_type: &AppType) {
    println!("\n{}", "═".repeat(60));
    println!("{}", highlight(texts::welcome_title()));
    println!("{}", "═".repeat(60));
    println!(
        "{} {}: {}",
        info("📱"),
        texts::application(),
        highlight(app_type.as_str())
    );
    println!("{}", "─".repeat(60));
    println!();
}

fn show_main_menu(app_type: &AppType) -> Result<MainMenuChoice, AppError> {
    let choices = vec![
        MainMenuChoice::ManageProviders,
        MainMenuChoice::ManageMCP,
        MainMenuChoice::ManagePrompts,
        MainMenuChoice::ViewCurrentConfig,
        MainMenuChoice::SwitchApp,
        MainMenuChoice::Settings,
        MainMenuChoice::Exit,
    ];

    let choice = Select::new(&texts::main_menu_prompt(app_type.as_str()), choices)
        .prompt()
        .map_err(|_| AppError::Message("Selection cancelled".to_string()))?;

    Ok(choice)
}

fn select_app() -> Result<AppType, AppError> {
    let apps = vec![AppType::Claude, AppType::Codex, AppType::Gemini];

    let app = Select::new(texts::select_application(), apps)
        .prompt()
        .map_err(|_| AppError::Message("Selection cancelled".to_string()))?;

    println!("\n{}", success(&texts::switched_to_app(app.as_str())));
    pause();

    Ok(app)
}

// ============================================================================
// Settings Menu
// ============================================================================

fn settings_menu() -> Result<(), AppError> {
    loop {
        println!("\n{}", highlight(texts::settings_title()));
        println!("{}", "─".repeat(60));

        // Show current language
        let lang = current_language();
        println!(
            "{}: {}",
            texts::current_language_label(),
            highlight(lang.display_name())
        );
        println!();

        let choices = vec![texts::change_language(), texts::back_to_main()];

        let choice = Select::new(texts::choose_action(), choices)
            .prompt()
            .map_err(|_| AppError::Message("Selection cancelled".to_string()))?;

        if choice == texts::change_language() {
            change_language_interactive()?;
        } else {
            break;
        }
    }

    Ok(())
}

fn change_language_interactive() -> Result<(), AppError> {
    let languages = vec![Language::English, Language::Chinese];

    let selected = Select::new(texts::select_language(), languages)
        .prompt()
        .map_err(|_| AppError::Message("Selection cancelled".to_string()))?;

    set_language(selected)?;

    println!("\n{}", success(texts::language_changed()));
    pause();

    Ok(())
}

// ============================================================================
// Provider Management
// ============================================================================

fn manage_providers_menu(app_type: &AppType) -> Result<(), AppError> {
    loop {
        println!("\n{}", highlight(texts::provider_management()));
        println!("{}", "─".repeat(60));

        let state = get_state()?;
        let providers = ProviderService::list(&state, app_type.clone())?;
        let current_id = ProviderService::current(&state, app_type.clone())?;

        if providers.is_empty() {
            println!("{}", info(texts::no_providers()));
        } else {
            let mut table = create_table();
            table.set_header(vec!["", texts::header_name(), texts::header_category()]);

            let mut provider_list: Vec<_> = providers.iter().collect();
            provider_list.sort_by(|(_, a), (_, b)| match (a.sort_index, b.sort_index) {
                (Some(idx_a), Some(idx_b)) => idx_a.cmp(&idx_b),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => a.created_at.cmp(&b.created_at),
            });

            for (id, provider) in &provider_list {
                let marker = if *id == &current_id { "✓" } else { " " };
                let name = if *id == &current_id {
                    format!("* {}", provider.name)
                } else {
                    format!("  {}", provider.name)
                };
                let category = provider.category.as_deref().unwrap_or("unknown");

                table.add_row(vec![marker.to_string(), name, category.to_string()]);
            }

            println!("{}", table);
        }

        println!();
        let choices = vec![
            texts::view_current_provider(),
            texts::switch_provider(),
            texts::delete_provider(),
            texts::back_to_main(),
        ];

        let choice = Select::new(texts::choose_action(), choices)
            .prompt()
            .map_err(|_| AppError::Message("Selection cancelled".to_string()))?;

        if choice == texts::view_current_provider() {
            view_current_provider(&state, app_type, &current_id)?;
            pause();
        } else if choice == texts::switch_provider() {
            switch_provider_interactive(&state, app_type, &providers, &current_id)?;
        } else if choice == texts::delete_provider() {
            delete_provider_interactive(&state, app_type, &providers, &current_id)?;
        } else {
            break;
        }
    }

    Ok(())
}

fn view_current_provider(
    state: &AppState,
    app_type: &AppType,
    current_id: &str,
) -> Result<(), AppError> {
    let providers = ProviderService::list(state, app_type.clone())?;
    if let Some(provider) = providers.get(current_id) {
        println!("\n{}", highlight(texts::current_provider_details()));
        println!("{}", "─".repeat(60));
        println!("ID:       {}", current_id);
        println!(
            "{}: {}",
            texts::header_name().trim_end_matches(':'),
            provider.name
        );
        println!(
            "{}: {}",
            texts::header_category().trim_end_matches(':'),
            provider.category.as_deref().unwrap_or("unknown")
        );
    }
    Ok(())
}

fn switch_provider_interactive(
    state: &AppState,
    app_type: &AppType,
    providers: &std::collections::HashMap<String, crate::provider::Provider>,
    current_id: &str,
) -> Result<(), AppError> {
    if providers.len() <= 1 {
        println!("\n{}", info(texts::only_one_provider()));
        pause();
        return Ok(());
    }

    let mut provider_choices: Vec<_> = providers
        .iter()
        .filter(|(id, _)| *id != current_id)
        .map(|(id, p)| format!("{} ({})", p.name, id))
        .collect();
    provider_choices.sort();

    if provider_choices.is_empty() {
        println!("\n{}", info(texts::no_other_providers()));
        pause();
        return Ok(());
    }

    let choice = Select::new(texts::select_provider_to_switch(), provider_choices)
        .prompt()
        .map_err(|_| AppError::Message("Selection cancelled".to_string()))?;

    let id = choice
        .split('(')
        .nth(1)
        .and_then(|s| s.strip_suffix(')'))
        .ok_or_else(|| AppError::Message("Invalid choice".to_string()))?;

    ProviderService::switch(state, app_type.clone(), id)?;

    println!("\n{}", success(&texts::switched_to_provider(id)));
    println!("{}", info(texts::restart_note()));
    pause();

    Ok(())
}

fn delete_provider_interactive(
    state: &AppState,
    app_type: &AppType,
    providers: &std::collections::HashMap<String, crate::provider::Provider>,
    current_id: &str,
) -> Result<(), AppError> {
    let deletable: Vec<_> = providers
        .iter()
        .filter(|(id, _)| *id != current_id)
        .map(|(id, p)| format!("{} ({})", p.name, id))
        .collect();

    if deletable.is_empty() {
        println!("\n{}", info(texts::no_deletable_providers()));
        pause();
        return Ok(());
    }

    let choice = Select::new(texts::select_provider_to_delete(), deletable)
        .prompt()
        .map_err(|_| AppError::Message("Selection cancelled".to_string()))?;

    let id = choice
        .split('(')
        .nth(1)
        .and_then(|s| s.strip_suffix(')'))
        .ok_or_else(|| AppError::Message("Invalid choice".to_string()))?;

    let confirm = Confirm::new(&texts::confirm_delete(id))
        .with_default(false)
        .prompt()
        .map_err(|_| AppError::Message("Confirmation failed".to_string()))?;

    if !confirm {
        println!("\n{}", info(texts::cancelled()));
        pause();
        return Ok(());
    }

    ProviderService::delete(state, app_type.clone(), id)?;
    println!("\n{}", success(&texts::deleted_provider(id)));
    pause();

    Ok(())
}

// ============================================================================
// MCP Management
// ============================================================================

fn manage_mcp_menu(_app_type: &AppType) -> Result<(), AppError> {
    loop {
        println!("\n{}", highlight(texts::mcp_management()));
        println!("{}", "─".repeat(60));

        let state = get_state()?;
        let servers = McpService::get_all_servers(&state)?;

        if servers.is_empty() {
            println!("{}", info(texts::no_mcp_servers()));
        } else {
            let mut table = create_table();
            table.set_header(vec![texts::header_name(), "Claude", "Codex", "Gemini"]);

            let mut server_list: Vec<_> = servers.iter().collect();
            server_list.sort_by_key(|(id, _)| *id);

            for (_, server) in &server_list {
                table.add_row(vec![
                    server.name.clone(),
                    if server.apps.claude { "✓" } else { "" }.to_string(),
                    if server.apps.codex { "✓" } else { "" }.to_string(),
                    if server.apps.gemini { "✓" } else { "" }.to_string(),
                ]);
            }

            println!("{}", table);
        }

        println!();
        let choices = vec![texts::sync_all_servers(), texts::back_to_main()];

        let choice = Select::new(texts::choose_action(), choices)
            .prompt()
            .map_err(|_| AppError::Message("Selection cancelled".to_string()))?;

        if choice == texts::sync_all_servers() {
            McpService::sync_all_enabled(&state)?;
            println!("\n{}", success(texts::synced_successfully()));
            pause();
        } else {
            break;
        }
    }

    Ok(())
}

// ============================================================================
// Prompts Management
// ============================================================================

fn manage_prompts_menu(app_type: &AppType) -> Result<(), AppError> {
    loop {
        println!("\n{}", highlight(texts::prompts_management()));
        println!("{}", "─".repeat(60));

        let state = get_state()?;
        let prompts = PromptService::get_prompts(&state, app_type.clone())?;

        if prompts.is_empty() {
            println!("{}", info(texts::no_prompts()));
        } else {
            let mut table = create_table();
            table.set_header(vec!["", texts::header_name(), texts::header_description()]);

            let mut prompt_list: Vec<_> = prompts.iter().collect();
            prompt_list.sort_by(|(_, a), (_, b)| {
                b.updated_at.unwrap_or(0).cmp(&a.updated_at.unwrap_or(0))
            });

            for (_, prompt) in &prompt_list {
                let marker = if prompt.enabled { "✓" } else { " " };
                let name = if prompt.enabled {
                    format!("* {}", prompt.name)
                } else {
                    format!("  {}", prompt.name)
                };
                let desc = prompt
                    .description
                    .as_deref()
                    .unwrap_or("")
                    .chars()
                    .take(40)
                    .collect::<String>();

                table.add_row(vec![marker.to_string(), name, desc]);
            }

            println!("{}", table);
        }

        println!();
        let choices = vec![texts::switch_active_prompt(), texts::back_to_main()];

        let choice = Select::new(texts::choose_action(), choices)
            .prompt()
            .map_err(|_| AppError::Message("Selection cancelled".to_string()))?;

        if choice == texts::switch_active_prompt() {
            switch_prompt_interactive(&state, app_type, &prompts)?;
        } else {
            break;
        }
    }

    Ok(())
}

fn switch_prompt_interactive(
    state: &AppState,
    app_type: &AppType,
    prompts: &std::collections::HashMap<String, crate::prompt::Prompt>,
) -> Result<(), AppError> {
    if prompts.is_empty() {
        println!("\n{}", info(texts::no_prompts_available()));
        pause();
        return Ok(());
    }

    let prompt_choices: Vec<_> = prompts
        .iter()
        .map(|(id, p)| format!("{} ({})", p.name, id))
        .collect();

    let choice = Select::new(texts::select_prompt_to_activate(), prompt_choices)
        .prompt()
        .map_err(|_| AppError::Message("Selection cancelled".to_string()))?;

    let id = choice
        .split('(')
        .nth(1)
        .and_then(|s| s.strip_suffix(')'))
        .ok_or_else(|| AppError::Message("Invalid choice".to_string()))?;

    PromptService::enable_prompt(state, app_type.clone(), id)?;

    println!("\n{}", success(&texts::activated_prompt(id)));
    println!("{}", info(texts::prompt_synced_note()));
    pause();

    Ok(())
}

// ============================================================================
// Configuration View
// ============================================================================

fn view_current_config(app_type: &AppType) -> Result<(), AppError> {
    println!("\n{}", highlight(texts::current_configuration()));
    println!("{}", "─".repeat(60));

    let state = get_state()?;

    // Provider info
    let current_provider = ProviderService::current(&state, app_type.clone())?;
    let providers = ProviderService::list(&state, app_type.clone())?;
    if let Some(provider) = providers.get(&current_provider) {
        println!("{}", highlight(texts::provider_label()));
        println!(
            "  {}: {}",
            texts::header_name().trim_end_matches(':'),
            provider.name
        );
        println!(
            "  {}: {}",
            texts::header_category().trim_end_matches(':'),
            provider.category.as_deref().unwrap_or("unknown")
        );
        println!();
    }

    // MCP servers count
    let mcp_servers = McpService::get_all_servers(&state)?;
    let enabled_count = mcp_servers
        .values()
        .filter(|s| s.apps.is_enabled_for(app_type))
        .count();
    println!("{}", highlight(texts::mcp_servers_label()));
    println!("  {}:   {}", texts::total(), mcp_servers.len());
    println!("  {}: {}", texts::enabled(), enabled_count);
    println!();

    // Prompts
    let prompts = PromptService::get_prompts(&state, app_type.clone())?;
    let active_prompt = prompts.iter().find(|(_, p)| p.enabled);
    println!("{}", highlight(texts::prompts_label()));
    println!("  {}:  {}", texts::total(), prompts.len());
    if let Some((_, p)) = active_prompt {
        println!("  {}: {}", texts::active(), p.name);
    } else {
        println!("  {}: {}", texts::active(), texts::none());
    }

    println!();
    pause();

    Ok(())
}

// ============================================================================
// Helper Functions
// ============================================================================

fn get_state() -> Result<AppState, AppError> {
    let config = MultiAppConfig::load()?;
    Ok(AppState {
        config: RwLock::new(config),
    })
}

fn pause() {
    let _ = Confirm::new(texts::press_enter())
        .with_default(true)
        .with_help_message("")
        .prompt();
}
