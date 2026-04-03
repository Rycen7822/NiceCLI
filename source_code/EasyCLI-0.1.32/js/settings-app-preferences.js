const appThemeSelect = document.getElementById("app-theme-select");
const appLanguageSelect = document.getElementById("app-language-select");

let appPreferencesInitialized = false;

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

function initializeAppPreferencesTab() {
  if (!appThemeSelect || !appLanguageSelect) {
    return;
  }

  syncThemePreferenceSelect();
  syncLanguagePreferenceSelect();

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

  window.addEventListener("nicecli:theme-changed", syncThemePreferenceSelect);
  window.addEventListener("nicecli:language-changed", () => {
    syncThemePreferenceSelect();
    syncLanguagePreferenceSelect();
  });

  appPreferencesInitialized = true;
}
