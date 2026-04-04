(function () {
  const STORAGE_KEY = "nicecli-theme";
  const SUPPORTED_THEMES = ["light", "dark"];
  const DEFAULT_THEME = "dark";

  function normalizeTheme(theme) {
    return SUPPORTED_THEMES.includes(theme) ? theme : DEFAULT_THEME;
  }

  function resolveStoredTheme() {
    try {
      return normalizeTheme(localStorage.getItem(STORAGE_KEY) || DEFAULT_THEME);
    } catch (error) {
      console.warn("Failed to read stored theme:", error);
      return DEFAULT_THEME;
    }
  }

  function applyTheme(theme, persist = true) {
    const normalizedTheme = normalizeTheme(theme);
    document.documentElement.setAttribute("data-theme", normalizedTheme);

    if (persist) {
      try {
        localStorage.setItem(STORAGE_KEY, normalizedTheme);
      } catch (error) {
        console.warn("Failed to persist theme:", error);
      }
    }

    return normalizedTheme;
  }

  function setTheme(theme) {
    const appliedTheme = applyTheme(theme, true);
    window.dispatchEvent(
      new CustomEvent("nicecli:theme-changed", {
        detail: { theme: appliedTheme },
      }),
    );
    return appliedTheme;
  }

  function initializeTheme() {
    return applyTheme(resolveStoredTheme(), false);
  }

  window.NiceCLITheme = {
    initialize: initializeTheme,
    setTheme,
    getCurrentTheme() {
      return normalizeTheme(
        document.documentElement.getAttribute("data-theme") ||
          resolveStoredTheme(),
      );
    },
    getSupportedThemes() {
      return [...SUPPORTED_THEMES];
    },
  };

  initializeTheme();
})();
