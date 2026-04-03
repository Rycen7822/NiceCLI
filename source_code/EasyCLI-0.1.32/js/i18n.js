(function () {
  const STORAGE_KEY = "nicecli-language";
  const SUPPORTED_LANGUAGES = ["en", "zh-CN"];

  const resources = {
    en: {
      translation: {
        common: {
          unknown: "Unknown",
          unavailable: "Unavailable",
          cancel: "Cancel",
          save: "Save",
          delete: "Delete",
          reset: "Reset",
          apply: "Apply",
          refresh: "Refresh",
          refreshing: "Refreshing...",
          loading: "Loading...",
          addKey: "Add Key",
          addProvider: "Add Provider",
          startLocal: "Start Local",
          checking: "Checking...",
          updating: "Updating...",
          selectAll: "Select All",
          unselectAll: "Unselect All",
          new: "New",
          download: "Download",
        },
        sidebar: {
          title: "NiceCLI Control Panel",
          settings: "Settings",
          server: "Server",
          accessToken: "Access Token",
          authFiles: "Authentication Files",
          apiKeys: "Third Party API Keys",
          openaiCompatibility: "OpenAI Compatibility",
          workspaceQuota: "Workspace Quota",
        },
        status: {
          local: "Local",
          remote: "Remote",
          unknown: "Unknown",
        },
        appSettings: {
          title: "App Settings",
          description:
            "Adjust the local NiceCLI experience without changing CLIProxyAPI server configuration.",
          themeLabel: "Theme",
          themeDescription:
            "Switch between light and dark appearance for NiceCLI.",
          languageLabel: "Language",
          languageDescription:
            "Choose the display language used by the login and settings pages.",
          light: "Light",
          dark: "Dark",
          english: "English",
          simplifiedChinese: "Simplified Chinese",
        },
        login: {
          pageTitle: "NiceCLI",
          localTitle: "Local",
          localDescription:
            "Start the bundled CLIProxyAPI server on this machine.",
          proxyLabel: "Proxy Server (Optional):",
          proxyPlaceholder:
            "http://host:port or https://host:port or socks5://user:pass@host:port",
          proxyHelp: "Support HTTP, HTTPS, and SOCKS5 proxy servers",
          progressDownloading: "Downloading CLIProxyAPI...",
          updateDialogTitle: "New Version Found",
          updateDialogMessage:
            "A new version is available. Do you want to update to the latest version?",
          updateLater: "Update Later",
          updateNow: "Update Now",
          startingLocal: "Starting local mode...",
          invalidProxyFormat:
            "Invalid proxy format. Supported formats: http://host:port, https://host:port, socks5://host:port, http://user:pass@host:port, https://user:pass@host:port, socks5://user:pass@host:port",
          processStartFailed: "CLIProxyAPI process start failed",
          processStartError: "CLIProxyAPI process start error",
          checkVersionFailed: "Failed to check version: {{error}}",
          checkVersionError: "Error checking version: {{error}}",
          tauriRequired: "This feature requires Tauri environment",
          updateFailed: "Failed to update CLIProxyAPI: {{error}}",
          updateError: "Error updating CLIProxyAPI: {{error}}",
          checkingVersion: "Checking version...",
          downloading: "Downloading CLIProxyAPI...",
          downloadCompleted: "Download completed!",
          latestVersion:
            "CLIProxyAPI {{version}} is already the latest version!",
          localReady: "Using local CLIProxyAPI {{version}}.",
          operationFailed: "Operation failed: {{error}}",
          connectionError: "Connection error: {{error}}",
          reason: "Reason: {{reason}}",
          processExited:
            "CLIProxyAPI process exited abnormally, exit code: {{code}}",
          downloadedSuccess:
            "CLIProxyAPI {{version}} downloaded and extracted successfully!",
          updatePrompt:
            "Current version: {{version}}\nLatest version: {{latestVersion}}\n\nDo you want to update to the latest version?",
        },
        settings: {
          pageTitle: "NiceCLI Control Panel",
        },
        basic: {
          portLabel: "Port",
          portDescription: "Server port number (default: 8080)",
          allowRemoteLabel: "Allow Remote Management",
          allowRemoteDescription:
            "Allow remote management access from other hosts",
          autoStartLabel: "Start at Login",
          autoStartDescription: "Launch NiceCLI automatically when you log in",
          secretKeyLabel: "Remote Management Secret Key",
          secretKeyDescription:
            "Secret key for remote management authentication",
          debugLabel: "Debug Mode",
          debugDescription: "Enable debug logging for troubleshooting",
          proxyLabel: "Proxy URL",
          proxyDescription:
            "Configure proxy server URL (e.g., socks5://user:pass@127.0.0.1:1080/)",
          requestLogLabel: "Request Log",
          requestLogDescription: "Enable request logging for debugging",
          requestRetryLabel: "Request Retry",
          requestRetryDescription:
            "Number of retry attempts for failed requests",
          switchProjectLabel: "Switch Project on Quota Exceeded",
          switchProjectDescription:
            "Automatically switch to another project when quota is exceeded",
          switchPreviewLabel: "Switch Preview Model on Quota Exceeded",
          switchPreviewDescription:
            "Automatically switch to preview model when quota is exceeded",
        },
        accessToken: {
          title: "Access Token",
          loading: "Loading access tokens...",
        },
        authFiles: {
          loading: "Loading authentication files...",
          emptyTitle: "No authentication files",
          emptySubtitle: "Upload authentication files to manage them here",
          emailPrefix: "Email: {{email}}",
          notePrefix: "Note: ",
          noNote: "No note",
          remarkBadge: "Remark",
          addNote: "Add Note",
          editNote: "Edit Note",
          noteDialogTitle: "Authentication File Remark",
          authFileLabel: "Authentication File",
          emailLabel: "Email",
          remarkLabel: "Remark",
          remarkPlaceholder:
            "Add a remark to identify this authentication file",
          remarkUpdated: "Remark updated",
          remarkCleared: "Remark cleared",
          remarkUpdateFailed: "Failed to update remark",
          deleteConfirmTitle: "Confirm Delete",
          deleteConfirmMessage:
            "Are you sure you want to delete {{count}} authentication {{fileLabel}}?\nThis action cannot be undone.",
          fileSingular: "file",
          filePlural: "files",
          deleteSuccess: "Deleted {{count}} file(s) successfully",
          deleteFailed: "Failed to delete {{count}} file(s)",
          deleteInProgress: "Deleting...",
          saveInProgress: "Saving...",
          geminiWebTitle: "Gemini WEB Authentication",
          geminiWebDescription: "Please enter your Gemini Web cookies:",
          secure1psid: "Secure-1PSID:",
          secure1psidts: "Secure-1PSIDTS:",
          confirm: "Confirm",
          enterGeminiTokens:
            "Please enter email, Secure-1PSID and Secure-1PSIDTS",
          geminiSaveSuccess: "Gemini Web tokens saved successfully",
          geminiSaveFailed: "Failed to save Gemini Web tokens: {{error}}",
          uploadSuccess: "Uploaded {{count}} file(s) successfully",
          uploadFailed: "Failed to upload files",
          downloadSuccess: "Downloaded {{count}} file(s) successfully",
          downloadFailed: "Failed to download {{count}} file(s)",
          newTypes: {
            gemini: "Gemini CLI",
            geminiWeb: "Gemini WEB",
            claude: "Claude Code",
            codex: "Codex",
            qwen: "Qwen Code",
            vertex: "Vertex",
            iflow: "iFlow",
            antigravity: "Antigravity",
            local: "Local File",
          },
        },
        api: {
          geminiTitle: "Gemini API Keys",
          codexTitle: "Codex API Keys",
          claudeTitle: "Claude Code API Keys",
          loadingGemini: "Loading Gemini API keys...",
          loadingCodex: "Loading Codex API keys...",
          loadingClaude: "Loading Claude API keys...",
        },
        openai: {
          title: "OpenAI Compatibility Providers",
        },
        workspaceQuota: {
          title: "Workspace Quota",
          description:
            "View the latest Codex workspace quota snapshots grouped by account and workspace.",
          loading: "Loading workspace quota snapshots...",
          loadFailed: "Failed to load workspace quota",
          emptyTitle: "No workspace quota snapshots",
          emptySubtitle:
            "Open this tab or click Refresh to fetch Codex quota snapshots.",
          noMatchTitle: "No snapshots match the current filters",
          noMatchSubtitle:
            "Adjust the account, workspace, or stale filter and try again.",
          allAccounts: "All accounts",
          allWorkspaces: "All workspaces",
          filterAccount: "Account",
          filterWorkspace: "Workspace",
          staleOnly: "Only stale",
          currentWorkspace: "Current Workspace",
          unknownAuth: "Unknown Auth",
          accountsSummary: "Accounts {{count}}",
          snapshotsSummary: "Snapshots {{count}}",
          staleSummary: "Stale {{count}}",
          errorsSummary: "Errors {{count}}",
          workspaceCount_one: "{{count}} workspace",
          workspaceCount_other: "{{count}} workspaces",
          plan: "Plan {{value}}",
          limit: "Limit {{value}}",
          stale: "Stale",
          error: "Error",
          source: "Source {{value}}",
          fetched: "Fetched {{value}}",
          primary: "Primary",
          secondary: "Secondary",
          credits: "Credits",
          noData: "No data",
          noWindowSnapshot: "No window snapshot available",
          remaining: "{{value}}% remaining",
          remainingUnavailable: "Remaining unavailable",
          windowMinutes: "{{count}} minute window",
          windowUnavailable: "Window unavailable",
          resetsAt: "Resets {{value}}",
          resetUnavailable: "Reset time unavailable",
          unlimited: "Unlimited",
          meteredCredits: "Metered credits are not enforced",
          creditsAvailable: "Credits available",
          balanceReported: "Balance reported",
          creditsAttached: "Credits are attached to this workspace",
          available: "Available",
          unknownPlan: "Unknown",
          refreshSuccess: "Workspace quota refreshed",
        },
        toasts: {
          failedToLoadSettings: "Failed to load settings",
          failedToResetSettings: "Failed to reset settings",
          autoStartEnabled: "Auto-start enabled successfully",
          autoStartEnableFailed: "Failed to enable auto-start",
          autoStartDisabled: "Auto-start disabled successfully",
          autoStartDisableFailed: "Failed to disable auto-start",
          autoStartUpdateFailed: "Failed to update auto-start setting",
          noChanges: "No changes to apply in {{tab}}",
          appliedSettings: "Applied {{count}} {{tab}} setting(s) successfully",
          failedToApply: "Failed to apply {{count}} setting(s)",
          networkError: "Network error",
          portRestartSaved:
            "Port configuration saved, restarting CLIProxyAPI process...",
          resetToServer: "{{tab}} reset to server config",
          themeUpdated: "Theme updated",
          languageUpdated: "Language updated",
        },
        tabs: {
          basic: "server",
          accessToken: "access token",
          api: "third party API keys",
          openai: "OpenAI compatibility",
        },
      },
    },
    "zh-CN": {
      translation: {
        common: {
          unknown: "未知",
          unavailable: "不可用",
          cancel: "取消",
          save: "保存",
          delete: "删除",
          reset: "重置",
          apply: "应用",
          refresh: "刷新",
          refreshing: "刷新中...",
          loading: "加载中...",
          addKey: "添加 Key",
          addProvider: "添加 Provider",
          startLocal: "启动本地模式",
          checking: "检查中...",
          updating: "更新中...",
          selectAll: "全选",
          unselectAll: "取消全选",
          new: "新建",
          download: "下载",
        },
        sidebar: {
          title: "NiceCLI 控制面板",
          settings: "设置",
          server: "服务",
          accessToken: "Access Token",
          authFiles: "认证文件",
          apiKeys: "第三方 API Keys",
          openaiCompatibility: "OpenAI 兼容",
          workspaceQuota: "Workspace Quota",
        },
        status: {
          local: "本地",
          remote: "远程",
          unknown: "未知",
        },
        appSettings: {
          title: "应用设置",
          description:
            "调整 NiceCLI 本地界面体验，不会修改 CLIProxyAPI 的服务端配置。",
          themeLabel: "主题",
          themeDescription: "切换 NiceCLI 的浅色 / 深色外观。",
          languageLabel: "语言",
          languageDescription: "选择登录页和设置页的显示语言。",
          light: "浅色",
          dark: "深色",
          english: "English",
          simplifiedChinese: "简体中文",
        },
        login: {
          pageTitle: "NiceCLI",
          localTitle: "本地",
          localDescription: "在当前机器上启动内置的 CLIProxyAPI 服务。",
          proxyLabel: "代理服务器（可选）：",
          proxyPlaceholder:
            "http://host:port 或 https://host:port 或 socks5://user:pass@host:port",
          proxyHelp: "支持 HTTP、HTTPS 和 SOCKS5 代理",
          progressDownloading: "正在下载 CLIProxyAPI...",
          updateDialogTitle: "发现新版本",
          updateDialogMessage: "检测到新版本，是否现在更新到最新版？",
          updateLater: "稍后更新",
          updateNow: "立即更新",
          startingLocal: "正在启动本地模式...",
          invalidProxyFormat:
            "代理格式无效。支持格式：http://host:port、https://host:port、socks5://host:port、http://user:pass@host:port、https://user:pass@host:port、socks5://user:pass@host:port",
          processStartFailed: "CLIProxyAPI 进程启动失败",
          processStartError: "CLIProxyAPI 进程启动出错",
          checkVersionFailed: "检查版本失败：{{error}}",
          checkVersionError: "检查版本出错：{{error}}",
          tauriRequired: "此功能需要在 Tauri 环境下运行",
          updateFailed: "更新 CLIProxyAPI 失败：{{error}}",
          updateError: "更新 CLIProxyAPI 出错：{{error}}",
          checkingVersion: "正在检查版本...",
          downloading: "正在下载 CLIProxyAPI...",
          downloadCompleted: "下载完成！",
          latestVersion: "CLIProxyAPI {{version}} 已是最新版本！",
          localReady: "正在使用本地 CLIProxyAPI {{version}}。",
          operationFailed: "操作失败：{{error}}",
          connectionError: "连接错误：{{error}}",
          reason: "原因：{{reason}}",
          processExited: "CLIProxyAPI 异常退出，退出码：{{code}}",
          downloadedSuccess: "CLIProxyAPI {{version}} 已成功下载并解压！",
          updatePrompt:
            "当前版本：{{version}}\n最新版本：{{latestVersion}}\n\n是否现在更新到最新版？",
        },
        settings: {
          pageTitle: "NiceCLI 控制面板",
        },
        basic: {
          portLabel: "端口",
          portDescription: "服务端口号（默认：8080）",
          allowRemoteLabel: "允许远程管理",
          allowRemoteDescription: "允许其他主机远程访问管理接口",
          autoStartLabel: "开机启动",
          autoStartDescription: "登录系统时自动启动 NiceCLI",
          secretKeyLabel: "远程管理密钥",
          secretKeyDescription: "用于远程管理认证的密钥",
          debugLabel: "调试模式",
          debugDescription: "开启调试日志，便于排查问题",
          proxyLabel: "代理 URL",
          proxyDescription:
            "配置代理服务器地址（例如 socks5://user:pass@127.0.0.1:1080/）",
          requestLogLabel: "请求日志",
          requestLogDescription: "开启请求日志，便于调试",
          requestRetryLabel: "请求重试次数",
          requestRetryDescription: "请求失败时的重试次数",
          switchProjectLabel: "额度超限时切换项目",
          switchProjectDescription: "当额度超限时自动切换到其他项目",
          switchPreviewLabel: "额度超限时切换预览模型",
          switchPreviewDescription: "当额度超限时自动切换到预览模型",
        },
        accessToken: {
          title: "Access Token",
          loading: "正在加载 Access Token...",
        },
        authFiles: {
          loading: "正在加载认证文件...",
          emptyTitle: "暂无认证文件",
          emptySubtitle: "上传认证文件后即可在这里管理",
          emailPrefix: "邮箱：{{email}}",
          notePrefix: "备注：",
          noNote: "暂无备注",
          remarkBadge: "备注",
          addNote: "添加备注",
          editNote: "编辑备注",
          noteDialogTitle: "认证文件备注",
          authFileLabel: "认证文件",
          emailLabel: "邮箱",
          remarkLabel: "备注",
          remarkPlaceholder: "填写备注，便于识别这个认证文件",
          remarkUpdated: "备注已更新",
          remarkCleared: "备注已清空",
          remarkUpdateFailed: "更新备注失败",
          deleteConfirmTitle: "确认删除",
          deleteConfirmMessage:
            "确定要删除 {{count}} 个认证{{fileLabel}}吗？\n此操作不可撤销。",
          fileSingular: "文件",
          filePlural: "文件",
          deleteSuccess: "成功删除 {{count}} 个文件",
          deleteFailed: "删除失败 {{count}} 个文件",
          deleteInProgress: "删除中...",
          saveInProgress: "保存中...",
          geminiWebTitle: "Gemini WEB 认证",
          geminiWebDescription: "请输入 Gemini Web Cookies：",
          secure1psid: "Secure-1PSID：",
          secure1psidts: "Secure-1PSIDTS：",
          confirm: "确认",
          enterGeminiTokens: "请填写邮箱、Secure-1PSID 和 Secure-1PSIDTS",
          geminiSaveSuccess: "Gemini Web Tokens 保存成功",
          geminiSaveFailed: "保存 Gemini Web Tokens 失败：{{error}}",
          uploadSuccess: "成功上传 {{count}} 个文件",
          uploadFailed: "上传文件失败",
          downloadSuccess: "成功下载 {{count}} 个文件",
          downloadFailed: "下载失败 {{count}} 个文件",
          newTypes: {
            gemini: "Gemini CLI",
            geminiWeb: "Gemini WEB",
            claude: "Claude Code",
            codex: "Codex",
            qwen: "Qwen Code",
            vertex: "Vertex",
            iflow: "iFlow",
            antigravity: "Antigravity",
            local: "本地文件",
          },
        },
        api: {
          geminiTitle: "Gemini API Keys",
          codexTitle: "Codex API Keys",
          claudeTitle: "Claude Code API Keys",
          loadingGemini: "正在加载 Gemini API Keys...",
          loadingCodex: "正在加载 Codex API Keys...",
          loadingClaude: "正在加载 Claude API Keys...",
        },
        openai: {
          title: "OpenAI 兼容 Providers",
        },
        workspaceQuota: {
          title: "Workspace Quota",
          description: "按账号和 workspace 分组查看最新的 Codex quota 快照。",
          loading: "正在加载 workspace quota 快照...",
          loadFailed: "加载 workspace quota 失败",
          emptyTitle: "暂无 workspace quota 快照",
          emptySubtitle: "打开这个页面或点击刷新即可拉取 Codex quota 快照。",
          noMatchTitle: "当前筛选条件下没有匹配结果",
          noMatchSubtitle: "调整账号、workspace 或 stale 条件后再试。",
          allAccounts: "全部账号",
          allWorkspaces: "全部 workspaces",
          filterAccount: "账号",
          filterWorkspace: "Workspace",
          staleOnly: "仅显示 stale",
          currentWorkspace: "当前 Workspace",
          unknownAuth: "未知账号",
          accountsSummary: "账号 {{count}}",
          snapshotsSummary: "快照 {{count}}",
          staleSummary: "Stale {{count}}",
          errorsSummary: "错误 {{count}}",
          workspaceCount_one: "{{count}} 个 workspace",
          workspaceCount_other: "{{count}} 个 workspace",
          plan: "套餐 {{value}}",
          limit: "限制 {{value}}",
          stale: "Stale",
          error: "错误",
          source: "来源 {{value}}",
          fetched: "获取时间 {{value}}",
          primary: "主额度",
          secondary: "次额度",
          credits: "Credits",
          noData: "暂无数据",
          noWindowSnapshot: "暂无窗口快照",
          remaining: "剩余 {{value}}%",
          remainingUnavailable: "剩余额度未知",
          windowMinutes: "{{count}} 分钟窗口",
          windowUnavailable: "窗口未知",
          resetsAt: "{{value}} 重置",
          resetUnavailable: "重置时间未知",
          unlimited: "无限制",
          meteredCredits: "未启用计量 Credits 限制",
          creditsAvailable: "Credits 可用",
          balanceReported: "已返回余额",
          creditsAttached: "该 workspace 已附带 Credits",
          available: "可用",
          unknownPlan: "未知",
          refreshSuccess: "Workspace quota 已刷新",
        },
        toasts: {
          failedToLoadSettings: "加载设置失败",
          failedToResetSettings: "重置设置失败",
          autoStartEnabled: "已开启开机启动",
          autoStartEnableFailed: "开启开机启动失败",
          autoStartDisabled: "已关闭开机启动",
          autoStartDisableFailed: "关闭开机启动失败",
          autoStartUpdateFailed: "更新开机启动设置失败",
          noChanges: "{{tab}}没有需要应用的改动",
          appliedSettings: "已成功应用 {{count}} 项{{tab}}设置",
          failedToApply: "有 {{count}} 项设置应用失败",
          networkError: "网络错误",
          portRestartSaved: "端口配置已保存，正在重启 CLIProxyAPI...",
          resetToServer: "{{tab}}已重置为服务端配置",
          themeUpdated: "主题已更新",
          languageUpdated: "语言已更新",
        },
        tabs: {
          basic: "服务",
          accessToken: "Access Token",
          api: "第三方 API Keys",
          openai: "OpenAI 兼容",
        },
      },
    },
  };

  const staticBindings = {
    login: [
      { selector: "title", key: "login.pageTitle" },
      { selector: ".card-title", key: "login.localTitle" },
      { selector: ".card-description", key: "login.localDescription" },
      { selector: ".proxy-label", key: "login.proxyLabel" },
      {
        selector: "#proxy-input",
        key: "login.proxyPlaceholder",
        attribute: "placeholder",
      },
      { selector: ".proxy-help", key: "login.proxyHelp" },
      { selector: "#continue-btn", key: "common.startLocal" },
      { selector: "#progress-label", key: "login.progressDownloading" },
      { selector: ".update-dialog-title", key: "login.updateDialogTitle" },
      { selector: "#update-dialog-message", key: "login.updateDialogMessage" },
      { selector: "#update-cancel-btn", key: "login.updateLater" },
      { selector: "#update-confirm-btn", key: "login.updateNow" },
    ],
    settings: [
      { selector: "title", key: "settings.pageTitle" },
      { selector: ".sidebar-title", key: "sidebar.title" },
      { selector: '.tab[data-tab="app-settings"]', key: "sidebar.settings" },
      { selector: '.tab[data-tab="basic"]', key: "sidebar.server" },
      {
        selector: '.tab[data-tab="access-token"]',
        key: "sidebar.accessToken",
      },
      { selector: '.tab[data-tab="auth"]', key: "sidebar.authFiles" },
      { selector: '.tab[data-tab="api"]', key: "sidebar.apiKeys" },
      {
        selector: '.tab[data-tab="openai"]',
        key: "sidebar.openaiCompatibility",
      },
      {
        selector: '.tab[data-tab="workspace-quota"]',
        key: "sidebar.workspaceQuota",
      },
      { selector: 'label[for="port-input"]', key: "basic.portLabel" },
      {
        selector: "#port-setting .setting-description",
        key: "basic.portDescription",
      },
      {
        selector: 'label[for="allow-remote-switch"]',
        key: "basic.allowRemoteLabel",
      },
      {
        selector: "#allow-remote-setting .setting-description",
        key: "basic.allowRemoteDescription",
      },
      {
        selector: 'label[for="auto-start-switch"]',
        key: "basic.autoStartLabel",
      },
      {
        selector: "#auto-start-setting .setting-description",
        key: "basic.autoStartDescription",
      },
      {
        selector: 'label[for="secret-key-input"]',
        key: "basic.secretKeyLabel",
      },
      {
        selector: "#secret-key-setting .setting-description",
        key: "basic.secretKeyDescription",
      },
      { selector: 'label[for="debug-switch"]', key: "basic.debugLabel" },
      {
        selector:
          '.setting-item label[for="debug-switch"] + .setting-description',
        key: "basic.debugDescription",
      },
      {
        selector: 'label[for="proxy-url-input"]',
        key: "basic.proxyLabel",
      },
      {
        selector:
          '.setting-item label[for="proxy-url-input"] + .setting-description',
        key: "basic.proxyDescription",
      },
      {
        selector: 'label[for="request-log-switch"]',
        key: "basic.requestLogLabel",
      },
      {
        selector:
          '.setting-item label[for="request-log-switch"] + .setting-description',
        key: "basic.requestLogDescription",
      },
      {
        selector: 'label[for="request-retry-input"]',
        key: "basic.requestRetryLabel",
      },
      {
        selector:
          '.setting-item label[for="request-retry-input"] + .setting-description',
        key: "basic.requestRetryDescription",
      },
      {
        selector: 'label[for="switch-project-switch"]',
        key: "basic.switchProjectLabel",
      },
      {
        selector:
          '.setting-item label[for="switch-project-switch"] + .setting-description',
        key: "basic.switchProjectDescription",
      },
      {
        selector: 'label[for="switch-preview-model-switch"]',
        key: "basic.switchPreviewLabel",
      },
      {
        selector:
          '.setting-item label[for="switch-preview-model-switch"] + .setting-description',
        key: "basic.switchPreviewDescription",
      },
      {
        selector: "#local-api-keys-section .api-section-title",
        key: "accessToken.title",
      },
      {
        selector: "#remote-api-keys-section .api-section-title",
        key: "accessToken.title",
      },
      { selector: "#add-local-api-key-btn", key: "common.addKey" },
      { selector: "#add-remote-api-key-btn", key: "common.addKey" },
      { selector: "#local-api-keys-loading span", key: "accessToken.loading" },
      { selector: "#remote-api-keys-loading span", key: "accessToken.loading" },
      { selector: "#auth-loading span", key: "authFiles.loading" },
      { selector: "#add-gemini-key-btn", key: "common.addKey" },
      { selector: "#add-codex-key-btn", key: "common.addKey" },
      { selector: "#add-claude-key-btn", key: "common.addKey" },
      { selector: "#gemini-loading span", key: "api.loadingGemini" },
      { selector: "#codex-loading span", key: "api.loadingCodex" },
      { selector: "#claude-loading span", key: "api.loadingClaude" },
      { selector: "#add-provider-btn", key: "common.addProvider" },
      {
        selector: ".workspace-quota-title",
        key: "workspaceQuota.title",
      },
      {
        selector: ".workspace-quota-description",
        key: "workspaceQuota.description",
      },
      {
        selector: "#workspace-quota-refresh-btn",
        key: "common.refresh",
      },
      {
        selector: 'label[for="workspace-quota-account-filter"]',
        key: "workspaceQuota.filterAccount",
      },
      {
        selector: 'label[for="workspace-quota-workspace-filter"]',
        key: "workspaceQuota.filterWorkspace",
      },
      {
        selector: ".workspace-quota-toggle span",
        key: "workspaceQuota.staleOnly",
      },
      { selector: "#reset-btn", key: "common.reset" },
      { selector: "#apply-btn", key: "common.apply" },
      { selector: "#new-btn", key: "common.new" },
      { selector: "#download-btn", key: "common.download" },
      { selector: "#delete-btn", key: "common.delete" },
      {
        selector: '.dropdown-item[data-type="gemini"]',
        key: "authFiles.newTypes.gemini",
      },
      {
        selector: '.dropdown-item[data-type="gemini-web"]',
        key: "authFiles.newTypes.geminiWeb",
      },
      {
        selector: '.dropdown-item[data-type="claude"]',
        key: "authFiles.newTypes.claude",
      },
      {
        selector: '.dropdown-item[data-type="codex"]',
        key: "authFiles.newTypes.codex",
      },
      {
        selector: '.dropdown-item[data-type="qwen"]',
        key: "authFiles.newTypes.qwen",
      },
      {
        selector: '.dropdown-item[data-type="vertex"]',
        key: "authFiles.newTypes.vertex",
      },
      {
        selector: '.dropdown-item[data-type="iflow"]',
        key: "authFiles.newTypes.iflow",
      },
      {
        selector: '.dropdown-item[data-type="antigravity"]',
        key: "authFiles.newTypes.antigravity",
      },
      {
        selector: '.dropdown-item[data-type="local"]',
        key: "authFiles.newTypes.local",
      },
      { selector: "#confirm-title", key: "authFiles.deleteConfirmTitle" },
      { selector: "#confirm-cancel", key: "common.cancel" },
      { selector: "#confirm-delete", key: "common.delete" },
      { selector: "#modal-cancel", key: "common.cancel" },
      { selector: "#modal-save", key: "common.save" },
      { selector: "#access-token-modal-cancel", key: "common.cancel" },
      { selector: "#access-token-modal-save", key: "common.save" },
      { selector: "#provider-modal-cancel", key: "common.cancel" },
      { selector: "#provider-modal-save", key: "common.save" },
    ],
  };

  let currentPage = "";
  let initialized = false;
  let listenerRegistered = false;

  function normalizeLanguage(language) {
    if (!language) {
      return "en";
    }

    const normalized = String(language).trim().toLowerCase();
    if (normalized.startsWith("zh")) {
      return "zh-CN";
    }
    return "en";
  }

  function resolvePreferredLanguage() {
    try {
      const storedLanguage = localStorage.getItem(STORAGE_KEY);
      if (storedLanguage) {
        return normalizeLanguage(storedLanguage);
      }
    } catch (error) {
      console.warn("Failed to read stored language:", error);
    }

    return normalizeLanguage(
      navigator.language || navigator.userLanguage || "en",
    );
  }

  function detectPage() {
    const pathname = String(window.location.pathname || "").toLowerCase();
    if (pathname.includes("settings")) {
      return "settings";
    }
    if (pathname.includes("login")) {
      return "login";
    }
    return "";
  }

  function translateText(key, options = {}) {
    if (!window.i18next || !window.i18next.isInitialized) {
      return options.defaultValue || key;
    }
    return window.i18next.t(key, options);
  }

  function applyDataAttributeTranslations(root = document) {
    root.querySelectorAll("[data-i18n]").forEach((element) => {
      const key = element.getAttribute("data-i18n");
      if (key) {
        element.textContent = translateText(key);
      }
    });

    root.querySelectorAll("[data-i18n-placeholder]").forEach((element) => {
      const key = element.getAttribute("data-i18n-placeholder");
      if (key) {
        element.setAttribute("placeholder", translateText(key));
      }
    });

    root.querySelectorAll("[data-i18n-title]").forEach((element) => {
      const key = element.getAttribute("data-i18n-title");
      if (key) {
        element.setAttribute("title", translateText(key));
      }
    });
  }

  function applyStaticBindingTranslations(root = document) {
    const bindings = staticBindings[currentPage] || [];

    bindings.forEach((binding) => {
      const element = root.querySelector(binding.selector);
      if (!element) {
        return;
      }

      const value = translateText(binding.key);
      if (binding.attribute) {
        element.setAttribute(binding.attribute, value);
        return;
      }

      if (element.tagName === "TITLE") {
        element.textContent = value;
        return;
      }

      element.textContent = value;
    });
  }

  function applyTranslations(root = document) {
    applyStaticBindingTranslations(root);
    applyDataAttributeTranslations(root);
  }

  function handleLanguageChanged(language) {
    const normalizedLanguage = normalizeLanguage(language);

    try {
      localStorage.setItem(STORAGE_KEY, normalizedLanguage);
    } catch (error) {
      console.warn("Failed to persist language:", error);
    }

    document.documentElement.lang = normalizedLanguage;
    applyTranslations(document);

    window.dispatchEvent(
      new CustomEvent("nicecli:language-changed", {
        detail: { language: normalizedLanguage },
      }),
    );
  }

  async function initialize(pageName = "") {
    currentPage = pageName || currentPage || detectPage();

    if (!window.i18next) {
      console.warn("i18next is not available.");
      return resolvePreferredLanguage();
    }

    if (!initialized) {
      await window.i18next.init({
        lng: resolvePreferredLanguage(),
        fallbackLng: "en",
        resources,
        interpolation: {
          escapeValue: false,
        },
      });
      initialized = true;
    }

    if (!listenerRegistered) {
      window.i18next.on("languageChanged", handleLanguageChanged);
      listenerRegistered = true;
    }

    applyTranslations(document);
    handleLanguageChanged(window.i18next.language);

    return normalizeLanguage(window.i18next.language);
  }

  async function setLanguage(language) {
    const normalizedLanguage = normalizeLanguage(language);

    if (!initialized) {
      await initialize(currentPage);
    }

    if (window.i18next.language === normalizedLanguage) {
      handleLanguageChanged(normalizedLanguage);
      return normalizedLanguage;
    }

    await window.i18next.changeLanguage(normalizedLanguage);
    return normalizedLanguage;
  }

  window.NiceCLIi18n = {
    initialize,
    setLanguage,
    applyTranslations,
    t: translateText,
    getCurrentLanguage() {
      if (window.i18next && window.i18next.language) {
        return normalizeLanguage(window.i18next.language);
      }
      return resolvePreferredLanguage();
    },
    getSupportedLanguages() {
      return [...SUPPORTED_LANGUAGES];
    },
  };

  window.nicecliT = translateText;
})();
