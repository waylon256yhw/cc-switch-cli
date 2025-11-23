use crate::settings::{get_settings, update_settings};
use std::sync::OnceLock;
use std::sync::RwLock;

/// Supported languages
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    English,
    Chinese,
}

impl Language {
    pub fn code(&self) -> &'static str {
        match self {
            Language::English => "en",
            Language::Chinese => "zh",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Language::English => "English",
            Language::Chinese => "中文",
        }
    }

    pub fn from_code(code: &str) -> Self {
        match code.to_lowercase().as_str() {
            "zh" | "zh-cn" | "zh-tw" | "chinese" => Language::Chinese,
            _ => Language::English,
        }
    }
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Global language state
fn language_store() -> &'static RwLock<Language> {
    static STORE: OnceLock<RwLock<Language>> = OnceLock::new();
    STORE.get_or_init(|| {
        let settings = get_settings();
        let lang = settings
            .language
            .as_deref()
            .map(Language::from_code)
            .unwrap_or(Language::English);
        RwLock::new(lang)
    })
}

/// Get current language
pub fn current_language() -> Language {
    *language_store().read().expect("Failed to read language")
}

/// Set current language and persist
pub fn set_language(lang: Language) -> Result<(), crate::error::AppError> {
    // Update runtime state
    {
        let mut guard = language_store().write().expect("Failed to write language");
        *guard = lang;
    }

    // Persist to settings
    let mut settings = get_settings();
    settings.language = Some(lang.code().to_string());
    update_settings(settings)
}

/// Check if current language is Chinese
pub fn is_chinese() -> bool {
    current_language() == Language::Chinese
}

// ============================================================================
// Localized Text Macros and Functions
// ============================================================================

/// Get localized text based on current language
#[macro_export]
macro_rules! t {
    ($en:expr, $zh:expr) => {
        if $crate::cli::i18n::is_chinese() {
            $zh
        } else {
            $en
        }
    };
}

// Re-export for convenience
pub use t;

// ============================================================================
// Common UI Texts
// ============================================================================

pub mod texts {
    use super::is_chinese;

