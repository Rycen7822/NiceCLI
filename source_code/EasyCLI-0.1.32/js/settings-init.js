// Page initialization after DOM is ready

async function initializeSettingsPreferences() {
  window.NiceCLITheme?.initialize();
  if (window.NiceCLIi18n) {
    await window.NiceCLIi18n.initialize("settings");
  }
  initializeAppPreferencesTab();
}

function handleSettingsLanguageChange() {
  updateServerStatus();
  updateActionButtons();

  if (typeof renderAuthFiles === "function" && Array.isArray(authFiles)) {
    renderAuthFiles();
  }
  if (
    typeof renderWorkspaceQuotaSnapshots === "function" &&
    Array.isArray(workspaceQuotaSnapshots)
  ) {
    syncWorkspaceQuotaFilters();
    renderWorkspaceQuotaSnapshots();
  }
}

window.addEventListener(
  "nicecli:language-changed",
  handleSettingsLanguageChange,
);

document.addEventListener("DOMContentLoaded", async () => {
  try {
    await initializeSettingsPreferences();
    const currentConfig = await getCurrentConfig();
    originalConfig = currentConfig;
    await initializeDebugSwitch();
    await initializePort();
    await initializeProxyUrl();
    await initializeAdditionalSettings();
    await initializeAutoStart();
    toggleLocalOnlyFields();
    updateServerStatus();
    updateActionButtons();

    const currentTabEl = document.querySelector(".tab.active");
    const currentTab = currentTabEl
      ? currentTabEl.getAttribute("data-tab")
      : "basic";
    if (typeof loadTabContent === "function") {
      await loadTabContent(currentTab);
    }
    if (typeof setWorkspaceQuotaAutoRefreshActive === "function") {
      setWorkspaceQuotaAutoRefreshActive(currentTab === "workspace-quota");
    }
  } catch (error) {
    console.error("Error initializing settings:", error);
    showError(nicecliT("toasts.failedToLoadSettings"));
  }
});

// Stop workspace quota polling when page is unloaded
window.addEventListener("beforeunload", () => {
  if (typeof stopWorkspaceQuotaAutoRefresh === "function") {
    stopWorkspaceQuotaAutoRefresh();
  }
});
