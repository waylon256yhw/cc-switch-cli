use reqwest::Client;
use serde::Deserialize;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::error::AppError;

const GITHUB_API_URL: &str = "https://api.github.com/repos/saladday/cc-switch-cli/releases/latest";
const USER_AGENT: &str = "cc-switch-updater/1.0";
const REQUEST_TIMEOUT_SECS: u64 = 30;

#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseInfo {
    pub tag_name: String,
    pub html_url: String,
    pub published_at: String,
    pub assets: Vec<ReleaseAsset>,
}

impl ReleaseInfo {
    pub fn version(&self) -> &str {
        self.tag_name.strip_prefix('v').unwrap_or(&self.tag_name)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseAsset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
}

#[derive(Debug)]
pub enum ApplyResult {
    Applied { requires_restart: bool },
    ManualRequired { path: PathBuf, instructions: String },
}

pub struct UpdateService;

impl UpdateService {
    pub fn current_version() -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    pub async fn check_latest() -> Result<ReleaseInfo, AppError> {
        let client = Self::build_client()?;

        let response = client
            .get(GITHUB_API_URL)
            .send()
            .await
            .map_err(|e| AppError::Message(format!("Failed to fetch release info: {}", e)))?;

        if !response.status().is_success() {
            return Err(AppError::Message(format!(
                "GitHub API returned status: {}",
                response.status()
            )));
        }

        response
            .json::<ReleaseInfo>()
            .await
            .map_err(|e| AppError::Message(format!("Failed to parse release info: {}", e)))
    }

    pub fn is_newer(current: &str, latest: &str) -> bool {
        let current = semver::Version::parse(current).ok();
        let latest = semver::Version::parse(latest).ok();

        match (current, latest) {
            (Some(c), Some(l)) => l > c,
            _ => false,
        }
    }

    pub fn detect_platform_suffix() -> &'static str {
        let os = std::env::consts::OS;
        let arch = std::env::consts::ARCH;

        match (os, arch) {
            ("linux", "x86_64") => "linux-x64-musl",
            ("linux", "aarch64") => "linux-arm64-musl",
            ("macos", _) => "darwin-universal",
            ("windows", "x86_64") => "windows-x64",
            _ => "unknown",
        }
    }

    pub fn find_matching_asset(release: &ReleaseInfo) -> Option<&ReleaseAsset> {
        let suffix = Self::detect_platform_suffix();
        release
            .assets
            .iter()
            .find(|a| a.name.contains(suffix) && !a.name.ends_with(".sha256"))
    }

    pub async fn download_asset<F>(
        asset: &ReleaseAsset,
        on_progress: F,
    ) -> Result<PathBuf, AppError>
    where
        F: Fn(f32) + Send,
    {
        let client = Self::build_client()?;

        let response = client
            .get(&asset.browser_download_url)
            .send()
            .await
            .map_err(|e| AppError::Message(format!("Failed to start download: {}", e)))?;

        if !response.status().is_success() {
            return Err(AppError::Message(format!(
                "Download failed with status: {}",
                response.status()
            )));
        }

        let total_size = response.content_length().unwrap_or(asset.size);

        // Try to download alongside current exe first (avoids cross-filesystem rename issues)
        // Fall back to temp dir if that fails
        let temp_path = Self::get_download_path()?;

        let mut file = std::fs::File::create(&temp_path)
            .map_err(|e| AppError::io(&temp_path, e))?;

        let mut downloaded: u64 = 0;
        let mut stream = response.bytes_stream();

        use futures::StreamExt;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk
                .map_err(|e| AppError::Message(format!("Download error: {}", e)))?;

            file.write_all(&chunk)
                .map_err(|e| AppError::io(&temp_path, e))?;

            downloaded += chunk.len() as u64;
            let progress = if total_size > 0 {
                (downloaded as f32 / total_size as f32) * 100.0
            } else {
                0.0
            };
            on_progress(progress.min(100.0));
        }

        file.flush().map_err(|e| AppError::io(&temp_path, e))?;
        drop(file);

