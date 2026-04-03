// Process state handling via Tauri events

function showProcessClosedError(message) {
  showError(message);
  setTimeout(() => {
    if (window.__TAURI__?.core?.invoke) {
      window.__TAURI__.core.invoke("open_login_window").catch((error) => {
        console.error("open_login_window failed:", error);
      });
      return;
    }
    window.location.href = "login.html";
  }, 3000);
}

if (window.__TAURI__?.event?.listen) {
  window.__TAURI__.event.listen("process-closed", (event) => {
    const data = event?.payload || {};
    console.log("CLIProxyAPI process closed:", data);
    showProcessClosedError(data.message || "CLIProxyAPI process has closed");
  });

  window.__TAURI__.event.listen("process-exit-error", (event) => {
    const errorData = event?.payload || {};
    console.error("CLIProxyAPI process exited abnormally:", errorData);
    showProcessClosedError(
      `CLIProxyAPI process exited abnormally, exit code: ${errorData.code}`,
    );
  });

  window.__TAURI__.event.listen("cliproxyapi-restarted", (event) => {
    const data = event?.payload || {};
    console.log("CLIProxyAPI process restarted successfully:", data);
    if (data.password) {
      localStorage.setItem("local-management-key", data.password);
    }
    showSuccessMessage("CLIProxyAPI process restarted successfully!");
  });
}
