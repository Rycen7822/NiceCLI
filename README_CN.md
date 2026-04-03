<p align="center">
  <img src="./nicecli-logo.png" alt="NiceCLI" width="160" />
</p>

<p align="center">
  <img src="https://img.shields.io/badge/platform-Windows-0078D4" alt="Windows" />
  <img src="https://img.shields.io/badge/runtime-Tauri%202-24C8DB" alt="Tauri 2" />
  <img src="https://img.shields.io/badge/backend-CLIProxyAPI-00ADD8" alt="CLIProxyAPI" />
  <img src="https://img.shields.io/badge/mode-Local%20Only-111111" alt="Local only" />
  <img src="https://img.shields.io/badge/release-portable%20exe-3A7AFE" alt="Portable exe" />
</p>

<p align="center">
  <a href="./README.md">English</a> | <a href="./README_CN.md">中文</a>
</p>

<h3 align="center">
  一个面向 CLIProxyAPI 的本地优先桌面控制台。
</h3>

<p align="center">
  NiceCLI 是一个以 Windows 为主的 EasyCLI 分支与产品化重构版本，重点放在更干净的 local-only 工作流、内嵌的 desktop-lite CLIProxyAPI 后端，以及更完善的面向开发者的控制面板体验。
</p>

<p align="center">
  <img src="./local_login.png" alt="NiceCLI 本地启动页" width="88%" />
</p>

<p align="center">
  <img src="./codex_workspace_quota.png" alt="NiceCLI Workspace Quota 面板" width="88%" />
</p>

## 功能亮点

NiceCLI 保留了 EasyCLI 原有的核心操作价值，但把产品重心收敛到当前项目最需要的工作流上：本地启动、管理认证文件、查看运行状态，以及在无需远程管理配置的前提下，查看同一个 ChatGPT 账号下多个 workspace 的 quota。

- **仅保留本地启动**：没有远程连接流程，也不再要求设置 remote-management 密码，启动后可以更快进入控制面板
- **内嵌 CLIProxyAPI 运行时**：桌面应用会打包并启动它所需的本地后端
- **Codex Workspace Quota 可视化**：支持查看同一个 ChatGPT 账号下多个 workspace 的 quota，并显示剩余额度、进度条与定时自动刷新
- **认证文件备注**：可给 auth file 添加备注，并在 quota 面板中以 `备注（邮箱）` 的形式展示
- **更聚焦的桌面体验**：使用 NiceCLI 自有品牌，减少启动摩擦，简化导航，并提供更清晰的深色界面
- **主题与语言设置**：内置深浅色切换，并为 i18n 多语言支持打下基础
- **便携版分发**：当前版本以可直接运行的 `nicecli.exe` 便携形式为主

## 为什么是 NiceCLI

EasyCLI 是一个很好的基础，但这个分支有意做得更聚焦。NiceCLI 去掉了 remote-first 的复杂性，裁掉了不适合本地桌面场景的流程，并补上了日常账号管理与额度排查真正需要的产品细节，尤其是在你需要查看同一个 ChatGPT 账号下多个 workspace 的 quota 时。

这个项目目前围绕以下目标展开：

- 更快地启动本地运行环境
- 让认证文件管理更直观、更实用
- 在同一个 ChatGPT 账号存在多个 workspace 时，更容易定位 workspace 额度问题
- 保持单机桌面产品体验，而不是远程管理控制台

## 快速开始

面向 Windows 用户：

1. 如果系统中还没有可用的 Microsoft Edge WebView2 Runtime，请先安装。
2. 打开便携版发布目录，例如 `NiceCLI_Portable/v0.3.5/`。
3. 运行 `nicecli.exe`。
4. NiceCLI 会启动本地运行时并打开控制面板。

启动后，你可以：

- 管理认证文件
- 给认证文件添加备注
- 查看 Codex Workspace Quota，并查看同一个 ChatGPT 账号下多个 workspace 的 quota
- 调整本地界面设置，例如主题和语言

## 开发

当前工作区使用的有效源码目录为：

- `source_code/EasyCLI-0.1.32`：NiceCLI 桌面前端与 Tauri 壳层
- `source_code/CLIProxyAPI-6.9.7`：内嵌使用的 CLIProxyAPI 后端分支

当前工作区中保留的参考仓库：

- `EasyCLI-main`：上游 / 参考用 EasyCLI 仓库
- `tldraw-main`：README 风格与结构参考仓库

本地开发前提：

- Node.js 18+
- Rust toolchain，用于 Tauri 构建
- Go toolchain，用于 CLIProxyAPI 构建
- Windows 上可用的 WebView2 Runtime

## 构建

当前正在使用的源码树位于 `source_code/` 下。如果你直接使用根目录的辅助脚本，请先确认脚本中的工作区路径和你本地实际布局一致。

开发 / 便携版构建：

```powershell
powershell -ExecutionPolicy Bypass -File .\build-windows-dev.ps1
```

安装包 / bundled 构建：

```powershell
powershell -ExecutionPolicy Bypass -File .\build-windows.ps1
```

整体构建流程包括：

- 编译 CLIProxyAPI 后端
- 将后端资源放入 Tauri 应用中进行内嵌
- 准备前端静态资源
- 构建 `nicecli.exe`

## 项目结构

- `README.md`：英文版项目说明
- `README_CN.md`：中文版项目说明
- `plan.md`：当前执行计划与工作记录
- `nicecli-logo.png`：NiceCLI 品牌图标
- `NiceCLI_Portable/`：便携版发布输出目录
- `source_code/`：当前实际开发使用的源码目录
- `build-windows-dev.ps1`：便携 / 开发版构建脚本
- `build-windows.ps1`：安装包构建脚本

`source_code/` 内部结构：

- `EasyCLI-0.1.32/`
  - `login.html`、`settings.html`、`css/`、`js/`：桌面端 UI
  - `src-tauri/`：Rust / Tauri 宿主应用
- `CLIProxyAPI-6.9.7/`
  - `cmd/server/`：后端入口
  - `internal/`：运行时、API、quota、管理逻辑
  - `sdk/`：路由、auth 调度、provider 执行层

## 相比 EasyCLI 改了什么

这个分支已经在多个可见层面上偏离了原始 EasyCLI：

- 更名并重塑为 NiceCLI
- 启动流程改为 local-only
- 为桌面场景加入内嵌后端打包
- 去掉了启动时的更新检查摩擦
- 重做了 Workspace Quota 面板的行为与数据展示，以支持查看同一个 ChatGPT 账号下多个 workspace 的 quota
- 将 auth file 备注功能接入 quota 可视化
- 加入新的视觉风格、主题切换与语言设置基础能力

## 当前发布形态

目前项目主要围绕 Windows 便携版工作流进行优化。当前最实际的产物是：

- `NiceCLI_Portable/v0.3.5/nicecli.exe`

与该版本对应的源码快照放在同目录下：

- `NiceCLI_Portable/v0.3.5/source_code/`

## 说明

- NiceCLI 基于 EasyCLI 与 CLIProxyAPI 进行项目化改造，目标是更贴合本地桌面使用场景。
- 当前这份 README 仍然是草稿，后续可以随着版本、打包方式和对外发布方式稳定后继续收敛。
