use indicatif::{ProgressBar, ProgressStyle};

use crate::cli::i18n::{current_language, set_language, texts, Language};
use crate::cli::ui::{error, highlight, info, success};
use crate::error::AppError;
use crate::services::{ApplyResult, UpdateService};

use super::utils::{clear_screen, pause, prompt_select};

pub fn settings_menu() -> Result<(), AppError> {
    loop {
        clear_screen();
        println!("\n{}", highlight(texts::settings_title()));
        println!("{}", texts::tui_rule(60));

        let lang = current_language();
        println!(
            "{}: {}",
            texts::current_language_label(),
            highlight(lang.display_name())
        );
        println!();

        let choices = vec![
            texts::change_language(),
            texts::check_for_updates(),
            texts::back_to_main(),
        ];

        let Some(choice) = prompt_select(texts::choose_action(), choices)? else {
            break;
        };

        if choice == texts::change_language() {
            change_language_interactive()?;
        } else if choice == texts::check_for_updates() {
            if let Err(e) = check_for_updates_interactive() {
                println!("\n{}", error(&texts::update_error(&e.to_string())));
                pause();
            }
        } else {
            break;
        }
    }

    Ok(())
}

fn change_language_interactive() -> Result<(), AppError> {
    clear_screen();
    let languages = vec![Language::English, Language::Chinese];

    let Some(selected) = prompt_select(texts::select_language(), languages)? else {
        return Ok(());
    };

    set_language(selected)?;

    println!("\n{}", success(texts::language_changed()));
    pause();

    Ok(())
}

fn check_for_updates_interactive() -> Result<(), AppError> {
    let current = UpdateService::current_version();
    println!(
        "\n{} {}",
        info(texts::update_current_version()),
        highlight(current)
    );

    println!("{}", info(texts::update_checking()));

    let runtime = tokio::runtime::Runtime::new()
        .map_err(|e| AppError::Message(format!("Failed to create runtime: {}", e)))?;

    let release = runtime.block_on(UpdateService::check_latest())?;
    let latest = release.version();

    println!(
        "{} {}",
        info(texts::update_latest_version()),
        highlight(latest)
    );

    if !UpdateService::is_newer(current, latest) {
        println!("\n{}", success(texts::update_up_to_date()));
        pause();
        return Ok(());
    }

    println!("\n{}", success(texts::update_available()));
    println!(
        "{} {} → {}",
        info(texts::update_available_version()),
        highlight(current),
        highlight(latest)
    );

    let asset = UpdateService::find_matching_asset(&release).ok_or_else(|| {
        AppError::Message(texts::update_no_asset_for_platform().to_string())
    })?;

    println!(
        "{} {} ({:.2} MB)",
        info(texts::update_asset_found()),
        asset.name,
        asset.size as f64 / 1024.0 / 1024.0
    );

    let confirm = inquire::Confirm::new(texts::update_confirm_download())
        .with_default(true)
        .prompt()
        .map_err(|_| AppError::Message("Cancelled".to_string()))?;

    if !confirm {
        println!("{}", info(texts::cancelled()));
        pause();
        return Ok(());
    }

    let pb = ProgressBar::new(100);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} {msg} [{wide_bar:.cyan/blue}] {percent}%")
            .unwrap()
            .progress_chars("█▉▊▋▌▍▎▏ "),
    );
    pb.set_message(texts::update_downloading().to_string());

    let pb_clone = pb.clone();

    let downloaded_path = runtime.block_on(UpdateService::download_asset(asset, move |progress| {
        pb_clone.set_position(progress as u64);
    }))?;

    pb.finish_with_message(texts::update_download_complete().to_string());

    println!("{}", info(texts::update_extracting()));
    let binary_path = UpdateService::extract_binary(&downloaded_path)?;

    println!("{}", info(texts::update_applying()));

    match UpdateService::apply_update(&binary_path)? {
        ApplyResult::Applied { requires_restart } => {
            if requires_restart {
                println!("\n{}", success(texts::update_success_restart()));
            } else {
                println!("\n{}", success(texts::update_success()));
            }
        }
        ApplyResult::ManualRequired { path, instructions } => {
            println!("\n{}", info(texts::update_manual_required()));
            println!(
                "{}: {}",
                highlight(texts::update_downloaded_to()),
                path.display()
            );
            println!("{}", info(texts::update_run_command()));
            println!("  {}", highlight(&instructions));
        }
    }

    pause();
    Ok(())
}
