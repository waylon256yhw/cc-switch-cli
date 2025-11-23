# Changelog

All notable changes to CC Switch will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [4.0.0-cli] - 2025-11-23 (CLI Edition Fork)

### Overview

Complete migration from Tauri GUI application to standalone CLI tool. This is a **CLI-focused fork** of the original CC-Switch project.

### Breaking Changes

- Removed Tauri GUI (desktop window, React frontend, WebView runtime)
- Removed system tray menu, auto-updater, deep link protocol (`ccswitch://`)
- Users must transition to command-line or interactive mode

### New Features

- **Dual Interface Modes**: Command-line mode + Interactive TUI mode
- **Default Interactive Mode**: Run `cc-switch` without arguments to enter interactive mode
- **Provider Management**: list, add, edit, delete, switch, duplicate, speedtest
- **MCP Server Management**: list, add, edit, delete, enable/disable, sync, import/export
- **Prompts Management**: list, activate, show, create, edit, delete
- **Configuration Management**: show, export, import, backup, restore, validate, reset
- **Utilities**: shell completions (bash/zsh/fish/powershell), env check, app switch

### Removed

- Complete React 18 + TypeScript frontend (~50,000 lines)
- Tauri 2.8 desktop runtime and all GUI-specific features
- 200+ npm dependencies

### Preserved

- 100% core business logic (ProviderService, McpService, PromptService, ConfigService)
- Configuration format and file locations
- Multi-app support (Claude/Codex/Gemini)

### Technical

- **New Stack**: clap v4.5, inquire v0.7, comfy-table v7.1, colored v2.1
- **Binary Size**: ~5-8 MB (vs ~15-20 MB GUI)
- **Startup Time**: <50ms (vs 500-1000ms GUI)
- **Dependencies**: ~20 Rust crates (vs 200+ npm + 50+ Rust)

### Credits

- Original Project: [CC-Switch](https://github.com/farion1231/cc-switch) by Jason Young
- CLI Fork Maintainer: saladday

---

## [3.7.1] - 2025-11-22

### Fixed

- Skills third-party repository installation (#268)
- Gemini configuration persistence
- Dialog overlay click protection

### Added

- Gemini configuration directory support (#255)
- ArchLinux installation support (#259)

### Improved

- Skills error messages i18n (28+ messages)
- Download timeout extended to 60s

---

## [3.7.0] - 2025-11-19

### Major Features

- **Gemini CLI Integration** - Third major application support
- **MCP v3.7.0 Unified Architecture** - Single interface for Claude/Codex/Gemini
- **Claude Skills Management System** - GitHub repository integration
- **Prompts Management** - Multi-preset system prompts
- **Deep Link Protocol** - `ccswitch://` URL scheme
- **Environment Variable Conflict Detection**

### New Features

- Provider presets: DouBaoSeed, Kimi For Coding, BaiLing
- Common config migration to `config.json`
- macOS native design color scheme

### Statistics

- 85 commits, 152 files changed
- Skills: 2,034 lines, Prompts: 1,302 lines, Gemini: ~1,000 lines

---

## [3.6.0] - 2025-11-07

### New Features

- Provider Duplicate
- Edit Mode Toggle
- Custom Endpoint Management
- Usage Query Enhancements
- Auto-sync on Directory Change
- New Provider Presets: DMXAPI, Azure Codex, AnyRouter, AiHubMix, MiniMax

### Technical Improvements

- Backend: 5-phase refactoring (error handling, commands, services, concurrency)
- Frontend: 4-stage refactoring (tests, hooks, components, cleanup)
- Hooks unit tests 100% coverage

---

## [3.5.0] - 2025-01-15

### Breaking Changes

- Tauri commands only accept `app` parameter (values: `claude`/`codex`)
- Frontend type unified to `AppId`

### New Features

- MCP (Model Context Protocol) Management
- Configuration Import/Export
- Endpoint Speed Testing

---

## [3.4.0] - 2025-10-01

### Features

- Internationalization (i18next) with Chinese default
- Claude plugin sync
- Extended provider presets
- Portable mode and single instance enforcement

---

## [3.3.0] - 2025-09-22

### Features

- VS Code integration for provider sync _(Removed in 3.4.x)_
- Codex provider wizard enhancements
- Shared common config snippets

---

## [3.2.0] - 2025-09-13

### New Features

- System tray provider switching
- Built-in update flow via Tauri Updater
- Single source of truth for provider configs
- One-time migration from v1 to v2

---

## [3.1.0] - 2025-09-01

### New Features

- **Codex application support** - Manage auth.json and config.toml
- Multi-app config v2 structure
- Automatic v1→v2 migration

---

## [3.0.0] - 2025-08-27

### Major Changes

- **Complete migration from Electron to Tauri 2.0**
- 90% reduction in bundle size (~150MB → ~15MB)
- Significantly improved startup performance

---

## [2.0.0] - Previous Electron Release

- Multi-provider configuration management
- Quick provider switching
- Import/export configurations

---

## [1.0.0] - Initial Release

- Basic provider management
- Claude Code integration
