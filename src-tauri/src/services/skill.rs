//! Skills service layer
//!
//! This aligns with upstream `cc-switch` v3.10+ "SSOT + sync" architecture, while keeping
//! `cc-switch-cli` storage file-based (no DB for now):
//! - SSOT directory: `~/.cc-switch/skills/`
//! - Index/config: `~/.cc-switch/skills.json`

use chrono::{DateTime, Utc};
use futures::future::join_all;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use tokio::time::timeout;

use crate::app_config::AppType;
use crate::config::{copy_file, get_app_config_dir, write_json_file};
use crate::error::{format_skill_error, AppError};

const SKILLS_INDEX_VERSION: u32 = 1;

fn default_skills_index_version() -> u32 {
    SKILLS_INDEX_VERSION
}

fn get_skills_store_path() -> PathBuf {
    get_app_config_dir().join("skills.json")
}

// ============================================================================
// Legacy (v2) store structures - kept for backward compatibility
// ============================================================================

/// Skill repository configuration (legacy, kept for backward compatibility).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRepo {
    /// GitHub 用户/组织名
    pub owner: String,
    /// 仓库名称
    pub name: String,
    /// 分支 (默认 "main")
    pub branch: String,
    /// 是否启用
    pub enabled: bool,
    /// 技能所在的子目录路径 (可选, 如 "skills", "my-skills/subdir")
    #[serde(rename = "skillsPath")]
    pub skills_path: Option<String>,
}

/// Legacy install state: directory -> installed timestamp (Claude-only era).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillState {
    /// 是否已安装
    pub installed: bool,
    /// 安装时间
    #[serde(rename = "installedAt")]
    pub installed_at: DateTime<Utc>,
}

/// Legacy persistent store (was embedded in config.json in older CLI versions).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillStore {
    /// directory -> 安装状态
    pub skills: HashMap<String, SkillState>,
    /// 仓库列表
    pub repos: Vec<SkillRepo>,
}

impl Default for SkillStore {
    fn default() -> Self {
        SkillStore {
            skills: HashMap::new(),
            // Keep aligned with upstream defaults where possible.
            repos: vec![
                SkillRepo {
                    owner: "anthropics".to_string(),
                    name: "skills".to_string(),
                    branch: "main".to_string(),
                    enabled: true,
                    skills_path: None,
                },
                SkillRepo {
                    owner: "ComposioHQ".to_string(),
                    name: "awesome-claude-skills".to_string(),
                    branch: "master".to_string(),
                    enabled: true,
                    skills_path: None,
                },
                SkillRepo {
                    owner: "cexll".to_string(),
                    name: "myclaude".to_string(),
                    branch: "master".to_string(),
                    enabled: true,
                    // Keep our historical default: scan skills/ subdir.
                    skills_path: Some("skills".to_string()),
                },
                SkillRepo {
                    owner: "JimLiu".to_string(),
                    name: "baoyu-skills".to_string(),
                    branch: "main".to_string(),
                    enabled: true,
                    skills_path: None,
                },
            ],
        }
    }
}

// ============================================================================
// New (Phase 3) SSOT-based model persisted to ~/.cc-switch/skills.json (no DB)
// ============================================================================

/// Skill sync method (upstream-aligned).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum SyncMethod {
    /// Auto choose: prefer symlink, fallback to copy.
    #[default]
    Auto,
    /// Always use symlink.
    Symlink,
    /// Always use directory copy.
    Copy,
}

/// Skill enablement per app (stored in skills.json).
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct SkillApps {
    #[serde(default)]
    pub claude: bool,
    #[serde(default)]
    pub codex: bool,
    #[serde(default)]
    pub gemini: bool,
}

impl SkillApps {
    pub fn only(app: &AppType) -> Self {
        let mut apps = SkillApps::default();
        apps.set_enabled_for(app, true);
        apps
    }

    pub fn is_enabled_for(&self, app: &AppType) -> bool {
        match app {
            AppType::Claude => self.claude,
            AppType::Codex => self.codex,
            AppType::Gemini => self.gemini,
        }
    }

    pub fn set_enabled_for(&mut self, app: &AppType, enabled: bool) {
        match app {
            AppType::Claude => self.claude = enabled,
            AppType::Codex => self.codex = enabled,
            AppType::Gemini => self.gemini = enabled,
        }
    }

    pub fn merge_enabled(&mut self, other: &SkillApps) {
        self.claude |= other.claude;
        self.codex |= other.codex;
        self.gemini |= other.gemini;
    }
}

/// Installed skill record (stored in skills.json).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledSkill {
    /// Unique id: "owner/name:directory" or "local:directory"
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub directory: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "readmeUrl")]
    pub readme_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "repoOwner")]
    pub repo_owner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "repoName")]
    pub repo_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "repoBranch")]
    pub repo_branch: Option<String>,
    pub apps: SkillApps,
    pub installed_at: i64,
}

/// Unmanaged skill discovered in app directories.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnmanagedSkill {
    pub directory: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub found_in: Vec<String>,
}

/// skills.json (SSOT index; no DB).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillsIndex {
    #[serde(default = "default_skills_index_version")]
    pub version: u32,
    #[serde(default)]
    pub sync_method: SyncMethod,
    #[serde(default)]
    pub repos: Vec<SkillRepo>,
    /// directory -> record
    #[serde(default)]
    pub skills: HashMap<String, InstalledSkill>,
    /// One-time SSOT migration flag (scan app dirs -> copy into SSOT -> build records).
    #[serde(default)]
    pub ssot_migration_pending: bool,
}

impl Default for SkillsIndex {
    fn default() -> Self {
        Self {
            version: SKILLS_INDEX_VERSION,
            sync_method: SyncMethod::default(),
            repos: SkillStore::default().repos,
            skills: HashMap::new(),
            ssot_migration_pending: false,
        }
    }
}

