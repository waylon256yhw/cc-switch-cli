use inquire::{Confirm, Select, Text};
use std::path::Path;

use crate::app_config::{AppType, MultiAppConfig};
use crate::cli::i18n::texts;
use crate::cli::ui::{error, highlight, info, success};
use crate::config::get_app_config_path;
use crate::error::AppError;
use crate::services::ConfigService;
use crate::services::ProviderService;

use super::utils::{clear_screen, get_state, pause};

pub fn manage_config_menu(app_type: &AppType) -> Result<(), AppError> {
    loop {
        clear_screen();
        println!("\n{}", highlight(texts::config_management()));
        println!("{}", "─".repeat(60));

        let choices = vec![
            texts::config_show_path(),
            texts::config_show_full(),
            texts::config_export(),
            texts::config_import(),
            texts::config_backup(),
            texts::config_restore(),
            texts::config_validate(),
            texts::config_common_snippet(),
            texts::config_reset(),
            texts::back_to_main(),
        ];

        let choice = Select::new(texts::choose_action(), choices)
            .prompt()
            .map_err(|_| AppError::Message("Selection cancelled".to_string()))?;

        if choice == texts::config_show_path() {
            show_config_path_interactive()?;
        } else if choice == texts::config_show_full() {
            show_full_config_interactive()?;
        } else if choice == texts::config_export() {
            let path = Text::new(texts::enter_export_path())
                .with_default("./config-export.json")
                .prompt()
                .map_err(|e| AppError::Message(format!("Input failed: {}", e)))?;
            export_config_interactive(&path)?;
        } else if choice == texts::config_import() {
            let path = Text::new(texts::enter_import_path())
                .prompt()
                .map_err(|e| AppError::Message(format!("Input failed: {}", e)))?;
            import_config_interactive(&path)?;
        } else if choice == texts::config_backup() {
            backup_config_interactive()?;
        } else if choice == texts::config_restore() {
            restore_config_interactive()?;
        } else if choice == texts::config_validate() {
            validate_config_interactive()?;
        } else if choice == texts::config_common_snippet() {
            edit_common_config_snippet_interactive(app_type)?;
        } else if choice == texts::config_reset() {
            reset_config_interactive()?;
        } else {
            break;
        }
    }

    Ok(())
}

fn edit_common_config_snippet_interactive(app_type: &AppType) -> Result<(), AppError> {
    clear_screen();
    println!(
        "\n{}",
        highlight(
            texts::config_common_snippet()
                .trim_start_matches("🧩 ")
                .trim()
        )
    );
    println!("{}", "─".repeat(60));

    let state = get_state()?;
    let current = {
        let cfg = state.config.read()?;
        cfg.common_config_snippets.get(app_type).cloned()
    }
    .unwrap_or_default();

    let initial = if current.trim().is_empty() {
        "{}\n".to_string()
    } else {
        current
    };

    let field_name = format!("common_config_snippet.{}", app_type.as_str());

    loop {
        println!(
            "\n{}",
            info(&format!(
                "{} ({})",
                texts::opening_external_editor(),
                field_name
            ))
        );

        let edited = match open_external_editor(&initial) {
            Ok(content) => content,
            Err(e) => {
                println!("\n{}", error(&format!("{}", e)));
                return Ok(());
            }
        };

        // Check if content was changed
        if edited.trim() == initial.trim() {
            println!("\n{}", info(texts::no_changes_detected()));
            return Ok(());
        }

        let edited = edited.trim().to_string();
        let (next_snippet, action_label) = if edited.is_empty() {
            (None, texts::common_config_snippet_cleared())
        } else {
            let value: serde_json::Value = match serde_json::from_str(&edited) {
                Ok(v) => v,
                Err(e) => {
                    println!(
                        "\n{}",
                        error(&format!("{}: {}", texts::invalid_json_syntax(), e))
                    );
                    if !retry_prompt()? {
                        return Ok(());
                    }
                    continue;
                }
            };

            if !value.is_object() {
                println!(
                    "\n{}",
                    error(&texts::common_config_snippet_not_object().to_string())
                );
                if !retry_prompt()? {
                    return Ok(());
                }
                continue;
            }

            let pretty = serde_json::to_string_pretty(&value)
                .map_err(|e| AppError::Message(format!("Failed to serialize JSON: {}", e)))?;

            println!("\n{}", highlight(texts::config_common_snippet()));
            println!("{}", "─".repeat(60));
            println!("{}", pretty);

            let confirm = Confirm::new(texts::confirm_save_changes())
                .with_default(false)
                .prompt()
                .map_err(|e| AppError::Message(format!("Confirmation failed: {}", e)))?;

            if !confirm {
                println!("\n{}", info(texts::cancelled()));
                return Ok(());
            }

            (Some(pretty), texts::common_config_snippet_saved())
        };

        {
            let mut cfg = state.config.write()?;
            cfg.common_config_snippets.set(app_type, next_snippet);
        }
        state.save()?;

        println!("\n{}", success(action_label));

        break;
    }

    let apply = Confirm::new(texts::common_config_snippet_apply_now())
        .with_default(true)
        .prompt()
        .map_err(|_| AppError::Message("Confirmation failed".to_string()))?;

    if apply {
        let current_id = ProviderService::current(&state, app_type.clone())?;
        if current_id.trim().is_empty() {
            println!(
                "{}",
                info(texts::common_config_snippet_no_current_provider())
            );
        } else {
            ProviderService::switch(&state, app_type.clone(), &current_id)?;
            println!("{}", success(texts::common_config_snippet_applied()));
        }
    } else {
        println!("{}", info(texts::common_config_snippet_apply_hint()));
    }

    pause();
    Ok(())
}

