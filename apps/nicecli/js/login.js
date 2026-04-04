const continueBtn = document.getElementById("continue-btn");
const proxyInput = document.getElementById("proxy-input");
const errorToast = document.getElementById("error-toast");
const successToast = document.getElementById("success-toast");

initializeLoginPreferences();
initializeLocalState();

async function initializeLoginPreferences() {
  window.NiceCLITheme?.initialize();
  if (window.NiceCLIi18n) {
    await window.NiceCLIi18n.initialize("login");
  }
}

function validateProxyUrl(proxyUrl) {
  if (!proxyUrl || proxyUrl.trim() === "") {
    return { valid: true, error: null };
  }

  const trimmedUrl = proxyUrl.trim();
  const httpProxyRegex = /^https?:\/\/[^:\s@]+:\d+$/;
  const httpProxyWithAuthRegex = /^https?:\/\/[^:\s]+:[^:\s]+@[^:\s]+:\d+$/;
  const socks5ProxyRegex = /^socks5:\/\/[^:\s@]+:\d+$/;
  const socks5WithAuthRegex = /^socks5:\/\/[^:\s]+:[^:\s]+@[^:\s]+:\d+$/;

  if (
    httpProxyRegex.test(trimmedUrl) ||
    httpProxyWithAuthRegex.test(trimmedUrl) ||
    socks5ProxyRegex.test(trimmedUrl) ||
    socks5WithAuthRegex.test(trimmedUrl)
  ) {
    return { valid: true, error: null };
  }

  return {
    valid: false,
    error:
      "Invalid proxy format. Supported formats: http://host:port, https://host:port, socks5://host:port, http://user:pass@host:port, https://user:pass@host:port, socks5://user:pass@host:port",
  };
}

function setLocalConnectionDefaults() {
  localStorage.setItem("type", "local");
  localStorage.removeItem("base-url");
  localStorage.removeItem("password");
}

function initializeLocalState() {
  setLocalConnectionDefaults();

  const proxyUrl = localStorage.getItem("proxy-url");
  if (proxyUrl) {
    proxyInput.value = proxyUrl;
  }
}

async function startLocalCliProxyAndOpenSettings() {
  try {
    const proxyUrl = proxyInput.value.trim();
    const startRes = await window.__TAURI__.core.invoke("start_cliproxyapi", {
      proxyUrl: proxyUrl || null,
    });
    if (!startRes || !startRes.success) {
      showError(nicecliT("login.processStartFailed"));
      return false;
    }

    if (startRes.password) {
      localStorage.setItem("local-management-key", startRes.password);
    }

    await window.__TAURI__.core.invoke("open_settings_window");
    return true;
  } catch (e) {
    showError(nicecliT("login.processStartError"));
    return false;
  }
}

if (window.__TAURI__?.event?.listen) {
  window.__TAURI__.event.listen("process-start-error", (event) => {
    const errorData = event?.payload || {};
    console.error("CLIProxyAPI process start failed:", errorData);
    showError(
      nicecliT("login.connectionError", {
        error: errorData.error,
      }),
    );
    if (errorData.reason) {
      showError(
        nicecliT("login.reason", {
          reason: errorData.reason,
        }),
      );
    }
  });
  window.__TAURI__.event.listen("process-exit-error", (event) => {
    const errorData = event?.payload || {};
    console.error("CLIProxyAPI process exited abnormally:", errorData);
    showError(nicecliT("login.processExited", { code: errorData.code }));
  });
}

async function handleConnectClick() {
  try {
    showSuccess(nicecliT("login.startingLocal"));
  } catch (_) {}

  const proxyUrl = proxyInput.value.trim();
  if (proxyUrl) {
    const validation = validateProxyUrl(proxyUrl);
    if (!validation.valid) {
      showError(nicecliT("login.invalidProxyFormat"));
      return;
    }
  }

  try {
    continueBtn.disabled = true;
    continueBtn.textContent = nicecliT("login.startingLocal");

    if (proxyUrl) {
      localStorage.setItem("proxy-url", proxyUrl);
    } else {
      localStorage.removeItem("proxy-url");
    }

    if (window.__TAURI__?.core?.invoke) {
      setLocalConnectionDefaults();
      await startLocalCliProxyAndOpenSettings();
    } else {
      showError(nicecliT("login.tauriRequired"));
    }
  } catch (error) {
    console.error("Error starting local runtime:", error);
    showError(
      nicecliT("login.processStartError", {
        error: error.message,
      }),
    );
  } finally {
    continueBtn.disabled = false;
    continueBtn.textContent = nicecliT("common.startLocal");
  }
}

if (continueBtn) {
  continueBtn.addEventListener("click", handleConnectClick);
}

window.__onConnect = handleConnectClick;

document.addEventListener("click", (e) => {
  const t = e.target;
  if (t && t.id === "continue-btn") {
    handleConnectClick();
  }
});

let toastQueue = [];
let isShowingToast = false;

function showError(message) {
  addToQueue("error", message);
}

function showSuccess(message) {
  addToQueue("success", message);
}

function addToQueue(type, message) {
  toastQueue.push({ type, message });
  if (!isShowingToast) {
    showNextToast();
  }
}

function showNextToast() {
  if (toastQueue.length === 0) {
    isShowingToast = false;
    return;
  }

  isShowingToast = true;
  const { type, message } = toastQueue.shift();
  const toast = type === "error" ? errorToast : successToast;

  toast.textContent = message;
  toast.classList.add("show");

  setTimeout(() => {
    toast.classList.remove("show");
    setTimeout(() => {
      showNextToast();
    }, 300);
  }, 3000);
}