// ============================================================================
// Discovery types (repo scanning)
// ============================================================================

/// Discoverable skill (from GitHub repos).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverableSkill {
    /// Unique key: "owner/name:directory"
    pub key: String,
    pub name: String,
    pub description: String,
    /// Directory name (the final path segment)
    pub directory: String,
    #[serde(rename = "readmeUrl")]
    pub readme_url: Option<String>,
    #[serde(rename = "repoOwner")]
    pub repo_owner: String,
    #[serde(rename = "repoName")]
    pub repo_name: String,
    #[serde(rename = "repoBranch")]
    pub repo_branch: String,
    /// Optional subdir path inside repo (our CLI extension)
    #[serde(rename = "skillsPath")]
    pub skills_path: Option<String>,
}

/// CLI-friendly skill object (discoverable + installed flag).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    pub key: String,
    pub name: String,
    pub description: String,
    pub directory: String,
    #[serde(rename = "readmeUrl")]
    pub readme_url: Option<String>,
    pub installed: bool,
    #[serde(rename = "repoOwner")]
    pub repo_owner: Option<String>,
    #[serde(rename = "repoName")]
    pub repo_name: Option<String>,
    #[serde(rename = "repoBranch")]
    pub repo_branch: Option<String>,
    #[serde(rename = "skillsPath")]
    pub skills_path: Option<String>,
}

/// Skill metadata extracted from SKILL.md YAML front matter.
#[derive(Debug, Clone, Deserialize)]
pub struct SkillMetadata {
    pub name: Option<String>,
    pub description: Option<String>,
}

// ============================================================================
// SkillService
// ============================================================================

pub struct SkillService {
    http_client: Client,
}

impl SkillService {
    pub fn new() -> Result<Self, AppError> {
        let http_client = Client::builder()
            .user_agent("cc-switch")
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| {
                AppError::localized(
                    "skills.http_client_failed",
                    format!("创建 HTTP 客户端失败: {e}"),
                    format!("Failed to create HTTP client: {e}"),
                )
            })?;