fn retry_prompt() -> Result<bool, AppError> {
    Confirm::new(texts::retry_editing())
        .with_default(true)
        .prompt()
        .map_err(|e| AppError::Message(format!("Confirmation failed: {}", e)))
}

fn open_external_editor(initial_content: &str) -> Result<String, AppError> {
    edit::edit(initial_content)
        .map_err(|e| AppError::Message(format!("{}: {}", texts::editor_failed(), e)))
}

fn show_config_path_interactive() -> Result<(), AppError> {
    clear_screen();
    let config_path = get_app_config_path();
    let config_dir = config_path.parent().unwrap_or(&config_path);

    println!(
        "\n{}",
        highlight(texts::config_show_path().trim_start_matches("📍 "))
    );
    println!("{}", "─".repeat(60));
    println!("Config file: {}", config_path.display());
    println!("Config dir:  {}", config_dir.display());

    if config_path.exists() {
        if let Ok(metadata) = std::fs::metadata(&config_path) {
            println!("File size:   {} bytes", metadata.len());
        }
    } else {
        println!("Status:      File does not exist");
    }

    let backup_dir = config_dir.join("backups");
    if backup_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&backup_dir) {
            let count = entries.filter(|e| e.is_ok()).count();
            println!("Backups:     {} files in {}", count, backup_dir.display());
        }
    }

    pause();
    Ok(())
}

fn show_full_config_interactive() -> Result<(), AppError> {
    clear_screen();
    let config = MultiAppConfig::load()?;
    let json = serde_json::to_string_pretty(&config)
        .map_err(|e| AppError::Message(format!("Failed to serialize config: {}", e)))?;

    println!(
        "\n{}",
        highlight(texts::config_show_full().trim_start_matches("👁️  "))
    );
    println!("{}", "─".repeat(60));
    println!("{}", json);

    pause();
    Ok(())
}

fn export_config_interactive(path: &str) -> Result<(), AppError> {
    clear_screen();
    let target_path = Path::new(path);

    if target_path.exists() {
        let confirm = Confirm::new(&texts::file_overwrite_confirm(path))
            .with_default(false)
            .prompt()
            .map_err(|_| AppError::Message("Confirmation failed".to_string()))?;

        if !confirm {
            println!("\n{}", info(texts::cancelled()));
            pause();
            return Ok(());
        }
    }

    ConfigService::export_config_to_path(target_path)?;

    println!("\n{}", success(&texts::exported_to(path)));
    pause();
    Ok(())
}

fn import_config_interactive(path: &str) -> Result<(), AppError> {
    clear_screen();
    let file_path = Path::new(path);

    if !file_path.exists() {
        return Err(AppError::Message(format!("File not found: {}", path)));
    }

    let confirm = Confirm::new(texts::confirm_import())
        .with_default(false)
        .prompt()
        .map_err(|_| AppError::Message("Confirmation failed".to_string()))?;

    if !confirm {
        println!("\n{}", info(texts::cancelled()));
        pause();
        return Ok(());
    }

    let state = get_state()?;
    let backup_id = ConfigService::import_config_from_path(file_path, &state)?;

    println!("\n{}", success(&texts::imported_from(path)));
    println!("{}", info(&format!("Backup created: {}", backup_id)));
    pause();
    Ok(())
}

fn backup_config_interactive() -> Result<(), AppError> {
    clear_screen();
    println!(
        "\n{}",
        highlight(texts::config_backup().trim_start_matches("💾 "))
    );
    println!("{}", "─".repeat(60));

    // 询问是否使用自定义名称
    let use_custom_name = Confirm::new("是否使用自定义备份名称？")
        .with_default(false)
        .with_help_message("自定义名称可以帮助您识别备份用途，如 'before-update'")
        .prompt()
        .map_err(|_| AppError::Message("Confirmation failed".to_string()))?;

    let custom_name = if use_custom_name {
        Some(
            Text::new("请输入备份名称：")
                .with_help_message("仅支持字母、数字、短横线和下划线")
                .prompt()
                .map_err(|e| AppError::Message(format!("Input failed: {}", e)))?
                .trim()
                .to_string(),
        )
    } else {
        None
    };

    let config_path = get_app_config_path();
    let backup_id = ConfigService::create_backup(&config_path, custom_name)?;

    println!("\n{}", success(&texts::backup_created(&backup_id)));

    // 显示备份文件完整路径
    let backup_dir = config_path.parent().unwrap().join("backups");
    let backup_file = backup_dir.join(format!("{}.json", backup_id));
    println!("{}", info(&format!("位置: {}", backup_file.display())));

    pause();
    Ok(())
}

