// Tab switching, lazy loading, and initial content loading per tab

const tabs = document.querySelectorAll(".tab");
const tabContents = document.querySelectorAll(".tab-content");

const tabScriptGroups = {
  auth: [
    "js/settings-auth-codex.js",
    "js/settings-auth-claude.js",
    "js/settings-auth-gemini.js",
    "js/settings-auth-qwen.js",
    "js/settings-auth-vertex.js",
    "js/settings-auth-antigravity.js",
  ],
  "access-token": ["js/settings-access-token.js"],
  api: ["js/settings-api-keys.js"],
  openai: ["js/settings-openai.js"],
  "workspace-quota": ["js/settings-workspace-quota.js"],
};

const settingsScriptLoaders = new Map();

function loadSettingsScript(src) {
  if (settingsScriptLoaders.has(src)) {
    return settingsScriptLoaders.get(src);
  }

  const promise = new Promise((resolve, reject) => {
    const existing = document.querySelector(`script[data-settings-src="${src}"]`);
    if (existing) {
      if (existing.dataset.loaded === "true") {
        resolve();
        return;
      }

      existing.addEventListener("load", () => resolve(), { once: true });
      existing.addEventListener(
        "error",
        () => reject(new Error(`Failed to load ${src}`)),
        { once: true },
      );
      return;
    }

    const script = document.createElement("script");
    script.src = src;
    script.dataset.settingsSrc = src;
    script.addEventListener(
      "load",
      () => {
        script.dataset.loaded = "true";
        resolve();
      },
      { once: true },
    );
    script.addEventListener(
      "error",
      () => {
        settingsScriptLoaders.delete(src);
        reject(new Error(`Failed to load ${src}`));
      },
      { once: true },
    );
    document.body.appendChild(script);
  });

  settingsScriptLoaders.set(src, promise);
  return promise;
}

async function ensureTabScriptsLoaded(tabId) {
  const scripts = tabScriptGroups[tabId] || [];
  for (const src of scripts) {
    await loadSettingsScript(src);
  }
}

async function loadTabContent(tabId) {
  await ensureTabScriptsLoaded(tabId);

  if (tabId === "app-settings") {
    await initializeAppPreferencesTab();
  }
  if (tabId === "auth") {
    await loadAuthFiles();
  }
  if (tabId === "access-token") {
    await loadAccessTokenKeys();
  }
  if (tabId === "api") {
    await loadAllApiKeys();
  }
  if (tabId === "openai") {
    await loadOpenaiProviders();
  }
  if (tabId === "workspace-quota") {
    await loadWorkspaceQuotaSnapshots();
  }
}

async function activateTab(tabId) {
  tabs.forEach((tab) => {
    tab.classList.toggle("active", tab.getAttribute("data-tab") === tabId);
  });

  tabContents.forEach((content) => {
    content.classList.toggle("active", content.id === `${tabId}-content`);
  });

  await loadTabContent(tabId);

  if (typeof setWorkspaceQuotaAutoRefreshActive === "function") {
    setWorkspaceQuotaAutoRefreshActive(tabId === "workspace-quota");
  }

  updateActionButtons();
}

tabs.forEach((tab) => {
  tab.addEventListener("click", async () => {
    const tabId = tab.getAttribute("data-tab");
    try {
      await activateTab(tabId);
    } catch (error) {
      console.error(`Error loading tab ${tabId}:`, error);
      showError(error?.message || "Failed to load tab");
    }
  });
});