    // Welcome & Headers
    pub fn welcome_title() -> &'static str {
        if is_chinese() {
            "    🎯 CC-Switch 交互模式"
        } else {
            "    🎯 CC-Switch Interactive Mode"
        }
    }

    pub fn application() -> &'static str {
        if is_chinese() {
            "应用程序"
        } else {
            "Application"
        }
    }

    pub fn goodbye() -> &'static str {
        if is_chinese() {
            "👋 再见！"
        } else {
            "👋 Goodbye!"
        }
    }

    // Main Menu
    pub fn main_menu_prompt(app: &str) -> String {
        if is_chinese() {
            format!("请选择操作 (当前: {})", app)
        } else {
            format!("What would you like to do? (Current: {})", app)
        }
    }

    pub fn menu_manage_providers() -> &'static str {
        if is_chinese() {
            "🔌 管理供应商"
        } else {
            "🔌 Manage Providers"
        }
    }

    pub fn menu_manage_mcp() -> &'static str {
        if is_chinese() {
            "🛠️  管理 MCP 服务器"
        } else {
            "🛠️  Manage MCP Servers"
        }
    }

    pub fn menu_manage_prompts() -> &'static str {
        if is_chinese() {
            "💬 管理提示词"
        } else {
            "💬 Manage Prompts"
        }
    }

    pub fn menu_view_config() -> &'static str {
        if is_chinese() {
            "👁️  查看当前配置"
        } else {
            "👁️  View Current Configuration"
        }
    }

    pub fn menu_switch_app() -> &'static str {
        if is_chinese() {
            "🔄 切换应用"
        } else {
            "🔄 Switch Application"
        }
    }

    pub fn menu_settings() -> &'static str {
        if is_chinese() {
            "⚙️  设置"
        } else {
            "⚙️  Settings"
        }
    }

    pub fn menu_exit() -> &'static str {
        if is_chinese() {
            "🚪 退出"
        } else {
            "🚪 Exit"
        }
    }

    // Provider Management
    pub fn provider_management() -> &'static str {
        if is_chinese() {
            "🔌 供应商管理"
        } else {
            "🔌 Provider Management"
        }
    }

    pub fn no_providers() -> &'static str {
        if is_chinese() {
            "未找到供应商。"
        } else {
            "No providers found."
        }
    }

    pub fn view_current_provider() -> &'static str {
        if is_chinese() {
            "📋 查看当前供应商详情"
        } else {
            "📋 View Current Provider Details"
        }
    }

    pub fn switch_provider() -> &'static str {
        if is_chinese() {
            "🔄 切换供应商"
        } else {
            "🔄 Switch Provider"
        }
    }

    pub fn delete_provider() -> &'static str {
        if is_chinese() {
            "🗑️  删除供应商"
        } else {
            "🗑️  Delete Provider"
        }
    }

    pub fn back_to_main() -> &'static str {
        if is_chinese() {
            "⬅️  返回主菜单"
        } else {
            "⬅️  Back to Main Menu"
        }
    }

    pub fn choose_action() -> &'static str {
        if is_chinese() {
            "选择操作："
        } else {
            "Choose an action:"
        }
    }

    pub fn current_provider_details() -> &'static str {
        if is_chinese() {
            "当前供应商详情"
        } else {
            "Current Provider Details"
        }
    }

    pub fn only_one_provider() -> &'static str {
        if is_chinese() {
            "只有一个供应商，无法切换。"
        } else {
            "Only one provider available. Cannot switch."
        }
    }

    pub fn no_other_providers() -> &'static str {
        if is_chinese() {
            "没有其他供应商可切换。"
        } else {
            "No other providers to switch to."
        }
    }

    pub fn select_provider_to_switch() -> &'static str {
        if is_chinese() {
            "选择要切换到的供应商："
        } else {
            "Select provider to switch to:"
        }
    }

    pub fn switched_to_provider(id: &str) -> String {
        if is_chinese() {
            format!("✓ 已切换到供应商 '{}'", id)
        } else {
            format!("✓ Switched to provider '{}'", id)
        }
    }

    pub fn restart_note() -> &'static str {
        if is_chinese() {
            "注意：请重启 CLI 客户端以应用更改。"
        } else {
            "Note: Restart your CLI client to apply the changes."
        }
    }

    pub fn no_deletable_providers() -> &'static str {
        if is_chinese() {
            "没有可删除的供应商（无法删除当前供应商）。"
        } else {
            "No providers available for deletion (cannot delete current provider)."
        }
    }

    pub fn select_provider_to_delete() -> &'static str {
        if is_chinese() {
            "选择要删除的供应商："
        } else {
            "Select provider to delete:"
        }
    }

    pub fn confirm_delete(id: &str) -> String {
        if is_chinese() {
            format!("确定要删除供应商 '{}' 吗？", id)
        } else {
            format!("Are you sure you want to delete provider '{}'?", id)
        }
    }

    pub fn cancelled() -> &'static str {
        if is_chinese() {
            "已取消。"
        } else {
            "Cancelled."
        }
    }

    pub fn deleted_provider(id: &str) -> String {
        if is_chinese() {
            format!("✓ 已删除供应商 '{}'", id)
        } else {
            format!("✓ Deleted provider '{}'", id)
        }
    }

    // MCP Management
    pub fn mcp_management() -> &'static str {
        if is_chinese() {
            "🛠️  MCP 服务器管理"
        } else {
            "🛠️  MCP Server Management"
        }
    }

    pub fn no_mcp_servers() -> &'static str {
        if is_chinese() {
            "未找到 MCP 服务器。"
        } else {
            "No MCP servers found."
        }
    }

    pub fn sync_all_servers() -> &'static str {
        if is_chinese() {
            "🔄 同步所有服务器"
        } else {
            "🔄 Sync All Servers"
        }
    }

    pub fn synced_successfully() -> &'static str {
        if is_chinese() {
            "✓ 所有 MCP 服务器同步成功"
        } else {
            "✓ All MCP servers synced successfully"
        }
    }

    // Prompts Management
    pub fn prompts_management() -> &'static str {
        if is_chinese() {
            "💬 提示词管理"
        } else {
            "💬 Prompt Management"
        }
    }

    pub fn no_prompts() -> &'static str {
        if is_chinese() {
            "未找到提示词预设。"
        } else {
            "No prompt presets found."
        }
    }

    pub fn switch_active_prompt() -> &'static str {
        if is_chinese() {
            "🔄 切换活动提示词"
        } else {
            "🔄 Switch Active Prompt"
        }
    }

    pub fn no_prompts_available() -> &'static str {
        if is_chinese() {
            "没有可用的提示词。"
        } else {
            "No prompts available."
        }
    }

    pub fn select_prompt_to_activate() -> &'static str {
        if is_chinese() {
            "选择要激活的提示词："
        } else {
            "Select prompt to activate:"
        }
    }

    pub fn activated_prompt(id: &str) -> String {
        if is_chinese() {
            format!("✓ 已激活提示词 '{}'", id)
        } else {
            format!("✓ Activated prompt '{}'", id)
        }
    }

    pub fn prompt_synced_note() -> &'static str {
        if is_chinese() {
            "注意：提示词已同步到实时配置文件。"
        } else {
            "Note: The prompt has been synced to the live configuration file."
        }
    }

    // Configuration View
    pub fn current_configuration() -> &'static str {
        if is_chinese() {
            "👁️  当前配置"
        } else {
            "👁️  Current Configuration"
        }
    }

    pub fn provider_label() -> &'static str {
        if is_chinese() {
            "供应商："
        } else {
            "Provider:"
        }
    }

    pub fn mcp_servers_label() -> &'static str {
        if is_chinese() {
            "MCP 服务器："
        } else {
            "MCP Servers:"
        }
    }

    pub fn prompts_label() -> &'static str {
        if is_chinese() {
            "提示词："
        } else {
            "Prompts:"
        }
    }

    pub fn total() -> &'static str {
        if is_chinese() {
            "总计"
        } else {
            "Total"
        }
    }

    pub fn enabled() -> &'static str {
        if is_chinese() {
            "启用"
        } else {
            "Enabled"
        }
    }

    pub fn active() -> &'static str {
        if is_chinese() {
            "活动"
        } else {
            "Active"
        }
    }

    pub fn none() -> &'static str {
        if is_chinese() {
            "无"
        } else {
            "None"
        }
    }

    // Settings
    pub fn settings_title() -> &'static str {
        if is_chinese() {
            "⚙️  设置"
        } else {
            "⚙️  Settings"
        }
    }

    pub fn change_language() -> &'static str {
        if is_chinese() {
            "🌐 切换语言"
        } else {
            "🌐 Change Language"
        }
    }

    pub fn current_language_label() -> &'static str {
        if is_chinese() {
            "当前语言"
        } else {
            "Current Language"
        }
    }

    pub fn select_language() -> &'static str {
        if is_chinese() {
            "选择语言："
        } else {
            "Select language:"
        }
    }

    pub fn language_changed() -> &'static str {
        if is_chinese() {
            "✓ 语言已更改"
        } else {
            "✓ Language changed"
        }
    }

    // App Selection
    pub fn select_application() -> &'static str {
        if is_chinese() {
            "选择应用程序："
        } else {
            "Select application:"
        }
    }

    pub fn switched_to_app(app: &str) -> String {
        if is_chinese() {
            format!("✓ 已切换到 {}", app)
        } else {
            format!("✓ Switched to {}", app)
        }
    }

    // Common
    pub fn press_enter() -> &'static str {
        if is_chinese() {
            "按 Enter 继续..."
        } else {
            "Press Enter to continue..."
        }
    }

    pub fn error_prefix() -> &'static str {
        if is_chinese() {
            "错误"
        } else {
            "Error"
        }
    }

    // Table Headers
    pub fn header_name() -> &'static str {
        if is_chinese() {
            "名称"
        } else {
            "Name"
        }
    }

    pub fn header_category() -> &'static str {
        if is_chinese() {
            "类别"
        } else {
            "Category"
        }
    }

    pub fn header_description() -> &'static str {
        if is_chinese() {
            "描述"
        } else {
            "Description"
        }
    }
}
