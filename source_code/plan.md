# 使用规则
- 不要过度设计，不要写过度兜底或过度兼容的代码；保持逻辑清晰、可维护。
- 每次进入新任务前，先看本文件最前面的未完成项。
- 按步骤连续完成代码、验证、构建和同步，不因小问题中断。
- 完成某一步后，把 `[ ]` 改成 `[x]`，并补 `代码完成注：日期 / 文件 / 功能`。
- 如果执行中发现计划与当前代码冲突，允许直接修正本文件，但要保持文档精简。
- 除非缺权限、缺数据或文档冲突，否则不要中途停下来让用户做决定。

# 当前硬约束
- 只编辑 `D:\dev\wincli\source_code\CLIProxyAPI-6.9.7` 和 `D:\dev\wincli\source_code\EasyCLI-0.1.32`。
- 不覆盖 `D:\dev\wincli\NiceCLI_Portable\v0.3.0`。
- `v0.3.0` 保持 `Local-only`；不恢复 Remote 模式、远程入口或远程密码逻辑。
- `v0.3.0` 继续保持单 `nicecli.exe` 分发，不退回外置 backend 安装链路。
- quota 快照继续独立于 `/v0/management/usage`，唯一键保持 `provider + auth_id + workspace_id`。
- 前端继续复用本地 loopback HTTP management API，不做大规模 IPC 重写。
- 第三方 API / provider 执行链暂时保留；后续清理优先砍桌面版未用能力，不先砍主能力。

# 当前稳定基线
- [x] 当前开发基线已切到 `D:\dev\wincli\source_code`。
- [x] `desktop-lite` 已切到 `CLIProxyAPI-6.9.7/cmd/desktoplite`，NiceCLI 内嵌后端走该入口。
- [x] `v0.3.0` 已是单 `nicecli.exe` 分发，启动后会解包并拉起内嵌 `CLIProxyAPI`。
- [x] desktop-lite 已移除 control panel asset 链路和未使用管理面入口，管理鉴权收缩为 `localhost + runtime local password`。
- [x] NiceCLI 已收口为单窗口切页，`login.html` 和 `settings.html` 复用 `main` 窗口。
- [x] NiceCLI 前端主流程已收口为 `local-only`。
- [x] 复杂核心区 `sdk/cliproxy/auth/*` 与 `internal/quota/*` 本轮只做过只读审查，暂不默认收口。
- [x] 当前可用产物：`D:\dev\wincli\NiceCLI_Portable\v0.3.5\nicecli.exe`
- [x] 当前源码目录：`D:\dev\wincli\source_code`
- [x] 旧根目录源码树与临时目录已删除，后续默认不再回到 `D:\dev\wincli\EasyCLI-0.1.32`、`D:\dev\wincli\CLIProxyAPI-6.9.7` 等旧路径。

# 后续更新入口
- [ ] 新需求开始前，先判断影响范围属于哪一层：`NiceCLI Tauri 壳`、`NiceCLI 前端`、`CLIProxyAPI desktop-lite 主链`、`quota/auth 核心区`。
- [ ] 先列出本轮的 `可删项 / 可收敛项 / 必须保留项`，再开始改代码。
- [ ] 默认先改稳定外层：旧命令残留、旧兼容分支、重复包装；没有明确收益时不先动复杂核心区。
- [ ] 如果需要重新发版，按“前端准备 -> release 构建 -> 同步到便携目录 -> 验证目录结构”执行。
- [x] 当前任务 / NiceCLI 图标裁剪与窗口放大
  可删项：无
  可收敛项：窗口初始尺寸统一由 `tauri.conf.json` 和 `src-tauri/src/main.rs` 同步维护，避免 login 首启与后续切页尺寸不一致
  必须保留项：单窗口切页、现有 Local-only 流程、Windows 下现有 `icon.ico` 使用链路
  完成情况：已完成；窗口尺寸统一放大 20%，并按当前 `nicecli-logo.png` 重生成裁掉四周 15% 的 `icon.png` / `icon.ico`
- [x] 当前任务 / 删除认证文件后清理 Workspace Quota 残留快照
  可删项：删除后失去对应 `auth_id` 的 quota 缓存项
  可收敛项：`ListSnapshotsWithOptions` / `RefreshWithOptions` 共用一套“按当前 auth 枚举结果同步缓存”的逻辑
  必须保留项：现有 quota 快照模型、当前 auth 元数据覆盖逻辑、删除认证文件后的其他 auth 快照
  完成情况：已完成；删除认证文件后，缺失 `auth_id` 的 quota 快照会在列表与刷新路径上同步清理