        Ok(temp_path)
    }

    /// Get the best path for downloading the update.
    /// Prefers the same directory as the current executable to avoid cross-filesystem rename issues.
    fn get_download_path() -> Result<PathBuf, AppError> {
        let file_name = if cfg!(windows) {
            "cc-switch.exe.update"
        } else {
            "cc-switch.update"
        };

        // Try current exe directory first
        if let Ok(current_exe) = std::env::current_exe() {
            if let Some(parent) = current_exe.parent() {
                let candidate = parent.join(file_name);
                // Check if we can write there
                if std::fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .open(&candidate)
                    .is_ok()
                {
                    return Ok(candidate);
                }
            }
        }

        // Fall back to temp dir
        Ok(std::env::temp_dir().join(file_name))
    }

    /// Extract binary from downloaded archive (.tar.gz or .zip)
    pub fn extract_binary(archive_path: &Path) -> Result<PathBuf, AppError> {
        use flate2::read::GzDecoder;
        use std::fs;
        use tar::Archive;

        let file = fs::File::open(archive_path)
            .map_err(|e| AppError::io(archive_path, e))?;

        let binary_name = if cfg!(windows) { "cc-switch.exe" } else { "cc-switch" };
        let output_name = if cfg!(windows) { "cc-switch-new.exe" } else { "cc-switch-new" };
        let output_path = archive_path.with_file_name(output_name);

        let gz = GzDecoder::new(file);
        let mut archive = Archive::new(gz);

        for entry in archive.entries().map_err(|e| AppError::Message(format!("Failed to read archive: {}", e)))? {
            let mut entry = entry.map_err(|e| AppError::Message(format!("Failed to read entry: {}", e)))?;
            let path = entry.path().map_err(|e| AppError::Message(format!("Invalid path: {}", e)))?;

            if path.file_name().map(|n| n == binary_name).unwrap_or(false) {
                entry.unpack(&output_path)
                    .map_err(|e| AppError::Message(format!("Failed to extract: {}", e)))?;

                // Clean up archive
                fs::remove_file(archive_path).ok();

                return Ok(output_path);
            }
        }

        Err(AppError::Message(format!("Binary '{}' not found in archive", binary_name)))
    }

    pub fn apply_update(downloaded_path: &Path) -> Result<ApplyResult, AppError> {
        let current_exe = std::env::current_exe()
            .map_err(|e| AppError::Message(format!("Cannot determine current executable: {}", e)))?;

        #[cfg(unix)]
        {
            Self::apply_update_unix(downloaded_path, &current_exe)
        }

        #[cfg(windows)]
        {
            Self::apply_update_windows(downloaded_path, &current_exe)
        }
    }

    #[cfg(unix)]
    fn apply_update_unix(downloaded_path: &Path, current_exe: &Path) -> Result<ApplyResult, AppError> {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        // Check if current binary is writable
        let metadata = fs::metadata(current_exe)
            .map_err(|e| AppError::io(current_exe, e))?;

        let is_writable = metadata.permissions().mode() & 0o200 != 0
            && fs::OpenOptions::new()
                .write(true)
                .open(current_exe)
                .is_ok();

        if is_writable {
            // Backup current executable
            let backup_path = current_exe.with_extension("old");
            if backup_path.exists() {
                fs::remove_file(&backup_path).ok();
            }

            fs::rename(current_exe, &backup_path)
                .map_err(|e| AppError::io(current_exe, e))?;

            // Move new executable (with copy fallback for cross-filesystem)
            if let Err(rename_err) = fs::rename(downloaded_path, current_exe) {
                // Check if it's a cross-device link error (EXDEV)
                if rename_err.raw_os_error() == Some(libc::EXDEV) {
                    // Fall back to copy
                    if let Err(copy_err) = Self::copy_file(downloaded_path, current_exe) {
                        // Restore backup on failure
                        let _ = fs::rename(&backup_path, current_exe);
                        return Err(copy_err);
                    }
                    // Remove the temp file after successful copy
                    fs::remove_file(downloaded_path).ok();
                } else {
                    // Other error, try to restore backup
                    let _ = fs::rename(&backup_path, current_exe);
                    return Err(AppError::io(current_exe, rename_err));
                }
            }

            // Make executable
            let mut perms = fs::metadata(current_exe)
                .map_err(|e| AppError::io(current_exe, e))?
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(current_exe, perms)
                .map_err(|e| AppError::io(current_exe, e))?;

            Ok(ApplyResult::Applied { requires_restart: true })
        } else {
            // Need sudo/manual installation
            let instructions = format!(
                "sudo mv {} {}",
                downloaded_path.display(),
                current_exe.display()
            );

            Ok(ApplyResult::ManualRequired {
                path: downloaded_path.to_path_buf(),
                instructions,
            })
        }
    }

    /// Copy file with proper error handling (used as fallback for cross-filesystem moves)
    #[cfg(unix)]
    fn copy_file(src: &Path, dst: &Path) -> Result<(), AppError> {
        use std::fs;

        let mut src_file = fs::File::open(src)
            .map_err(|e| AppError::io(src, e))?;
        let mut dst_file = fs::File::create(dst)
            .map_err(|e| AppError::io(dst, e))?;

        let mut buffer = [0u8; 64 * 1024]; // 64KB buffer
        loop {
            let bytes_read = src_file.read(&mut buffer)
                .map_err(|e| AppError::io(src, e))?;
            if bytes_read == 0 {
                break;
            }
            dst_file.write_all(&buffer[..bytes_read])
                .map_err(|e| AppError::io(dst, e))?;
        }

        dst_file.flush().map_err(|e| AppError::io(dst, e))?;
        dst_file.sync_all().map_err(|e| AppError::io(dst, e))?;

        Ok(())
    }

    #[cfg(windows)]
    fn apply_update_windows(downloaded_path: &Path, current_exe: &Path) -> Result<ApplyResult, AppError> {
        // On Windows, we can't replace a running executable directly
        // Provide manual instructions
        let instructions = format!(
            "1. Close cc-switch\n2. Copy {} to {}\n3. Restart cc-switch",
            downloaded_path.display(),
            current_exe.display()
        );

        Ok(ApplyResult::ManualRequired {
            path: downloaded_path.to_path_buf(),
            instructions,
        })
    }

    fn build_client() -> Result<Client, AppError> {
        Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .user_agent(USER_AGENT)
            .build()
            .map_err(|e| AppError::Message(format!("Failed to create HTTP client: {}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_newer() {
        assert!(UpdateService::is_newer("1.0.0", "1.0.1"));
        assert!(UpdateService::is_newer("1.0.0", "1.1.0"));
        assert!(UpdateService::is_newer("1.0.0", "2.0.0"));
        assert!(!UpdateService::is_newer("1.0.1", "1.0.0"));
        assert!(!UpdateService::is_newer("1.0.0", "1.0.0"));
    }

    #[test]
    fn test_detect_platform_suffix() {
        let suffix = UpdateService::detect_platform_suffix();
        assert!(!suffix.is_empty());
        // Should be one of the known platforms or "unknown"
        let known = ["linux-x64-musl", "linux-arm64-musl", "darwin-universal", "windows-x64", "unknown"];
        assert!(known.contains(&suffix));
    }

    #[test]
    fn test_current_version() {
        let version = UpdateService::current_version();
        assert!(!version.is_empty());
        assert!(semver::Version::parse(version).is_ok());
    }
}
