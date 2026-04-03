// Authentication files management: list, selection, upload/download, and actions

// Elements
const selectAllBtn = document.getElementById("select-all-btn");
const deleteBtn = document.getElementById("delete-btn");
const authFilesList = document.getElementById("auth-files-list");
const authLoading = document.getElementById("auth-loading");

// New dropdown elements
const newDropdown = document.getElementById("new-dropdown");
const newBtn = document.getElementById("new-btn");
const dropdownMenu = document.getElementById("dropdown-menu");
const downloadBtn = document.getElementById("download-btn");

// State
let selectedAuthFiles = new Set();
let authFiles = [];

// Load auth files from server
async function loadAuthFiles() {
  try {
    authFiles = await configManager.getAuthFiles();
    const activeNames = new Set(authFiles.map((file) => file.name));
    selectedAuthFiles = new Set(
      Array.from(selectedAuthFiles).filter((name) => activeNames.has(name)),
    );
    renderAuthFiles();
    updateActionButtons();
  } catch (error) {
    console.error("Error loading auth files:", error);
    showError(nicecliT("toasts.networkError"));
    showEmptyAuthFiles();
    updateActionButtons();
  }
}

// Render auth files list
function renderAuthFiles() {
  authLoading.style.display = "none";
  if (authFiles.length === 0) {
    showEmptyAuthFiles();
    return;
  }
  authFilesList.innerHTML = "";
  authFiles.forEach((file) => {
    const fileItem = document.createElement("div");
    fileItem.className = "auth-file-item";
    fileItem.dataset.filename = file.name;
    if (selectedAuthFiles.has(file.name)) {
      fileItem.classList.add("selected");
    }

    const fileSize = formatFileSize(file.size);
    const modTime = formatDate(file.modtime);
    const note = normalizeAuthFileText(file.note);
    const email = normalizeAuthFileText(file.email);
    const noteButtonLabel = note
      ? nicecliT("authFiles.editNote")
      : nicecliT("authFiles.addNote");
    const emailMarkup = email
      ? `<span class="auth-file-email">${escapeAuthFileHtml(
          nicecliT("authFiles.emailPrefix", { email }),
        )}</span>`
      : "";
    const noteMarkup = note
      ? escapeAuthFileHtml(note)
      : `<span class="auth-file-note-empty">${escapeAuthFileHtml(
          nicecliT("authFiles.noNote"),
        )}</span>`;

    fileItem.innerHTML = `
            <div class="auth-file-info">
                <div class="auth-file-title-row">
                    <div class="auth-file-name">${escapeAuthFileHtml(file.name)}</div>
                    ${note ? `<span class="auth-file-note-badge">${escapeAuthFileHtml(nicecliT("authFiles.remarkBadge"))}</span>` : ""}
                </div>
                <div class="auth-file-note">${escapeAuthFileHtml(nicecliT("authFiles.notePrefix"))}${noteMarkup}</div>
                <div class="auth-file-details">
                    ${emailMarkup}
                    <span class="auth-file-type">Type: ${escapeAuthFileHtml(file.type || "unknown")}</span>
                    <span class="auth-file-size">${escapeAuthFileHtml(fileSize)}</span>
                    <span>Modified: ${escapeAuthFileHtml(modTime)}</span>
                </div>
            </div>
            <div class="auth-file-actions">
                <button type="button" class="auth-file-note-btn">${noteButtonLabel}</button>
            </div>
        `;

    const noteBtn = fileItem.querySelector(".auth-file-note-btn");
    noteBtn.addEventListener("click", (event) => {
      event.stopPropagation();
      openAuthFileNoteDialog(file);
    });
    fileItem.addEventListener("click", () =>
      toggleAuthFileSelection(file.name, fileItem),
    );
    authFilesList.appendChild(fileItem);
  });
}

