<p align="center">
  <img src="./nicecli-logo.png" alt="NiceCLI" width="160" />
</p>

<p align="center">
  <img src="https://img.shields.io/badge/platform-Windows-0078D4" alt="Windows" />
  <img src="https://img.shields.io/badge/runtime-Tauri%202-24C8DB" alt="Tauri 2" />
  <img src="https://img.shields.io/badge/backend-Rust%20local%20backend-DEA584" alt="Rust local backend" />
  <img src="https://img.shields.io/badge/distribution-portable%20exe-3A7AFE" alt="Portable exe" />
  <img src="https://img.shields.io/badge/mode-local--only-111111" alt="Local only" />
</p>

<p align="center">
  <a href="./README.md">English</a> | <a href="./README_CN.md">中文</a>
</p>

<h3 align="center">一个面向 NiceCLI 的本地优先桌面控制台。</h3>

<p align="center">
  NiceCLI 是一个以 Windows 为主的桌面应用，起源于 EasyCLI 和 CLIProxyAPI，现在作为统一仓库持续维护。它把 Tauri 桌面壳层和本地 Rust 后端打包成一个本地工作流，让你无需单独安装后端，就可以直接启动、管理认证文件、查看运行状态，以及查看 Codex workspace quota。
</p>

<p align="center">
  <img src="./local_login.png" alt="NiceCLI 本地登录页" width="88%" />
</p>

<p align="center">
  <img src="./codex_workspace_quota.png" alt="NiceCLI Workspace Quota 页面" width="88%" />
</p>

## 功能亮点

- 仅保留本地启动流程，内嵌后端运行时
- 以便携版 `nicecli.exe` 为主，不依赖安装包
- 支持认证文件管理和备注编辑
- 支持查看同一账号下多个 workspace 的 Codex Workspace Quota
- 支持 workspace 维度的 quota 快照、重置倒计时和分组筛选
- 提供 NiceCLI 自己的桌面壳层、品牌和控制面板体验

## 快速开始

1. 在 Windows 上安装 Microsoft Edge WebView2 Runtime。
2. 构建或获取 `nicecli.exe`。
3. 运行 `nicecli.exe`。
4. NiceCLI 会自动启动内嵌的本地 Rust 后端，并打开控制面板。

## 开发环境要求

- Node.js 18+
- Rust toolchain
- Windows 上可用的 WebView2 Runtime

## 构建

在仓库根目录执行：

```powershell
powershell -ExecutionPolicy Bypass -File .\build-windows.ps1
```

构建输出：

```text
apps\nicecli\src-tauri\target\release\nicecli.exe
```

说明：

- 当前仓库只保留 portable exe 工作流
- setup / installer 打包已明确禁用
- 主构建链现在直接链接 Rust 后端，只使用 Rust / Node 工具链
- `.github/workflows/windows-rust-ci.yml` 是当前 GitHub Actions 的主验证入口，用于执行 Windows Rust 构建、根构建脚本、测试、backend smoke 检查，以及 tray-host smoke 检查
- `docs/maintenance.md` 记录当前构建、验证和发布维护主线

## 仓库结构

- `apps/nicecli`：桌面前端资源和 Tauri 宿主应用
- `crates/*`：Rust 后端、runtime、auth、config、quota、模型与契约测试 crate
- `scripts`：仓库级构建脚本
- `docs`：架构和维护说明
- `build-windows.ps1`：根目录构建入口

## 协作与治理

- [CONTRIBUTING.md](./CONTRIBUTING.md)：贡献流程与验证要求
- [CODE_OF_CONDUCT.md](./CODE_OF_CONDUCT.md)：协作行为约束
- [SECURITY.md](./SECURITY.md)：安全问题报告方式
- [SUPPORT.md](./SUPPORT.md)：支持与问题反馈入口
- [CHANGELOG.md](./CHANGELOG.md)：维护向变更记录

## 架构说明

NiceCLI 当前采用非常直接的运行结构：

- 前端运行在 Tauri 桌面壳层中
- Tauri 默认以同进程方式托管 Rust 后端
- UI 通过本地 loopback management API 与后端通信

这样可以在保持部署简单的前提下，继续维持本地 auth、quota 和管理工作流。

运行结构见 [docs/architecture.md](./docs/architecture.md)，当前构建、CI 和验证主线见 [docs/maintenance.md](./docs/maintenance.md)。

## 当前维护方向

- 保持 NiceCLI 为 local-only
- 保持单文件 portable `nicecli.exe` 分发
- 保持 Rust 后端作为唯一默认运行主线
- 持续优化 auth file 与 workspace quota 工作流
- 把仓库作为一个统一项目维护，而不是两个并排的历史项目目录

## 许可证

本项目按 [LICENSE](./LICENSE) 中的条款分发。
