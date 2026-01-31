# 2026-01-30 Ratatui 全量重写 Interactive TUI（实现追踪）

## 背景

当前 `cc-switch` 的交互模式主要由：

- `println!` + `console::Term::read_key()`（主菜单导航/过滤）
- `inquire`（Select/Text/Confirm/MultiSelect）

组成。整体可用但不够“应用化”，布局与视觉风格不统一，扩展复杂界面困难。

本计划引入 `ratatui` 作为新的 TUI 渲染层，将交互模式重构为统一的侧边导航 UI，同时保留少量 legacy 交互用于复杂输入（例如外部编辑器编辑 JSON/TOML）。

## 目标与范围

### 目标

- 交互模式（`cc-switch ui` / 无参数进入的 interactive）默认进入 ratatui TUI。
- UI 更美观：统一边框/布局/主题色，具备侧边导航、内容区、底部快捷键提示、帮助弹窗。
- 行为稳定：退出后终端状态正确恢复（raw/alternate screen/cursor 等）。
- 可回退：通过环境变量强制进入 legacy 交互模式。

### 范围（本期）

- 覆盖模块：Main / Providers / MCP / Prompts / Config / Settings / Exit Confirm
- 允许保留：少量复杂交互继续用 legacy（临时退出 ratatui 后执行，完成后返回）

### 不在本期

- 重新设计业务逻辑（services/store/config 格式）
- 新增 CLI 命令或改变命令输出格式（非 interactive 模式）
- 完整维护 legacy interactive / 非 TUI 分支：本期主要聚焦 ratatui TUI；CLI/legacy 的个别 UX/一致性问题暂不继续扩展（例如 validate command 的复杂输入解析、旧交互提示文案等）。

## 关键决策

- 布局：侧边导航（左）+ 内容区（右）+ Footer（底部）。
- 主题：按 `AppType` 上色（Codex/Claude/Gemini）。
- Unicode：默认允许 box drawing/少量图标；如 `NO_COLOR` 则降级。
- 回退策略：
  - `CC_SWITCH_LEGACY_TUI=1` 强制 legacy
  - stdin/stdout 非 TTY 自动 legacy
- 依赖统一：升级 `inquire` 并显式依赖 `crossterm`，避免多版本导致的终端状态问题。

## 里程碑（状态）

| Milestone | 内容 | 状态 |
|---|---|---|
| M0 | worktree + 基线 `cargo test` | DONE |
| M1 | 依赖：ratatui + crossterm + inquire 升级 | DONE |
| M2 | TUI 框架：terminal/event/theme/app/route/ui/data | DONE |
| M3 | Main/Nav/Footer + Help + Filter 模式 | DONE |
| M4 | Providers：列表/详情/切换/删除/测速 | DONE |
| M5 | MCP：列表/切换启用/导入/校验入口 | DONE |
| M6 | Settings：语言切换 | DONE |
| M7 | 配置摘要：合并到 Home（移除 ViewConfig 菜单项） | DONE |
| M8 | Prompts：列表/启用/编辑入口 | DONE |
| M9 | Config：动作列表 + 必要时 legacy | DONE |
| M10 | 测试（状态机）+ fmt + clippy + test | PARTIAL（fmt/test DONE；clippy -D warnings 因仓库既有 dead_code 等告警失败） |

## 屏幕功能对照

> 快捷键分三类：  
> 1) **NAV（底部 Footer）**：`←→` 切换菜单/内容焦点，`↑↓` 移动  
> 2) **ACT（底部 Footer，全局通用）**：`[`/`]` 切换 App，`/` 过滤，`Esc` 返回/关闭，`?` 帮助  
> 3) **Page Keys（页面/弹窗顶部 Key Bar）**：只显示当前页面/弹窗可用的动作键

| Route | 功能 | Page Keys（Key Bar） |
|---|---|---|
| Main | 概览 + 入口 | - |
| Providers | Provider 列表 | `Enter` 详情，`s` 切换，`a` 新增，`e` 编辑，`d` 删除，`t` 测速 |
| ProviderDetail | Provider 详情 | `s` 切换，`e` 编辑，`t` 测速 |
| MCP | MCP server 列表 | `x` 启用/禁用，`a` 新增，`e` 编辑，`i` 导入，`v` 校验命令，`d` 删除 |
| Prompts | Prompt 列表 | `Enter` 查看，`a` 激活，`x` 取消激活，`e` 编辑，`d` 删除 |
| Config | 配置动作列表 | `Enter` 打开/执行（按选中项），`e` 编辑片段（CommonSnippet） |
| Settings | 语言切换 | `Enter` 应用 |
| Confirm | 确认弹窗 | `Enter` 确认，`Esc` 取消 |
| TextInput | 输入弹窗 | `Enter` 提交，`Esc` 取消 |
| BackupPicker | 选择备份 | `Enter` 恢复，`Esc` 取消 |
| TextView | 文本查看 | `↑↓` 滚动，`Esc` 关闭 |
| Editor | 内嵌编辑页 | `Enter` 进入编辑，`↑↓` 滚动 / `↑↓←→` 移动光标，`Ctrl+S` 保存，`Esc` 关闭 |