        Ok(Self { http_client })
    }

    // ---------------------------------------------------------------------
    // Paths
    // ---------------------------------------------------------------------

    pub fn get_ssot_dir() -> Result<PathBuf, AppError> {
        let dir = get_app_config_dir().join("skills");
        fs::create_dir_all(&dir).map_err(|e| AppError::io(&dir, e))?;
        Ok(dir)
    }

    pub fn get_app_skills_dir(app: &AppType) -> Result<PathBuf, AppError> {
        // Override directories follow the same pattern as upstream: <override>/skills
        match app {
            AppType::Claude => {
                if let Some(custom) = crate::settings::get_claude_override_dir() {
                    return Ok(custom.join("skills"));
                }
            }
            AppType::Codex => {
                if let Some(custom) = crate::settings::get_codex_override_dir() {
                    return Ok(custom.join("skills"));
                }
            }
            AppType::Gemini => {
                if let Some(custom) = crate::settings::get_gemini_override_dir() {
                    return Ok(custom.join("skills"));
                }
            }
        }

        let home = dirs::home_dir().ok_or_else(|| {
            AppError::Message(format_skill_error(
                "GET_HOME_DIR_FAILED",
                &[],
                Some("checkPermission"),
            ))
        })?;

        Ok(match app {
            AppType::Claude => home.join(".claude").join("skills"),
            AppType::Codex => home.join(".codex").join("skills"),
            AppType::Gemini => home.join(".gemini").join("skills"),
        })
    }

    // ---------------------------------------------------------------------
    // skills.json store I/O
    // ---------------------------------------------------------------------

    pub fn load_index() -> Result<SkillsIndex, AppError> {
        let path = get_skills_store_path();
        if !path.exists() {
            // Fresh install: create index with migration pending (auto import from app dirs).
            let mut index = SkillsIndex::default();
            index.ssot_migration_pending = true;
            Self::save_index(&index)?;
            return Ok(index);
        }

        let content = fs::read_to_string(&path).map_err(|e| AppError::io(&path, e))?;
        let value: serde_json::Value =
            serde_json::from_str(&content).map_err(|e| AppError::json(&path, e))?;

        // If version is present, treat as v3 index; otherwise attempt legacy conversion.
        if value.get("version").and_then(|v| v.as_u64()).is_some() {
            let mut index: SkillsIndex =
                serde_json::from_value(value).map_err(|e| AppError::json(&path, e))?;
            if index.version == 0 {
                index.version = SKILLS_INDEX_VERSION;
            }
            return Ok(index);
        }

        // Legacy file: `SkillStore` (Claude-only) -> convert to `SkillsIndex`
        let legacy: SkillStore =
            serde_json::from_value(value).map_err(|e| AppError::json(&path, e))?;

        // Backup before overwriting.
        let backup_path = get_app_config_dir().join("skills.json.bak");
        if let Err(err) = copy_file(&path, &backup_path) {
            log::warn!("备份旧 skills.json 失败: {err}");
        }

        let mut index = SkillsIndex {
            version: SKILLS_INDEX_VERSION,
            sync_method: SyncMethod::Auto,
            repos: legacy.repos,
            skills: HashMap::new(),
            ssot_migration_pending: true,
        };

        for (directory, state) in legacy.skills.into_iter() {
            if !state.installed {
                continue;
            }
            let installed_at = state.installed_at.timestamp();
            let record = InstalledSkill {
                id: format!("local:{directory}"),
                name: directory.clone(),
                description: None,
                directory: directory.clone(),
                readme_url: None,
                repo_owner: None,
                repo_name: None,
                repo_branch: None,
                apps: SkillApps::only(&AppType::Claude),
                installed_at,
            };
            index.skills.insert(directory, record);
        }

        Self::save_index(&index)?;
        Ok(index)
    }

    pub fn save_index(index: &SkillsIndex) -> Result<(), AppError> {
        let path = get_skills_store_path();
        write_json_file(&path, index)
    }

    // ---------------------------------------------------------------------
    // One-time SSOT migration (scan app dirs -> copy to SSOT -> record in index)
    // ---------------------------------------------------------------------

    pub fn migrate_ssot_if_pending(index: &mut SkillsIndex) -> Result<usize, AppError> {
        if !index.ssot_migration_pending {
            return Ok(0);
        }

        let ssot_dir = Self::get_ssot_dir()?;
        let mut created = 0usize;

        // Safety guard (upstream-aligned):
        // - If we already have managed skills in the index, do NOT auto-import everything
        //   from app dirs (that could unexpectedly "claim" user directories as managed).
        // - Instead, only try to populate SSOT for the already-managed skills (best effort),
        //   then clear the pending flag.
        if !index.skills.is_empty() {
            for (directory, record) in index.skills.iter_mut() {
                let dest = ssot_dir.join(directory);
                if dest.exists() {
                    continue;
                }

                // Prefer looking in apps where this skill is enabled; fallback to all apps.
                let mut candidates: Vec<AppType> =
                    [AppType::Claude, AppType::Codex, AppType::Gemini]
                        .into_iter()
                        .filter(|app| record.apps.is_enabled_for(app))
                        .collect();
                if candidates.is_empty() {
                    candidates = vec![AppType::Claude, AppType::Codex, AppType::Gemini];
                }

                let mut source: Option<PathBuf> = None;
                for app in candidates {
                    let app_dir = match Self::get_app_skills_dir(&app) {
                        Ok(d) => d,
                        Err(_) => continue,
                    };
                    let skill_path = app_dir.join(directory);
                    if skill_path.exists() {
                        source = Some(skill_path);
                        break;
                    }
                }

                match source {
                    Some(source) => {
                        Self::copy_dir_recursive(&source, &dest)?;
                        created += 1;

                        // Backfill metadata if missing.
                        let skill_md = dest.join("SKILL.md");
                        if skill_md.exists() {
                            if let Ok(meta) = Self::parse_skill_metadata_static(&skill_md) {
                                if record.name.trim().is_empty()
                                    || record.name.eq_ignore_ascii_case(&record.directory)
                                {
                                    record.name =
                                        meta.name.unwrap_or_else(|| record.directory.clone());
                                }
                                if record.description.is_none() {
                                    record.description = meta.description;
                                }
                            }
                        }
                    }
                    None => {
                        log::warn!(
                            "SSOT 迁移: 未找到技能目录来源（directory={directory}），已跳过复制"
                        );
                    }
                }
            }

            index.ssot_migration_pending = false;
            Self::save_index(index)?;
            return Ok(created);
        }

        let mut discovered: HashMap<String, SkillApps> = HashMap::new();

        for app in [AppType::Claude, AppType::Codex, AppType::Gemini] {
            let app_dir = match Self::get_app_skills_dir(&app) {
                Ok(d) => d,
                Err(_) => continue,
            };
            if !app_dir.exists() {
                continue;
            }

            for entry in fs::read_dir(&app_dir).map_err(|e| AppError::io(&app_dir, e))? {
                let entry = entry.map_err(|e| AppError::io(&app_dir, e))?;
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }

                let dir_name = entry.file_name().to_string_lossy().to_string();
                if dir_name.starts_with('.') {
                    continue;
                }

                // Copy to SSOT if needed.
                let ssot_path = ssot_dir.join(&dir_name);
                if !ssot_path.exists() {
                    Self::copy_dir_recursive(&path, &ssot_path)?;
                }

                discovered
                    .entry(dir_name)
                    .or_default()
                    .set_enabled_for(&app, true);
            }
        }

        // Upsert index records.
        for (directory, apps) in discovered {
            let ssot_path = ssot_dir.join(&directory);
            let skill_md = ssot_path.join("SKILL.md");
            let (name, description) = if skill_md.exists() {
                match Self::parse_skill_metadata_static(&skill_md) {
                    Ok(meta) => (
                        meta.name.unwrap_or_else(|| directory.clone()),
                        meta.description,
                    ),
                    Err(_) => (directory.clone(), None),
                }
            } else {
                (directory.clone(), None)
            };

            match index.skills.get_mut(&directory) {
                Some(existing) => {
                    existing.apps.merge_enabled(&apps);
                    if existing.name.trim().is_empty() {
                        existing.name = name;
                    }
                    if existing.description.is_none() {
                        existing.description = description;
                    }
                }
                None => {
                    index.skills.insert(
                        directory.clone(),
                        InstalledSkill {
                            id: format!("local:{directory}"),
                            name,
                            description,
                            directory: directory.clone(),
                            readme_url: None,
                            repo_owner: None,
                            repo_name: None,
                            repo_branch: None,
                            apps,
                            installed_at: Utc::now().timestamp(),
                        },
                    );
                    created += 1;
                }
            }
        }

        index.ssot_migration_pending = false;
        Self::save_index(index)?;
        Ok(created)
    }

    // ---------------------------------------------------------------------
    // Sync / remove (file operations)
    // ---------------------------------------------------------------------

    #[cfg(unix)]
    fn create_symlink(src: &Path, dest: &Path) -> Result<(), AppError> {
        std::os::unix::fs::symlink(src, dest).map_err(|e| AppError::IoContext {
            context: format!("创建符号链接失败 ({} -> {})", src.display(), dest.display()),
            source: e,
        })
    }

    #[cfg(windows)]
    fn create_symlink(src: &Path, dest: &Path) -> Result<(), AppError> {
        std::os::windows::fs::symlink_dir(src, dest).map_err(|e| AppError::IoContext {
            context: format!("创建符号链接失败 ({} -> {})", src.display(), dest.display()),
            source: e,
        })
    }

    fn is_symlink(path: &Path) -> bool {
        path.symlink_metadata()
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
    }

    fn remove_path(path: &Path) -> Result<(), AppError> {
        if Self::is_symlink(path) {
            #[cfg(unix)]
            fs::remove_file(path).map_err(|e| AppError::io(path, e))?;
            #[cfg(windows)]
            fs::remove_dir(path).map_err(|e| AppError::io(path, e))?;
            return Ok(());
        }

        if path.is_dir() {
            fs::remove_dir_all(path).map_err(|e| AppError::io(path, e))?;
        } else if path.exists() {
            fs::remove_file(path).map_err(|e| AppError::io(path, e))?;
        }
        Ok(())
    }

    pub fn sync_to_app_dir(
        directory: &str,
        app: &AppType,
        method: SyncMethod,
    ) -> Result<(), AppError> {
        let ssot_dir = Self::get_ssot_dir()?;
        let source = ssot_dir.join(directory);
        if !source.exists() {
            return Err(AppError::Message(format!(
                "Skill 不存在于 SSOT: {directory}"
            )));
        }

        let app_dir = Self::get_app_skills_dir(app)?;
        // D5: allow creating target app dirs during skills sync.
        fs::create_dir_all(&app_dir).map_err(|e| AppError::io(&app_dir, e))?;

        let dest = app_dir.join(directory);
        if dest.exists() || Self::is_symlink(&dest) {
            Self::remove_path(&dest)?;
        }

        match method {
            SyncMethod::Auto => match Self::create_symlink(&source, &dest) {
                Ok(()) => Ok(()),
                Err(err) => {
                    log::warn!(
                        "Symlink 创建失败，将回退到文件复制: {} -> {}. 错误: {err}",
                        source.display(),
                        dest.display()
                    );
                    Self::copy_dir_recursive(&source, &dest)
                }
            },
            SyncMethod::Symlink => Self::create_symlink(&source, &dest),
            SyncMethod::Copy => Self::copy_dir_recursive(&source, &dest),
        }
    }

    pub fn remove_from_app(directory: &str, app: &AppType) -> Result<(), AppError> {
        let app_dir = Self::get_app_skills_dir(app)?;
        let path = app_dir.join(directory);
        if path.exists() || Self::is_symlink(&path) {
            Self::remove_path(&path)?;
        }
        Ok(())
    }

    pub fn sync_to_app(index: &SkillsIndex, app: &AppType) -> Result<(), AppError> {
        for skill in index.skills.values() {
            if skill.apps.is_enabled_for(app) {
                Self::sync_to_app_dir(&skill.directory, app, index.sync_method)?;
            }
        }
        Ok(())
    }

    /// Best-effort sync for live-flow triggers (provider switch etc).
    pub fn sync_all_enabled_best_effort() -> Result<(), AppError> {
        let mut index = Self::load_index()?;
        let _ = Self::migrate_ssot_if_pending(&mut index);
        for app in [AppType::Claude, AppType::Codex, AppType::Gemini] {
            if let Err(e) = Self::sync_to_app(&index, &app) {
                log::warn!("同步 Skill 到 {app:?} 失败: {e}");
            }
        }
        Ok(())
    }

    pub fn sync_all_enabled(app: Option<&AppType>) -> Result<(), AppError> {
        let mut index = Self::load_index()?;
        let _ = Self::migrate_ssot_if_pending(&mut index)?;

        match app {
            Some(app) => Self::sync_to_app(&index, app)?,
            None => {
                for app in [AppType::Claude, AppType::Codex, AppType::Gemini] {
                    Self::sync_to_app(&index, &app)?;
                }
            }
        }

        Ok(())
    }

    pub fn list_installed() -> Result<Vec<InstalledSkill>, AppError> {
        let mut index = Self::load_index()?;
        let _ = Self::migrate_ssot_if_pending(&mut index)?;
        let mut skills: Vec<InstalledSkill> = index.skills.values().cloned().collect();
        skills.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        Ok(skills)
    }

    pub fn list_repos() -> Result<Vec<SkillRepo>, AppError> {
        Ok(Self::load_index()?.repos)
    }

    pub fn get_sync_method() -> Result<SyncMethod, AppError> {
        Ok(Self::load_index()?.sync_method)
    }

    pub fn set_sync_method(method: SyncMethod) -> Result<(), AppError> {
        let mut index = Self::load_index()?;
        index.sync_method = method;
        Self::save_index(&index)
    }

    pub fn upsert_repo(repo: SkillRepo) -> Result<(), AppError> {
        let mut index = Self::load_index()?;
        if let Some(pos) = index
            .repos
            .iter()
            .position(|r| r.owner == repo.owner && r.name == repo.name)
        {
            index.repos[pos] = repo;
        } else {
            index.repos.push(repo);
        }
        Self::save_index(&index)?;
        Ok(())
    }

    pub fn remove_repo(owner: &str, name: &str) -> Result<(), AppError> {
        let mut index = Self::load_index()?;
        index
            .repos
            .retain(|r| !(r.owner == owner && r.name == name));
        Self::save_index(&index)?;
        Ok(())
    }

    fn resolve_directory_from_input(index: &SkillsIndex, input: &str) -> Option<String> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return None;
        }

        // Prefer exact directory match.
        if index.skills.contains_key(trimmed) {
            return Some(trimmed.to_string());
        }

        // Case-insensitive directory match.
        let trimmed_lower = trimmed.to_lowercase();
        if let Some((dir, _)) = index
            .skills
            .iter()
            .find(|(dir, _)| dir.to_lowercase() == trimmed_lower)
        {
            return Some(dir.clone());
        }

        // Match by id.
        if let Some((dir, _)) = index
            .skills
            .iter()
            .find(|(_, s)| s.id.eq_ignore_ascii_case(trimmed))
        {
            return Some(dir.clone());
        }

        None
    }

    pub fn toggle_app(directory_or_id: &str, app: &AppType, enabled: bool) -> Result<(), AppError> {
        let mut index = Self::load_index()?;
        let Some(dir) = Self::resolve_directory_from_input(&index, directory_or_id) else {
            return Err(AppError::Message(format!(
                "未找到已安装的 Skill: {directory_or_id}"
            )));
        };

        let Some(record) = index.skills.get_mut(&dir) else {
            return Err(AppError::Message(format!("未找到已安装的 Skill: {dir}")));
        };
        record.apps.set_enabled_for(app, enabled);

        if enabled {
            Self::sync_to_app_dir(&record.directory, app, index.sync_method)?;
        } else {
            Self::remove_from_app(&record.directory, app)?;
        }

        Self::save_index(&index)?;
        Ok(())
    }

    pub fn uninstall(directory_or_id: &str) -> Result<(), AppError> {
        let mut index = Self::load_index()?;
        let Some(dir) = Self::resolve_directory_from_input(&index, directory_or_id) else {
            return Err(AppError::Message(format!(
                "未找到已安装的 Skill: {directory_or_id}"
            )));
        };

        // Remove from app dirs (best effort).
        for app in [AppType::Claude, AppType::Codex, AppType::Gemini] {
            if let Err(e) = Self::remove_from_app(&dir, &app) {
                log::warn!("从 {app:?} 删除 Skill {dir} 失败: {e}");
            }
        }

        // Remove from SSOT.
        let ssot_dir = Self::get_ssot_dir()?;
        let ssot_path = ssot_dir.join(&dir);
        if ssot_path.exists() {
            fs::remove_dir_all(&ssot_path).map_err(|e| AppError::io(&ssot_path, e))?;
        }

        index.skills.remove(&dir);
        Self::save_index(&index)?;
        Ok(())
    }

    pub async fn install(&self, spec: &str, app: &AppType) -> Result<InstalledSkill, AppError> {
        let spec = spec.trim();
        if spec.is_empty() {
            return Err(AppError::InvalidInput("Skill 不能为空".to_string()));
        }

        let mut index = Self::load_index()?;
        let _ = Self::migrate_ssot_if_pending(&mut index)?;

        // Resolve spec to a discoverable skill.
        let discoverable = self.resolve_install_spec(&index, spec).await?;

        // Directory install name is always the last segment.
        let install_name = Path::new(&discoverable.directory)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| discoverable.directory.clone());

        // Conflict check (directory collisions across repos).
        if let Some(existing) = index.skills.get(&install_name) {
            let same_repo = existing.repo_owner.as_deref()
                == Some(discoverable.repo_owner.as_str())
                && existing.repo_name.as_deref() == Some(discoverable.repo_name.as_str());
            if !same_repo
                && (existing.repo_owner.is_some()
                    || existing.repo_name.is_some()
                    || existing.id.starts_with("local:"))
            {
                let existing_repo = format!(
                    "{}/{}",
                    existing.repo_owner.as_deref().unwrap_or("unknown"),
                    existing.repo_name.as_deref().unwrap_or("unknown")
                );
                let new_repo = format!("{}/{}", discoverable.repo_owner, discoverable.repo_name);

                return Err(AppError::Message(format_skill_error(
                    "SKILL_DIRECTORY_CONFLICT",
                    &[
                        ("directory", install_name.as_str()),
                        ("existing_repo", existing_repo.as_str()),
                        ("new_repo", new_repo.as_str()),
                    ],
                    Some("uninstallFirst"),
                )));
            }

            // Already installed: just enable current app and sync.
            let mut updated = existing.clone();
            updated.apps.set_enabled_for(app, true);
            index.skills.insert(install_name.clone(), updated.clone());
            Self::save_index(&index)?;
            Self::sync_to_app_dir(&install_name, app, index.sync_method)?;
            return Ok(updated);
        }

        // Ensure SSOT dir and install files.
        let ssot_dir = Self::get_ssot_dir()?;
        let dest = ssot_dir.join(&install_name);
        if !dest.exists() {
            let repo = SkillRepo {
                owner: discoverable.repo_owner.clone(),
                name: discoverable.repo_name.clone(),
                branch: discoverable.repo_branch.clone(),
                enabled: true,
                skills_path: discoverable.skills_path.clone(),
            };

            let temp_dir = timeout(
                std::time::Duration::from_secs(60),
                self.download_repo(&repo),
            )
            .await
            .map_err(|_| {
                AppError::Message(format_skill_error(
                    "DOWNLOAD_TIMEOUT",
                    &[
                        ("owner", repo.owner.as_str()),
                        ("name", repo.name.as_str()),
                        ("timeout", "60"),
                    ],
                    Some("checkNetwork"),
                ))
            })??;

            let source = if let Some(ref skills_path) = repo.skills_path {
                temp_dir
                    .join(skills_path.trim_matches('/'))
                    .join(&install_name)
            } else {
                temp_dir.join(&install_name)
            };

            if !source.exists() {
                let _ = fs::remove_dir_all(&temp_dir);
                let source_path_string = source.display().to_string();
                return Err(AppError::Message(format_skill_error(
                    "SKILL_DIR_NOT_FOUND",
                    &[("path", source_path_string.as_str())],
                    Some("checkRepoUrl"),
                )));
            }

            Self::copy_dir_recursive(&source, &dest)?;
            let _ = fs::remove_dir_all(&temp_dir);
        }

        let installed = InstalledSkill {
            id: discoverable.key.clone(),
            name: discoverable.name.clone(),
            description: if discoverable.description.trim().is_empty() {
                None
            } else {
                Some(discoverable.description.clone())
            },
            directory: install_name.clone(),
            readme_url: discoverable.readme_url.clone(),
            repo_owner: Some(discoverable.repo_owner.clone()),
            repo_name: Some(discoverable.repo_name.clone()),
            repo_branch: Some(discoverable.repo_branch.clone()),
            apps: SkillApps::only(app),
            installed_at: Utc::now().timestamp(),
        };

        index.skills.insert(install_name.clone(), installed.clone());
        Self::save_index(&index)?;
        Self::sync_to_app_dir(&install_name, app, index.sync_method)?;

        Ok(installed)
    }

    async fn resolve_install_spec(
        &self,
        index: &SkillsIndex,
        spec: &str,
    ) -> Result<DiscoverableSkill, AppError> {
        // If the user provides full key (owner/name:dir), match by key.
        let discoverable = self.discover_available(index.repos.clone()).await?;

        if let Some(found) = discoverable.iter().find(|s| s.key == spec) {
            return Ok(found.clone());
        }

        // Otherwise treat as directory name (may be ambiguous).
        let matches: Vec<DiscoverableSkill> = discoverable
            .into_iter()
            .filter(|s| s.directory.eq_ignore_ascii_case(spec))
            .collect();

        match matches.len() {
            0 => Err(AppError::Message(format!("未找到可安装的 Skill: {spec}"))),
            1 => Ok(matches[0].clone()),
            _ => Err(AppError::Message(format!(
                "Skill 名称不唯一，请使用完整 key（owner/name:directory）: {spec}"
            ))),
        }
    }

    // ---------------------------------------------------------------------
    // Unmanaged scan / import
    // ---------------------------------------------------------------------

    pub fn scan_unmanaged() -> Result<Vec<UnmanagedSkill>, AppError> {
        let index = Self::load_index()?;
        let managed: HashSet<String> = index.skills.keys().cloned().collect();

        let mut unmanaged: HashMap<String, UnmanagedSkill> = HashMap::new();

        for app in [AppType::Claude, AppType::Codex, AppType::Gemini] {
            let app_dir = match Self::get_app_skills_dir(&app) {
                Ok(d) => d,
                Err(_) => continue,
            };
            if !app_dir.exists() {
                continue;
            }

            for entry in fs::read_dir(&app_dir).map_err(|e| AppError::io(&app_dir, e))? {
                let entry = entry.map_err(|e| AppError::io(&app_dir, e))?;
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }

                let dir_name = entry.file_name().to_string_lossy().to_string();
                if dir_name.starts_with('.') {
                    continue;
                }
                if managed.contains(&dir_name) {
                    continue;
                }

                let skill_md = path.join("SKILL.md");
                let (name, description) = if skill_md.exists() {
                    match Self::parse_skill_metadata_static(&skill_md) {
                        Ok(meta) => (
                            meta.name.unwrap_or_else(|| dir_name.clone()),
                            meta.description,
                        ),
                        Err(_) => (dir_name.clone(), None),
                    }
                } else {
                    (dir_name.clone(), None)
                };

                let app_str = match app {
                    AppType::Claude => "claude",
                    AppType::Codex => "codex",
                    AppType::Gemini => "gemini",
                };

                unmanaged
                    .entry(dir_name.clone())
                    .and_modify(|s| s.found_in.push(app_str.to_string()))
                    .or_insert(UnmanagedSkill {
                        directory: dir_name,
                        name,
                        description,
                        found_in: vec![app_str.to_string()],
                    });
            }
        }

        Ok(unmanaged.into_values().collect())
    }

    pub fn import_from_apps(directories: Vec<String>) -> Result<Vec<InstalledSkill>, AppError> {
        let mut index = Self::load_index()?;
        let ssot_dir = Self::get_ssot_dir()?;
        let mut imported = Vec::new();

        for dir_name in directories {
            let mut source_path: Option<PathBuf> = None;
            let mut found_in: Vec<AppType> = Vec::new();

            for app in [AppType::Claude, AppType::Codex, AppType::Gemini] {
                if let Ok(app_dir) = Self::get_app_skills_dir(&app) {
                    let skill_path = app_dir.join(&dir_name);
                    if skill_path.exists() {
                        if source_path.is_none() {
                            source_path = Some(skill_path);
                        }
                        found_in.push(app);
                    }
                }
            }

            let Some(source) = source_path else { continue };

            let dest = ssot_dir.join(&dir_name);
            if !dest.exists() {
                Self::copy_dir_recursive(&source, &dest)?;
            }

            let skill_md = dest.join("SKILL.md");
            let (name, description) = if skill_md.exists() {
                match Self::parse_skill_metadata_static(&skill_md) {
                    Ok(meta) => (
                        meta.name.unwrap_or_else(|| dir_name.clone()),
                        meta.description,
                    ),
                    Err(_) => (dir_name.clone(), None),
                }
            } else {
                (dir_name.clone(), None)
            };

            let mut apps = SkillApps::default();
            for app in &found_in {
                apps.set_enabled_for(app, true);
            }

            let record = index
                .skills
                .entry(dir_name.clone())
                .or_insert_with(|| InstalledSkill {
                    id: format!("local:{dir_name}"),
                    name: name.clone(),
                    description: description.clone(),
                    directory: dir_name.clone(),
                    readme_url: None,
                    repo_owner: None,
                    repo_name: None,
                    repo_branch: None,
                    apps: SkillApps::default(),
                    installed_at: Utc::now().timestamp(),
                });

            record.apps.merge_enabled(&apps);
            if record.description.is_none() {
                record.description = description;
            }
            if record.name.trim().is_empty() {
                record.name = name;
            }

            imported.push(record.clone());
        }

        Self::save_index(&index)?;
        Ok(imported)
    }

    // ---------------------------------------------------------------------
    // Repo discovery / list
    // ---------------------------------------------------------------------

    pub async fn discover_available(
        &self,
        repos: Vec<SkillRepo>,
    ) -> Result<Vec<DiscoverableSkill>, AppError> {
        let enabled_repos: Vec<SkillRepo> = repos.into_iter().filter(|r| r.enabled).collect();
        let tasks = enabled_repos
            .iter()
            .map(|repo| self.fetch_repo_skills(repo));
        let results: Vec<Result<Vec<DiscoverableSkill>, AppError>> = join_all(tasks).await;

        let mut skills = Vec::new();
        for (repo, result) in enabled_repos.into_iter().zip(results.into_iter()) {
            match result {
                Ok(repo_skills) => skills.extend(repo_skills),
                Err(e) => log::warn!("获取仓库 {}/{} 技能失败: {}", repo.owner, repo.name, e),
            }
        }

        Self::deduplicate_discoverable(&mut skills);
        skills.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        Ok(skills)
    }

    pub async fn list_skills(&self) -> Result<Vec<Skill>, AppError> {
        let mut index = Self::load_index()?;
        let _ = Self::migrate_ssot_if_pending(&mut index)?;
        let discoverable = self.discover_available(index.repos.clone()).await?;
        let installed_dirs: HashSet<String> =
            index.skills.keys().map(|s| s.to_lowercase()).collect();

        let mut out: Vec<Skill> = discoverable
            .into_iter()
            .map(|d| {
                let installed = installed_dirs.contains(&d.directory.to_lowercase());
                Skill {
                    key: d.key,
                    name: d.name,
                    description: d.description,
                    directory: d.directory,
                    readme_url: d.readme_url,
                    installed,
                    repo_owner: Some(d.repo_owner),
                    repo_name: Some(d.repo_name),
                    repo_branch: Some(d.repo_branch),
                    skills_path: d.skills_path,
                }
            })
            .collect();

        // Add local SSOT-only skills not in repos.
        Self::merge_local_ssot_skills(&index, &mut out)?;

        // De-dup + sort.
        Self::deduplicate_skills(&mut out);
        out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        Ok(out)
    }

    fn merge_local_ssot_skills(
        index: &SkillsIndex,
        skills: &mut Vec<Skill>,
    ) -> Result<(), AppError> {
        let ssot = Self::get_ssot_dir()?;
        if !ssot.exists() {
            return Ok(());
        }

        for entry in fs::read_dir(&ssot).map_err(|e| AppError::io(&ssot, e))? {
            let entry = entry.map_err(|e| AppError::io(&ssot, e))?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let directory = entry.file_name().to_string_lossy().to_string();
            if directory.starts_with('.') {
                continue;
            }

            let mut found = false;
            for skill in skills.iter_mut() {
                if skill.directory.eq_ignore_ascii_case(&directory) {
                    skill.installed = true;
                    found = true;
                    break;
                }
            }
            if found {
                continue;
            }

            let record = index.skills.get(&directory);
            let skill_md = path.join("SKILL.md");
            let (name, description) = if let Some(r) = record {
                (r.name.clone(), r.description.clone().unwrap_or_default())
            } else if skill_md.exists() {
                match Self::parse_skill_metadata_static(&skill_md) {
                    Ok(meta) => (
                        meta.name.unwrap_or_else(|| directory.clone()),
                        meta.description.unwrap_or_default(),
                    ),
                    Err(_) => (directory.clone(), String::new()),
                }
            } else {
                (directory.clone(), String::new())
            };

            skills.push(Skill {
                key: format!("local:{directory}"),
                name,
                description,
                directory,
                readme_url: None,
                installed: true,
                repo_owner: None,
                repo_name: None,
                repo_branch: None,
                skills_path: None,
            });
        }

        Ok(())
    }

    async fn fetch_repo_skills(
        &self,
        repo: &SkillRepo,
    ) -> Result<Vec<DiscoverableSkill>, AppError> {
        let temp_dir = timeout(std::time::Duration::from_secs(60), self.download_repo(repo))
            .await
            .map_err(|_| {
                AppError::Message(format_skill_error(
                    "DOWNLOAD_TIMEOUT",
                    &[
                        ("owner", repo.owner.as_str()),
                        ("name", repo.name.as_str()),
                        ("timeout", "60"),
                    ],
                    Some("checkNetwork"),
                ))
            })??;

        let scan_dir = if let Some(ref skills_path) = repo.skills_path {
            let subdir = temp_dir.join(skills_path.trim_matches('/'));
            if !subdir.exists() {
                let _ = fs::remove_dir_all(&temp_dir);
                return Ok(Vec::new());
            }
            subdir
        } else {
            temp_dir.clone()
        };

        let mut skills = Vec::new();
        for entry in fs::read_dir(&scan_dir).map_err(|e| AppError::io(&scan_dir, e))? {
            let entry = entry.map_err(|e| AppError::io(&scan_dir, e))?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let skill_md = path.join("SKILL.md");
            if !skill_md.exists() {
                continue;
            }
            let meta = match Self::parse_skill_metadata_static(&skill_md) {
                Ok(m) => m,
                Err(_) => SkillMetadata {
                    name: None,
                    description: None,
                },
            };

            let directory = path.file_name().unwrap().to_string_lossy().to_string();
            let readme_path = if let Some(ref skills_path) = repo.skills_path {
                format!("{}/{}", skills_path.trim_matches('/'), directory)
            } else {
                directory.clone()
            };

            skills.push(DiscoverableSkill {
                key: format!("{}/{}:{}", repo.owner, repo.name, directory),
                name: meta.name.unwrap_or_else(|| directory.clone()),
                description: meta.description.unwrap_or_default(),
                directory,
                readme_url: Some(format!(
                    "https://github.com/{}/{}/tree/{}/{}",
                    repo.owner, repo.name, repo.branch, readme_path
                )),
                repo_owner: repo.owner.clone(),
                repo_name: repo.name.clone(),
                repo_branch: repo.branch.clone(),
                skills_path: repo.skills_path.clone(),
            });
        }

        let _ = fs::remove_dir_all(&temp_dir);
        Ok(skills)
    }

    fn deduplicate_discoverable(skills: &mut Vec<DiscoverableSkill>) {
        let mut seen: HashSet<String> = HashSet::new();
        skills.retain(|s| {
            let key = format!("{}|{}", s.repo_owner.to_lowercase(), s.key.to_lowercase());
            if seen.contains(&key) {
                false
            } else {
                seen.insert(key);
                true
            }
        });
    }

    fn deduplicate_skills(skills: &mut Vec<Skill>) {
        let mut seen = HashSet::new();
        skills.retain(|skill| {
            let key = skill.directory.to_lowercase();
            if seen.contains(&key) {
                false
            } else {
                seen.insert(key);
                true
            }
        });
    }

    fn parse_skill_metadata_static(path: &Path) -> Result<SkillMetadata, AppError> {
        let content = fs::read_to_string(path).map_err(|e| AppError::io(path, e))?;
        let content = content.trim_start_matches('\u{feff}');
        let parts: Vec<&str> = content.splitn(3, "---").collect();
        if parts.len() < 3 {
            return Ok(SkillMetadata {
                name: None,
                description: None,
            });
        }
        let front_matter = parts[1].trim();
        let meta: SkillMetadata = serde_yaml::from_str(front_matter).unwrap_or(SkillMetadata {
            name: None,
            description: None,
        });
        Ok(meta)
    }

    async fn download_repo(&self, repo: &SkillRepo) -> Result<PathBuf, AppError> {
        let temp_dir = tempfile::tempdir().map_err(|e| {
            AppError::localized(
                "skills.tempdir_failed",
                format!("创建临时目录失败: {e}"),
                format!("Failed to create temp dir: {e}"),
            )
        })?;
        let temp_path = temp_dir.path().to_path_buf();
        let _ = temp_dir.keep();

        let branches = if repo.branch.trim().is_empty() {
            vec!["main", "master"]
        } else {
            vec![repo.branch.as_str(), "main", "master"]
        };

        let mut last_error: Option<AppError> = None;
        for branch in branches {
            let url = format!(
                "https://github.com/{}/{}/archive/refs/heads/{}.zip",
                repo.owner, repo.name, branch
            );

            match self.download_and_extract(&url, &temp_path).await {
                Ok(()) => return Ok(temp_path),
                Err(e) => {
                    last_error = Some(e);
                    continue;
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            AppError::Message(format_skill_error(
                "DOWNLOAD_FAILED",
                &[],
                Some("checkNetwork"),
            ))
        }))
    }

    async fn download_and_extract(&self, url: &str, dest: &Path) -> Result<(), AppError> {
        let response = self.http_client.get(url).send().await.map_err(|e| {
            AppError::localized(
                "skills.download_failed",
                format!("下载失败: {e}"),
                format!("Download failed: {e}"),
            )
        })?;

        if !response.status().is_success() {
            let status = response.status().as_u16().to_string();
            return Err(AppError::Message(format_skill_error(
                "DOWNLOAD_FAILED",
                &[("status", status.as_str())],
                match status.as_str() {
                    "403" => Some("http403"),
                    "404" => Some("http404"),
                    "429" => Some("http429"),
                    _ => Some("checkNetwork"),
                },
            )));
        }

        let bytes = response.bytes().await.map_err(|e| {
            AppError::localized(
                "skills.download_failed",
                format!("读取下载内容失败: {e}"),
                format!("Failed to read download bytes: {e}"),
            )
        })?;

        let cursor = std::io::Cursor::new(bytes);
        let mut archive = zip::ZipArchive::new(cursor).map_err(|e| {
            AppError::localized(
                "skills.zip_invalid",
                format!("ZIP 文件损坏: {e}"),
                format!("Invalid ZIP: {e}"),
            )
        })?;

        let root_name = if !archive.is_empty() {
            let first_file = archive.by_index(0).map_err(|e| {
                AppError::localized(
                    "skills.zip_invalid",
                    format!("读取 ZIP 失败: {e}"),
                    format!("Failed to read ZIP: {e}"),
                )
            })?;
            let name = first_file.name();
            name.split('/').next().unwrap_or("").to_string()
        } else {
            return Err(AppError::Message(format_skill_error(
                "EMPTY_ARCHIVE",
                &[],
                Some("checkRepoUrl"),
            )));
        };

        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .map_err(|e| AppError::Message(e.to_string()))?;
            let file_path = file.name();

            let relative_path =
                if let Some(stripped) = file_path.strip_prefix(&format!("{root_name}/")) {
                    stripped
                } else {
                    continue;
                };
            if relative_path.is_empty() {
                continue;
            }

            let outpath = dest.join(relative_path);
            if file.is_dir() {
                fs::create_dir_all(&outpath).map_err(|e| AppError::io(&outpath, e))?;
            } else {
                if let Some(parent) = outpath.parent() {
                    fs::create_dir_all(parent).map_err(|e| AppError::io(parent, e))?;
                }
                let mut outfile =
                    fs::File::create(&outpath).map_err(|e| AppError::io(&outpath, e))?;
                std::io::copy(&mut file, &mut outfile).map_err(|e| AppError::IoContext {
                    context: format!("写入文件失败: {}", outpath.display()),
                    source: e,
                })?;
            }
        }

        Ok(())
    }

    fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<(), AppError> {
        fs::create_dir_all(dest).map_err(|e| AppError::io(dest, e))?;
        for entry in fs::read_dir(src).map_err(|e| AppError::io(src, e))? {
            let entry = entry.map_err(|e| AppError::io(src, e))?;
            let path = entry.path();
            let dest_path = dest.join(entry.file_name());

            if path.is_dir() {
                Self::copy_dir_recursive(&path, &dest_path)?;
            } else {
                fs::copy(&path, &dest_path).map_err(|e| AppError::io(&dest_path, e))?;
            }
        }
        Ok(())
    }
}