fn restore_config_interactive() -> Result<(), AppError> {
    clear_screen();
    println!(
        "\n{}",
        highlight(texts::config_restore().trim_start_matches("♻️  "))
    );
    println!("{}", "─".repeat(60));

    // 获取备份列表
    let config_path = get_app_config_path();
    let backups = ConfigService::list_backups(&config_path)?;

    if backups.is_empty() {
        println!("\n{}", info("暂无可用备份"));
        println!("{}", info("提示：使用 '💾 备份配置' 创建备份"));
        pause();
        return Ok(());
    }

    // 显示备份列表供选择
    println!("\n找到 {} 个备份：", backups.len());
    println!();

    let choices: Vec<String> = backups
        .iter()
        .map(|b| format!("{} - {}", b.display_name, b.id))
        .collect();

    let selection = Select::new("选择要恢复的备份：", choices)
        .prompt()
        .map_err(|_| AppError::Message("Selection cancelled".to_string()))?;

    // 从选择中提取备份 ID
    let selected_backup = backups
        .iter()
        .find(|b| selection.contains(&b.id))
        .ok_or_else(|| AppError::Message("无效的选择".to_string()))?;

    println!();
    println!("{}", highlight("警告："));
    println!("这将使用所选备份替换当前配置");
    println!("当前配置会先自动备份");
    println!();

    let confirm = Confirm::new("确认恢复？")
        .with_default(false)
        .prompt()
        .map_err(|_| AppError::Message("Confirmation failed".to_string()))?;

    if !confirm {
        println!("\n{}", info(texts::cancelled()));
        pause();
        return Ok(());
    }

    let state = get_state()?;
    let pre_restore_backup = ConfigService::restore_from_backup_id(&selected_backup.id, &state)?;

    println!(
        "\n{}",
        success(&format!("✓ 已从备份恢复: {}", selected_backup.display_name))
    );
    if !pre_restore_backup.is_empty() {
        println!(
            "{}",
            info(&format!("  恢复前配置已备份: {}", pre_restore_backup))
        );
    }
    println!("\n{}", info("注意：重启 CLI 客户端以应用更改"));

    pause();
    Ok(())
}

fn validate_config_interactive() -> Result<(), AppError> {
    clear_screen();
    let config_path = get_app_config_path();

    println!(
        "\n{}",
        highlight(texts::config_validate().trim_start_matches("✓ "))
    );
    println!("{}", "─".repeat(60));

    if !config_path.exists() {
        return Err(AppError::Message("Config file does not exist".to_string()));
    }

    let content = std::fs::read_to_string(&config_path)
        .map_err(|e| AppError::Message(format!("Failed to read config: {}", e)))?;

    let _: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| AppError::Message(format!("Invalid JSON: {}", e)))?;

    let config: MultiAppConfig = serde_json::from_str(&content)
        .map_err(|e| AppError::Message(format!("Invalid config structure: {}", e)))?;

    println!("{}", success(texts::config_valid()));
    println!();

    let claude_count = config
        .apps
        .get("claude")
        .map(|m| m.providers.len())
        .unwrap_or(0);
    let codex_count = config
        .apps
        .get("codex")
        .map(|m| m.providers.len())
        .unwrap_or(0);
    let gemini_count = config
        .apps
        .get("gemini")
        .map(|m| m.providers.len())
        .unwrap_or(0);
    let mcp_count = config.mcp.servers.as_ref().map(|s| s.len()).unwrap_or(0);

    println!("Claude providers: {}", claude_count);
    println!("Codex providers:  {}", codex_count);
    println!("Gemini providers: {}", gemini_count);
    println!("MCP servers:      {}", mcp_count);

    pause();
    Ok(())
}

fn reset_config_interactive() -> Result<(), AppError> {
    clear_screen();
    let confirm = Confirm::new(texts::confirm_reset())
        .with_default(false)
        .prompt()
        .map_err(|_| AppError::Message("Confirmation failed".to_string()))?;

    if !confirm {
        println!("\n{}", info(texts::cancelled()));
        pause();
        return Ok(());
    }

    let config_path = get_app_config_path();

    let backup_id = ConfigService::create_backup(&config_path, None)?;

    if config_path.exists() {
        std::fs::remove_file(&config_path)
            .map_err(|e| AppError::Message(format!("Failed to delete config: {}", e)))?;
    }

    let _ = MultiAppConfig::load()?;

    println!("\n{}", success(texts::config_reset_done()));
    println!(
        "{}",
        info(&format!("Previous config backed up: {}", backup_id))
    );
    pause();
    Ok(())
}
