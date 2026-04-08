const appThemeSelect = document.getElementById("app-theme-select");
const appLanguageSelect = document.getElementById("app-language-select");
const autoStartSwitch = document.getElementById("auto-start-switch");
const autoLoginSwitch = document.getElementById("auto-login-switch");
const silentStartupSwitch = document.getElementById("silent-startup-switch");

let appPreferencesInitialized = false;
let currentAutoStartEnabled = false;
let currentStartupPreferences = {
  autoLogin: false,
  silentStartup: false,
};

function buildAppPreferenceOptions(selectElement, options, selectedValue) {
  if (!selectElement) {
    return;
  }

  selectElement.innerHTML = options
    .map(
      (option) => `
        <option value="${option.value}" ${option.value === selectedValue ? "selected" : ""}>
          ${option.label}
        </option>
      `,
    )
    .join("");
}

function syncThemePreferenceSelect() {
  if (!appThemeSelect || !window.NiceCLITheme) {
    return;
  }

  const currentTheme = window.NiceCLITheme.getCurrentTheme();
  buildAppPreferenceOptions(
    appThemeSelect,
    [
      { value: "light", label: nicecliT("appSettings.light") },
      { value: "dark", label: nicecliT("appSettings.dark") },
    ],
    currentTheme,
  );
}

function syncLanguagePreferenceSelect() {
  if (!appLanguageSelect || !window.NiceCLIi18n) {
    return;
  }

  const currentLanguage = window.NiceCLIi18n.getCurrentLanguage();
  buildAppPreferenceOptions(
    appLanguageSelect,
    [
      { value: "en", label: nicecliT("appSettings.english") },
      { value: "zh-CN", label: nicecliT("appSettings.simplifiedChinese") },
    ],
    currentLanguage,
  );
}

function applyStartupPreferences(preferences = {}) {
  currentStartupPreferences = {
    autoLogin: preferences.autoLogin || false,
    silentStartup: preferences.silentStartup || false,
  };

  if (autoLoginSwitch) {
    autoLoginSwitch.checked = currentStartupPreferences.autoLogin;
  }
  if (silentStartupSwitch) {
    silentStartupSwitch.checked = currentStartupPreferences.silentStartup;
  }
}

function setStartupPreferenceControlsDisabled(disabled) {
  if (autoLoginSwitch) {
    autoLoginSwitch.disabled = disabled;
  }
  if (silentStartupSwitch) {
    silentStartupSwitch.disabled = disabled;
  }
}

async function syncAutoStartPreference() {
  if (!autoStartSwitch) {
    return;
  }

  try {
    if (window.__TAURI__?.core?.invoke) {
      const result = await window.__TAURI__.core.invoke(
        "check_auto_start_enabled",
      );
      currentAutoStartEnabled = result.enabled || false;
    } else {
      currentAutoStartEnabled = false;
    }
  } catch (error) {
    console.error("Error checking auto-start status:", error);
    currentAutoStartEnabled = false;
  }

  autoStartSwitch.checked = currentAutoStartEnabled;
}

async function syncStartupPreferences() {
  try {
    if (window.__TAURI__?.core?.invoke) {
      const preferences = await window.__TAURI__.core.invoke(
        "read_startup_preferences",
      );
      applyStartupPreferences(preferences);
      return;
    }
  } catch (error) {
    console.error("Error loading startup preferences:", error);
  }

  applyStartupPreferences();
}

async function persistStartupPreferences() {
  const nextPreferences = {
    autoLogin: autoLoginSwitch?.checked || false,
    silentStartup: silentStartupSwitch?.checked || false,
  };

  setStartupPreferenceControlsDisabled(true);
  try {
    if (!window.__TAURI__?.core?.invoke) {
      throw new Error("Tauri invoke is unavailable");
    }

    const savedPreferences = await window.__TAURI__.core.invoke(
      "update_startup_preferences",
      {
        preferences: nextPreferences,
      },
    );
    applyStartupPreferences(savedPreferences);
    showSuccessMessage(nicecliT("toasts.startupPreferencesUpdated"));
  } catch (error) {
    console.error("Error saving startup preferences:", error);
    applyStartupPreferences(currentStartupPreferences);
    showError(nicecliT("toasts.startupPreferencesUpdateFailed"));
  } finally {
    setStartupPreferenceControlsDisabled(false);
  }
}

async function handleAutoStartChange() {
  if (!autoStartSwitch) {
    return;
  }

  const nextEnabled = autoStartSwitch.checked;
  autoStartSwitch.disabled = true;

  try {
    if (!window.__TAURI__?.core?.invoke) {
      throw new Error("Tauri invoke is unavailable");
    }

    const result = await window.__TAURI__.core.invoke(
      nextEnabled ? "enable_auto_start" : "disable_auto_start",
    );
    if (!result?.success) {
      throw new Error("Auto-start command failed");
    }

    currentAutoStartEnabled = nextEnabled;
    showSuccessMessage(
      nicecliT(
        nextEnabled ? "toasts.autoStartEnabled" : "toasts.autoStartDisabled",
      ),
    );
  } catch (error) {
    console.error("Error toggling auto-start:", error);
    autoStartSwitch.checked = currentAutoStartEnabled;
    showError(nicecliT("toasts.autoStartUpdateFailed"));
  } finally {
    autoStartSwitch.disabled = false;
  }
}

async function initializeAppPreferencesTab() {
  if (!appThemeSelect || !appLanguageSelect) {
    return;
  }

  syncThemePreferenceSelect();
  syncLanguagePreferenceSelect();
  await Promise.all([syncAutoStartPreference(), syncStartupPreferences()]);

  if (appPreferencesInitialized) {
    return;
  }

  appThemeSelect.addEventListener("change", () => {
    window.NiceCLITheme?.setTheme(appThemeSelect.value);
    syncThemePreferenceSelect();
    showSuccessMessage(nicecliT("toasts.themeUpdated"));
  });

  appLanguageSelect.addEventListener("change", async () => {
    await window.NiceCLIi18n?.setLanguage(appLanguageSelect.value);
    syncThemePreferenceSelect();
    syncLanguagePreferenceSelect();
    showSuccessMessage(nicecliT("toasts.languageUpdated"));
  });

  autoStartSwitch?.addEventListener("change", handleAutoStartChange);
  autoLoginSwitch?.addEventListener("change", persistStartupPreferences);
  silentStartupSwitch?.addEventListener("change", persistStartupPreferences);

  window.addEventListener("nicecli:theme-changed", syncThemePreferenceSelect);
  window.addEventListener("nicecli:language-changed", () => {
    syncThemePreferenceSelect();
    syncLanguagePreferenceSelect();
  });

  appPreferencesInitialized = true;
}