## 风险与缓解

- **终端状态残留（raw/alternate screen）**：RAII 终端封装 + `Drop` 恢复；legacy 调用前后明确 restore/re-init。
- **crossterm 多版本冲突**：升级 `inquire` 并显式依赖 `crossterm` 统一版本；用 `cargo tree` 验证。
- **Windows 兼容**：避免依赖 `console::Term`；使用 `crossterm` 官方 API；NO_COLOR 降级。

## 更新日志

- 2026-01-30：创建 `feat/ratatui-tui` worktree；基线 `cd src-tauri && cargo test` 通过。
- 2026-01-30：升级依赖：`ratatui` + `crossterm`，并将 `inquire` 升级到 v0.9.x；`cargo tree` 验证无 `crossterm v0.25`。
- 2026-01-30：新增 ratatui TUI 框架模块 `src-tauri/src/cli/tui/`；interactive 增加 `legacy` 兼容入口与自动/手动回退机制。
- 2026-01-30：完成 Providers/MCP/Prompts/Config/Settings 屏幕与核心操作；新增 TUI i18n 文案；`cargo test` 通过；`cargo clippy -- -D warnings` 因仓库既有 dead_code 等问题失败（未在本期处理）。
- 2026-01-30：补齐关键稳定性：speedtest 改为单 worker 线程 + 单 Tokio runtime（避免重复触发创建线程/运行时）；raw mode 下支持 `Ctrl-C` 快速退出；`with_terminal_restored` 增加 unwind 安全的 re-activate guard；legacy 在非 TTY 时返回清晰错误提示（避免卡住）。
- 2026-01-30：交互与 UI 优化：`[`/`]` 切换 App；`←→` 切换菜单/内容焦点；移除 `Tab` 切换焦点（保留 `←→`）；Providers 列表 `Enter`=详情、`s`=切换；顶部使用 Tabs 显式当前 App。
- 2026-01-30：UI 风格调整：顶部 Header 外包围矩形；Providers 内容区保留外边框、仅选中行使用主题色背景色块（标题/表头不使用主题色）；Footer 快捷键提示按「导航键/功能键」分组并用灰度色块承载；Home 页增加上方信息区 + 下方 ASCII Logo。
- 2026-01-31：Home 页 Logo 调整为「下半区垂直居中」呈现（信息在上，Logo 在下居中）。
- 2026-01-31：输入与编辑统一：Filter Bar 与小输入弹窗改为“外框 + 内输入框”风格；新增内嵌编辑页（Enter 进入编辑、Esc 退出编辑、Ctrl+S 保存），用于 Prompts 编辑与 CommonSnippet JSON 编辑（替代外部编辑器/未实现的 prompts edit）。
- 2026-01-31：补齐 Provider 编辑：Providers 列表与详情页 `e` 进入内嵌 JSON 编辑页（Ctrl+S 保存），不再调用外部编辑器/legacy。
- 2026-01-31：Provider 内嵌 JSON 编辑页默认隐藏内部字段（如 `createdAt`/`updatedAt`/`inFailoverQueue`），避免把内部字段暴露在编辑界面。
- 2026-01-31：补齐 Provider 新增与 MCP 增删改：Providers `a` 进入内嵌 JSON 新增页；MCP 列表增加 `a` 新增、`e` 编辑（Ctrl+S 保存）。
- 2026-01-31：补齐 CLI stubs：`cc-switch prompts create/edit` 与 `cc-switch mcp add/edit` 改为使用外部编辑器完成编辑并写回统一配置（不再提示“手动改 config.json”）。
- 2026-01-31：快捷键提示重构：Footer 只显示 NAV + 全局 ACT；页面动作键（含弹窗/编辑页）统一放在页面顶部 Key Bar（避免 Footer 拥挤/误导）。
- 2026-01-31：补齐校验与错误提示：Provider 编辑保存前强制 `name` 非空；Provider/MCP 内嵌编辑 JSON 解析失败时使用统一 “Invalid JSON / JSON 无效” toast（不再复用 CommonSnippet 的错误文案）。
- 2026-01-31：进一步对齐快捷键：Config 列表页选中 CommonSnippet 时支持 `e` 直接进入编辑；CommonSnippet 预览弹窗支持 `a/c/e`（Key Bar 展示）；MCP 校验命令只取第一个 token 作为命令名进行 PATH 校验。
