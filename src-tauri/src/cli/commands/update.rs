use clap::Args;
use indicatif::{ProgressBar, ProgressStyle};

use crate::cli::i18n::texts;
use crate::cli::ui::{highlight, info, success};
use crate::error::AppError;
use crate::services::{ApplyResult, UpdateService};

#[derive(Args)]
pub struct UpdateCommand {
    /// Only check for updates without installing
    #[arg(short = 'c', long)]
    pub check: bool,

    /// Skip confirmation prompt
    #[arg(short = 'y', long)]
    pub yes: bool,

    /// Force reinstall even if already up to date
    #[arg(long)]
    pub force: bool,
}

pub fn execute(cmd: UpdateCommand) -> Result<(), AppError> {
    tokio::runtime::Runtime::new()
        .map_err(|e| AppError::Message(format!("Failed to create runtime: {}", e)))?
        .block_on(execute_async(cmd))
}

async fn execute_async(cmd: UpdateCommand) -> Result<(), AppError> {
    let current = UpdateService::current_version();
    println!(
        "{} {}",
        info(texts::update_current_version()),
        highlight(current)
    );

    println!("{}", info(texts::update_checking()));

    let release = UpdateService::check_latest().await?;
    let latest = release.version();

    let is_newer = UpdateService::is_newer(current, latest);

    if cmd.check {
        println!(
            "{} {}",
            info(texts::update_latest_version()),
            highlight(latest)
        );

        if is_newer {
            println!("{}", success(texts::update_available()));
        } else {
            println!("{}", success(texts::update_up_to_date()));
        }
        return Ok(());
    }

    if !is_newer && !cmd.force {
        println!("{}", success(&texts::update_already_latest(current)));
        return Ok(());
    }

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

    if !cmd.yes {
        let confirm = inquire::Confirm::new(texts::update_confirm_download())
            .with_default(true)
            .prompt()
            .map_err(|_| AppError::Message("Cancelled".to_string()))?;

        if !confirm {
            println!("{}", info(texts::cancelled()));
            return Ok(());
        }
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

    let downloaded_path = UpdateService::download_asset(asset, move |progress| {
        pb_clone.set_position(progress as u64);
    })
    .await?;

    pb.finish_with_message(texts::update_download_complete().to_string());

    println!("{}", info(texts::update_extracting()));
    let binary_path = UpdateService::extract_binary(&downloaded_path)?;

    println!("{}", info(texts::update_applying()));

    match UpdateService::apply_update(&binary_path)? {
        ApplyResult::Applied { requires_restart } => {
            if requires_restart {
                println!("{}", success(texts::update_success_restart()));
            } else {
                println!("{}", success(texts::update_success()));
            }
        }
        ApplyResult::ManualRequired { path, instructions } => {
            println!("{}", info(texts::update_manual_required()));
            println!("{}", highlight(&format!("{}: {}", texts::update_downloaded_to(), path.display())));
            println!("{}", info(texts::update_run_command()));
            println!("  {}", highlight(&instructions));
        }
    }

    Ok(())
}