# 重点检查入口
- NiceCLI 壳：`source_code\EasyCLI-0.1.32\src-tauri\src\main.rs`
- NiceCLI 前端主流：`source_code\EasyCLI-0.1.32\js\config-manager.js`、`source_code\EasyCLI-0.1.32\js\login.js`、`source_code\EasyCLI-0.1.32\js\settings-*.js`
- desktop-lite 主链：`source_code\CLIProxyAPI-6.9.7\cmd\desktoplite\main.go`、`source_code\CLIProxyAPI-6.9.7\sdk\cliproxy\builder.go`、`source_code\CLIProxyAPI-6.9.7\sdk\cliproxy\service.go`
- 管理 API：`source_code\CLIProxyAPI-6.9.7\internal\api\server.go`、`source_code\CLIProxyAPI-6.9.7\internal\api\handlers\management\handler.go`
- quota / auth 核心区：`source_code\CLIProxyAPI-6.9.7\internal\quota\*`、`source_code\CLIProxyAPI-6.9.7\sdk\cliproxy\auth\*`

# 发布与验证
- `cd D:\dev\wincli\source_code\CLIProxyAPI-6.9.7 && go test ./internal/api/... ./internal/quota/... ./sdk/cliproxy/...`
- `cd D:\dev\wincli\source_code\CLIProxyAPI-6.9.7 && go test -tags desktoplite ./cmd/desktoplite`
- `cd D:\dev\wincli\source_code\EasyCLI-0.1.32 && node src-tauri/prepare-web.js`
- `cd D:\dev\wincli\source_code\EasyCLI-0.1.32\src-tauri && cargo build --release --bin nicecli`
- 便携版输出目录：`D:\dev\wincli\NiceCLI_Portable\v0.3.5`
- 源码目录：`D:\dev\wincli\source_code`

# 代码完成注
- [x] 2026-04-03 / `EasyCLI-0.1.32/src-tauri/build.rs`、`CLIProxyAPI-6.9.7/cmd/desktoplite/main.go` / NiceCLI 内嵌后端切到 `desktop-lite` 入口。
- [x] 2026-04-03 / `EasyCLI-0.1.32/src-tauri/src/main.rs`、`EasyCLI-0.1.32/js/*` / NiceCLI 收口为单窗口、local-only 主流程，并清理旧命令与旧前端分支。
- [x] 2026-04-03 / `CLIProxyAPI-6.9.7/sdk/cliproxy/*`、`CLIProxyAPI-6.9.7/internal/api/*` / desktop-lite 主链、辅助抽象、管理路由与鉴权完成一轮收口。
- [x] 2026-04-03 / `CLIProxyAPI-6.9.7/sdk/cliproxy/auth/*`、`CLIProxyAPI-6.9.7/internal/quota/*` / 复杂核心区完成只读审查，当前默认不继续动。
- [x] 2026-04-03 / `D:\dev\wincli\NiceCLI_Portable\v0.3.0\nicecli.exe`、`D:\dev\wincli\NiceCLI_Portable\v0.3.0\source_code` / 完成 v0.3.0 便携版与源码快照同步。
- [x] 2026-04-03 / `plan.md` / 后续开发基线切到 `D:\dev\wincli\source_code`，以独立源码目录作为默认编辑目标。
- [x] 2026-04-03 / `D:\dev\wincli\EasyCLI-0.1.32`、`D:\dev\wincli\dist\cliproxyapi\6.9.7`、`D:\dev\wincli\CLIProxyAPI-6.9.7`、`D:\dev\wincli\_icon_tmp_nicecli`、`D:\dev\wincli\tmp-desktoplite-test` / 删除旧源码树与临时目录，后续只保留 `v0.3.0\source_code` 作为开发基线。
- [x] 2026-04-03 / `source_code\EasyCLI-0.1.32\src-tauri\src\main.rs`、`source_code\EasyCLI-0.1.32\src-tauri\tauri.conf.json`、`source_code\EasyCLI-0.1.32\images\icon.png`、`source_code\EasyCLI-0.1.32\images\icon.ico`、`source_code\EasyCLI-0.1.32\src-tauri\icons\icon.png`、`source_code\EasyCLI-0.1.32\src-tauri\icons\icon.ico` / 登录页与控制面板窗口按比例放大 20%，并重生成裁掉四周 15% 的 Windows 图标资源；执行 `node src-tauri/prepare-web.js` 与 `cargo build --release --bin nicecli` 验证通过。
- [x] 2026-04-03 / `D:\dev\wincli\NiceCLI_Portable\v0.3.5\nicecli.exe`、`plan.md` / 当前及后续构建产物默认同步到 `v0.3.5`，保留 `v0.3.0` 不覆盖。
- [x] 2026-04-03 / `source_code\CLIProxyAPI-6.9.7\internal\quota\cache.go`、`source_code\CLIProxyAPI-6.9.7\internal\quota\service.go`、`source_code\CLIProxyAPI-6.9.7\internal\quota\cache_test.go`、`source_code\CLIProxyAPI-6.9.7\internal\quota\service_test.go`、`D:\dev\wincli\NiceCLI_Portable\v0.3.5\nicecli.exe` / 修复删除认证文件后 Workspace Quota 仍显示旧快照的问题：quota 缓存会按当前 auth 枚举结果同步清理；执行 `go test ./internal/quota/...`、`go test ./internal/api/...`、`node src-tauri/prepare-web.js` 与 `cargo build --release --bin nicecli` 验证通过。