// Empty state for auth files
function showEmptyAuthFiles() {
  authLoading.style.display = "none";
  authFilesList.innerHTML = `
        <div class="empty-state">
            <div class="empty-state-icon">&#128193;</div>
            <div class="empty-state-text">${escapeAuthFileHtml(nicecliT("authFiles.emptyTitle"))}</div>
            <div class="empty-state-subtitle">${escapeAuthFileHtml(nicecliT("authFiles.emptySubtitle"))}</div>
        </div>
  `;
  updateActionButtons();
}

function normalizeAuthFileText(value) {
  return String(value ?? "").trim();
}

function escapeAuthFileHtml(value) {
  return String(value ?? "")
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

// Toggle selection of an auth file
function toggleAuthFileSelection(filename, fileItem) {
  if (selectedAuthFiles.has(filename)) {
    selectedAuthFiles.delete(filename);
    fileItem.classList.remove("selected");
  } else {
    selectedAuthFiles.add(filename);
    fileItem.classList.add("selected");
  }
  updateActionButtons();
}

// Update action buttons based on current tab/state
function updateActionButtons() {
  const hasSelection = selectedAuthFiles.size > 0;
  const allSelected =
    selectedAuthFiles.size === authFiles.length && authFiles.length > 0;
  const currentTab = document
    .querySelector(".tab.active")
    .getAttribute("data-tab");
  if (currentTab === "auth") {
    resetBtn.style.display = "none";
    applyBtn.style.display = "none";
    selectAllBtn.style.display = "block";
    deleteBtn.style.display = "block";
    newDropdown.style.display = "block";
    downloadBtn.style.display = "block";
    selectAllBtn.textContent = allSelected
      ? nicecliT("common.unselectAll")
      : nicecliT("common.selectAll");
    deleteBtn.disabled = !hasSelection;
    downloadBtn.disabled = !hasSelection;
  } else if (currentTab === "app-settings") {
    resetBtn.style.display = "none";
    applyBtn.style.display = "none";
    selectAllBtn.style.display = "none";
    deleteBtn.style.display = "none";
    newDropdown.style.display = "none";
    downloadBtn.style.display = "none";
  } else if (
    currentTab === "access-token" ||
    currentTab === "api" ||
    currentTab === "openai" ||
    currentTab === "basic"
  ) {
    resetBtn.style.display = "block";
    applyBtn.style.display = "block";
    selectAllBtn.style.display = "none";
    deleteBtn.style.display = "none";
    newDropdown.style.display = "none";
    downloadBtn.style.display = "none";
  } else if (currentTab === "workspace-quota") {
    resetBtn.style.display = "none";
    applyBtn.style.display = "none";
    selectAllBtn.style.display = "none";
    deleteBtn.style.display = "none";
    newDropdown.style.display = "none";
    downloadBtn.style.display = "none";
  }

  resetBtn.textContent = nicecliT("common.reset");
  applyBtn.textContent = nicecliT("common.apply");
  newBtn.textContent = nicecliT("common.new");
  downloadBtn.textContent = nicecliT("common.download");
  deleteBtn.textContent = nicecliT("common.delete");
}

// Toggle select all auth files
function toggleSelectAllAuthFiles() {
  const allSelected = selectedAuthFiles.size === authFiles.length;
  if (allSelected) {
    selectedAuthFiles.clear();
    document
      .querySelectorAll(".auth-file-item")
      .forEach((item) => item.classList.remove("selected"));
  } else {
    selectedAuthFiles.clear();
    authFiles.forEach((file) => selectedAuthFiles.add(file.name));
    document
      .querySelectorAll(".auth-file-item")
      .forEach((item) => item.classList.add("selected"));
  }
  updateActionButtons();
}

// Delete selected auth files
async function deleteSelectedAuthFiles() {
  if (selectedAuthFiles.size === 0 || deleteBtn.disabled) return;
  const fileCount = selectedAuthFiles.size;
  const fileText =
    fileCount === 1
      ? nicecliT("authFiles.fileSingular")
      : nicecliT("authFiles.filePlural");
  showConfirmDialog(
    nicecliT("authFiles.deleteConfirmTitle"),
    nicecliT("authFiles.deleteConfirmMessage", {
      count: fileCount,
      fileLabel: fileText,
    }),
    async () => {
      deleteBtn.disabled = true;
      deleteBtn.textContent = nicecliT("authFiles.deleteInProgress");
      try {
        const result = await configManager.deleteAuthFiles(
          Array.from(selectedAuthFiles),
        );
        if (result.success) {
          showSuccessMessage(
            nicecliT("authFiles.deleteSuccess", {
              count: result.successCount,
            }),
          );
          selectedAuthFiles.clear();
          await loadAuthFiles();
        } else {
          if (result.error) {
            showError(result.error);
          } else {
            showError(
              nicecliT("authFiles.deleteFailed", {
                count: result.errorCount,
              }),
            );
          }
        }
      } catch (error) {
        console.error("Error deleting auth files:", error);
        showError(nicecliT("toasts.networkError"));
      } finally {
        deleteBtn.disabled = false;
        deleteBtn.textContent = nicecliT("common.delete");
        updateActionButtons();
      }
    },
  );
}

// Toggle dropdown menu visibility
function toggleDropdown() {
  dropdownMenu.classList.toggle("show");
}

// Close dropdown menu
function closeDropdown() {
  dropdownMenu.classList.remove("show");
}

// Create a new auth file by type
function createNewAuthFile(type) {
  const typeNames = {
    gemini: nicecliT("authFiles.newTypes.gemini"),
    "gemini-web": nicecliT("authFiles.newTypes.geminiWeb"),
    claude: nicecliT("authFiles.newTypes.claude"),
    codex: nicecliT("authFiles.newTypes.codex"),
    qwen: nicecliT("authFiles.newTypes.qwen"),
    vertex: nicecliT("authFiles.newTypes.vertex"),
    iflow: nicecliT("authFiles.newTypes.iflow"),
    antigravity: nicecliT("authFiles.newTypes.antigravity"),
    local: nicecliT("authFiles.newTypes.local"),
  };

  if (type === "local") {
    uploadLocalFile();
  } else if (type === "codex") {
    startCodexAuthFlow();
  } else if (type === "claude") {
    startClaudeAuthFlow();
  } else if (type === "gemini") {
    showGeminiProjectIdDialog();
  } else if (type === "gemini-web") {
    showGeminiWebDialog();
  } else if (type === "qwen") {
    startQwenAuthFlow();
  } else if (type === "vertex") {
    showVertexImportDialog();
  } else if (type === "antigravity") {
    startAntigravityAuthFlow();
  } else if (type === "iflow") {
    startIFlowCookieFlow();
  } else {
    console.log(`Creating new ${typeNames[type]} auth file`);
    showSuccessMessage(`Creating new ${typeNames[type]} auth file...`);
  }
}

function handleAuthFileNoteEscapeKey(event) {
  if (event.key === "Escape") {
    closeAuthFileNoteDialog();
  }
}

function closeAuthFileNoteDialog() {
  document.removeEventListener("keydown", handleAuthFileNoteEscapeKey);
  const modal = document.getElementById("auth-file-note-modal");
  if (modal) {
    modal.remove();
  }
}

function openAuthFileNoteDialog(file) {
  closeAuthFileNoteDialog();

  const modal = document.createElement("div");
  modal.className = "modal show";
  modal.id = "auth-file-note-modal";
  modal.innerHTML = `
        <div class="modal-content">
            <div class="modal-header">
                <h3 class="modal-title">${escapeAuthFileHtml(nicecliT("authFiles.noteDialogTitle"))}</h3>
                <button class="modal-close" id="auth-file-note-modal-close">&times;</button>
            </div>
            <div class="modal-body">
                <div class="form-group">
                    <label for="auth-file-note-name">${escapeAuthFileHtml(nicecliT("authFiles.authFileLabel"))}</label>
                    <input type="text" id="auth-file-note-name" class="form-input" disabled>
                </div>
                <div class="form-group">
                    <label for="auth-file-note-email">${escapeAuthFileHtml(nicecliT("authFiles.emailLabel"))}</label>
                    <input type="text" id="auth-file-note-email" class="form-input" disabled>
                </div>
                <div class="form-group">
                    <label for="auth-file-note-input">${escapeAuthFileHtml(nicecliT("authFiles.remarkLabel"))}</label>
                    <textarea id="auth-file-note-input" class="form-input" rows="4" placeholder="${escapeAuthFileHtml(nicecliT("authFiles.remarkPlaceholder"))}"></textarea>
                </div>
                <div class="auth-actions">
                    <button type="button" id="auth-file-note-save-btn" class="btn-primary">${escapeAuthFileHtml(nicecliT("common.save"))}</button>
                    <button type="button" id="auth-file-note-cancel-btn" class="btn-cancel">${escapeAuthFileHtml(nicecliT("common.cancel"))}</button>
                </div>
            </div>
        </div>
    `;
  modal.addEventListener("click", (event) => {
    if (event.target === modal) {
      closeAuthFileNoteDialog();
    }
  });
  document.body.appendChild(modal);

  const nameInput = document.getElementById("auth-file-note-name");
  const emailInput = document.getElementById("auth-file-note-email");
  const noteInput = document.getElementById("auth-file-note-input");
  nameInput.value = normalizeAuthFileText(file.name);
  emailInput.value =
    normalizeAuthFileText(file.email) || nicecliT("common.unavailable");
  noteInput.value = normalizeAuthFileText(file.note);

  document
    .getElementById("auth-file-note-modal-close")
    .addEventListener("click", closeAuthFileNoteDialog);
  document
    .getElementById("auth-file-note-cancel-btn")
    .addEventListener("click", closeAuthFileNoteDialog);
  document
    .getElementById("auth-file-note-save-btn")
    .addEventListener("click", () => saveAuthFileNote(file));
  document.addEventListener("keydown", handleAuthFileNoteEscapeKey);
  noteInput.focus();
  noteInput.select();
}

async function saveAuthFileNote(file) {
  const noteInput = document.getElementById("auth-file-note-input");
  const saveBtn = document.getElementById("auth-file-note-save-btn");
  const cancelBtn = document.getElementById("auth-file-note-cancel-btn");
  if (!noteInput || !saveBtn || !cancelBtn) {
    return;
  }

  const note = noteInput.value.trim();
  saveBtn.disabled = true;
  cancelBtn.disabled = true;
  saveBtn.textContent = nicecliT("authFiles.saveInProgress");

  try {
    const result = await configManager.updateAuthFileFields(
      file.id || file.name,
      { note },
    );
    if (!result.success) {
      showError(result.error || nicecliT("authFiles.remarkUpdateFailed"));
      return;
    }

    closeAuthFileNoteDialog();
    showSuccessMessage(
      note
        ? nicecliT("authFiles.remarkUpdated")
        : nicecliT("authFiles.remarkCleared"),
    );
    await loadAuthFiles();
  } catch (error) {
    console.error("Error updating auth file note:", error);
    showError(nicecliT("authFiles.remarkUpdateFailed"));
  } finally {
    if (saveBtn) {
      saveBtn.disabled = false;
      saveBtn.textContent = nicecliT("common.save");
    }
    if (cancelBtn) {
      cancelBtn.disabled = false;
    }
  }
}

// Show Gemini Web dialog
function showGeminiWebDialog() {
  const modal = document.createElement("div");
  modal.className = "modal show";
  modal.id = "gemini-web-modal";
  modal.innerHTML = `
        <div class="modal-content">
            <div class="modal-header">
                <h3 class="modal-title">${escapeAuthFileHtml(nicecliT("authFiles.geminiWebTitle"))}</h3>
                <button class="modal-close" id="gemini-web-modal-close">&times;</button>
            </div>
            <div class="modal-body">
                <div class="codex-auth-content">
                    <p>${escapeAuthFileHtml(nicecliT("authFiles.geminiWebDescription"))}</p>
                    <div class="form-group">
                        <label for="gemini-web-secure-1psid-input">${escapeAuthFileHtml(nicecliT("authFiles.secure1psid"))}</label>
                        <input type="text" id="gemini-web-secure-1psid-input" class="form-input" placeholder="Enter Secure-1PSID">
                    </div>
                    <div class="form-group">
                        <label for="gemini-web-secure-1psidts-input">${escapeAuthFileHtml(nicecliT("authFiles.secure1psidts"))}</label>
                        <input type="text" id="gemini-web-secure-1psidts-input" class="form-input" placeholder="Enter Secure-1PSIDTS">
                    </div>
                    <div class="form-group">
                        <label for="gemini-web-email-input" style="text-align: left;">${escapeAuthFileHtml(nicecliT("authFiles.emailLabel"))}:</label>
                        <input type="email" id="gemini-web-email-input" class="form-input" placeholder="Enter your email address">
                    </div>
                    <div class="auth-actions">
                        <button type="button" id="gemini-web-confirm-btn" class="btn-primary">${escapeAuthFileHtml(nicecliT("authFiles.confirm"))}</button>
                        <button type="button" id="gemini-web-cancel-btn" class="btn-cancel">${escapeAuthFileHtml(nicecliT("common.cancel"))}</button>
                    </div>
                </div>
            </div>
        </div>`;
  document.body.appendChild(modal);
  document
    .getElementById("gemini-web-modal-close")
    .addEventListener("click", cancelGeminiWebDialog);
  document
    .getElementById("gemini-web-confirm-btn")
    .addEventListener("click", confirmGeminiWebTokens);
  document
    .getElementById("gemini-web-cancel-btn")
    .addEventListener("click", cancelGeminiWebDialog);
  document.addEventListener("keydown", handleGeminiWebEscapeKey);
  document.getElementById("gemini-web-secure-1psid-input").focus();
}

// Handle Gemini Web dialog escape key
function handleGeminiWebEscapeKey(e) {
  if (e.key === "Escape") {
    cancelGeminiWebDialog();
  }
}

// Cancel Gemini Web dialog
function cancelGeminiWebDialog() {
  document.removeEventListener("keydown", handleGeminiWebEscapeKey);
  const modal = document.getElementById("gemini-web-modal");
  if (modal) modal.remove();
}

// Confirm Gemini Web tokens
async function confirmGeminiWebTokens() {
  try {
    const emailInput = document.getElementById("gemini-web-email-input");
    const secure1psidInput = document.getElementById(
      "gemini-web-secure-1psid-input",
    );
    const secure1psidtsInput = document.getElementById(
      "gemini-web-secure-1psidts-input",
    );

    const email = emailInput.value.trim();
    const secure1psid = secure1psidInput.value.trim();
    const secure1psidts = secure1psidtsInput.value.trim();

    if (!email || !secure1psid || !secure1psidts) {
      showError(nicecliT("authFiles.enterGeminiTokens"));
      return;
    }

    cancelGeminiWebDialog();

    // Call Management API to save Gemini Web tokens
    const result = await configManager.saveGeminiWebTokens(
      secure1psid,
      secure1psidts,
      email,
    );

    if (result.success) {
      showSuccessMessage(nicecliT("authFiles.geminiSaveSuccess"));
      // Refresh the auth files list
      await loadAuthFiles();
    } else {
      showError(
        nicecliT("authFiles.geminiSaveFailed", {
          error: result.error || nicecliT("common.unknown"),
        }),
      );
    }
  } catch (error) {
    console.error("Error saving Gemini Web tokens:", error);
    showError(
      nicecliT("authFiles.geminiSaveFailed", {
        error: error.message,
      }),
    );
  }
}

// Upload local JSON files
function uploadLocalFile() {
  const fileInput = document.createElement("input");
  fileInput.type = "file";
  fileInput.accept = ".json";
  fileInput.multiple = true;
  fileInput.style.display = "none";
  document.body.appendChild(fileInput);
  fileInput.click();
  fileInput.addEventListener("change", async (event) => {
    const files = Array.from(event.target.files);
    if (files.length === 0) {
      document.body.removeChild(fileInput);
      return;
    }
    const invalidFiles = files.filter(
      (file) => !file.name.toLowerCase().endsWith(".json"),
    );
    if (invalidFiles.length > 0) {
      showError(
        `Please select only JSON files. Invalid files: ${invalidFiles.map((f) => f.name).join(", ")}`,
      );
      document.body.removeChild(fileInput);
      return;
    }
    try {
      await uploadFilesToServer(files);
      await loadAuthFiles();
    } catch (error) {
      console.error("Error uploading files:", error);
      showError(nicecliT("authFiles.uploadFailed"));
    } finally {
      document.body.removeChild(fileInput);
    }
  });
}

// Upload multiple files via config manager
async function uploadFilesToServer(files) {
  try {
    const result = await configManager.uploadAuthFiles(files);
    if (result.success && result.successCount > 0) {
      showSuccessMessage(
        nicecliT("authFiles.uploadSuccess", {
          count: result.successCount,
        }),
      );
    }
    if (result.errorCount > 0) {
      const errorMessage =
        result.errors && result.errors.length <= 3
          ? `Failed to upload ${result.errorCount} file(s): ${result.errors.join(", ")}`
          : `Failed to upload ${result.errorCount} file(s)`;
      showError(errorMessage);
    }
    if (result.error) {
      showError(result.error);
    }
  } catch (error) {
    console.error("Error uploading files:", error);
    showError(nicecliT("authFiles.uploadFailed"));
  }
}

// Download selected auth files
async function downloadSelectedAuthFiles() {
  if (selectedAuthFiles.size === 0 || downloadBtn.disabled) return;
  downloadBtn.disabled = true;
  downloadBtn.textContent = `${nicecliT("common.download")}...`;
  try {
    const result = await configManager.downloadAuthFiles(
      Array.from(selectedAuthFiles),
    );
    if (result.success && result.successCount > 0) {
      showSuccessMessage(
        nicecliT("authFiles.downloadSuccess", {
          count: result.successCount,
        }),
      );
    }
    if (result.errorCount > 0) {
      showError(
        nicecliT("authFiles.downloadFailed", {
          count: result.errorCount,
        }),
      );
    }
    if (result.error) {
      showError(result.error);
    }
  } catch (error) {
    console.error("Error downloading files:", error);
    showError(
      nicecliT("authFiles.downloadFailed", {
        count: 0,
      }),
    );
  } finally {
    downloadBtn.disabled = false;
    downloadBtn.textContent = nicecliT("common.download");
  }
}

// Event wiring for auth files UI
selectAllBtn.addEventListener("click", toggleSelectAllAuthFiles);
deleteBtn.addEventListener("click", deleteSelectedAuthFiles);
downloadBtn.addEventListener("click", downloadSelectedAuthFiles);

newBtn.addEventListener("click", (e) => {
  e.stopPropagation();
  toggleDropdown();
});

document.querySelectorAll(".dropdown-item").forEach((item) => {
  item.addEventListener("click", (e) => {
    e.stopPropagation();
    const type = item.getAttribute("data-type");
    createNewAuthFile(type);
    closeDropdown();
  });
});

document.addEventListener("click", (e) => {
  if (!newDropdown.contains(e.target)) {
    closeDropdown();
  }
});
