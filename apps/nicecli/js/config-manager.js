/**
 * Configuration Manager Abstraction Layer
 * Local-only operation interface for NiceCLI
 */
class ConfigManager {
  constructor() {
    this.type = "local";
    this.baseUrl = null;
    this.password = null;
  }

  /**
   * Save multiple files as a ZIP with a Save As dialog when possible
   * @param {Array<{name:string, content:string|Uint8Array|ArrayBuffer}>} files
   * @param {string} suggestedName
   * @returns {Promise<Object>} result
   */
  async saveFilesAsZip(files, suggestedName = "auth-files.zip") {
    try {
      if (!Array.isArray(files) || files.length === 0) {
        return { success: false, error: "No files to save" };
      }
      if (typeof window.__zipFiles !== "function") {
        // Missing ZIP util
        return { success: false, error: "ZIP utility not loaded" };
      }
      const blob = window.__zipFiles(files);
      if (typeof window.showSaveFilePicker === "function") {
        try {
          const handle = await window.showSaveFilePicker({
            suggestedName,
            types: [
              {
                description: "ZIP archive",
                accept: { "application/zip": [".zip"] },
              },
            ],
          });
          const writable = await handle.createWritable();
          await writable.write(blob);
          await writable.close();
        } catch (e) {
          if (e && e.name === "AbortError") {
            return { success: false, error: "User cancelled save dialog" };
          }
          // Fallback to anchor download
          const url = URL.createObjectURL(blob);
          const a = document.createElement("a");
          a.href = url;
          a.download = suggestedName;
          document.body.appendChild(a);
          a.click();
          document.body.removeChild(a);
          URL.revokeObjectURL(url);
        }
      } else {
        // Fallback: anchor download (default Downloads folder or per-browser settings)
        const url = URL.createObjectURL(blob);
        const a = document.createElement("a");
        a.href = url;
        a.download = suggestedName;
        document.body.appendChild(a);
        a.click();
        document.body.removeChild(a);
        URL.revokeObjectURL(url);
      }
      return { success: true, successCount: files.length, errorCount: 0 };
    } catch (error) {
      console.error("saveFilesAsZip error:", error);
      return { success: false, error: error?.message || String(error) };
    }
  }

  /**
   * Get current configuration
   * @returns {Promise<Object>} Configuration object
   */
  async getConfig() {
    this.refreshConnection();
    return this.getLocalConfig();
  }

  /**
   * Update configuration item
   * @param {string} endpoint - Configuration item path
   * @param {any} value - Configuration value
   * @param {boolean} isDelete - Whether to delete configuration item
   * @returns {Promise<boolean>} Whether operation was successful
   */
  async updateSetting(endpoint, value, isDelete = false) {
    return this.updateLocalSetting(endpoint, value, isDelete);
  }

  /**
   * Get API key configuration
   * @param {string} keyType - Key type (gemini, codex, claude, openai)
   * @returns {Promise<Array>} Key array
   */
  async getApiKeys(keyType) {
    return this.getLocalApiKeys(keyType);
  }

  /**
   * Update API key configuration
   * @param {string} keyType - Key type
   * @param {Array} keys - Key array
   * @returns {Promise<boolean>} Whether operation was successful
   */
  async updateApiKeys(keyType, keys) {
    return this.updateLocalApiKeys(keyType, keys);
  }

  /**
   * Get authentication file list
   * @returns {Promise<Array>} File list
   */
  async getAuthFiles() {
    return this.getLocalAuthFiles();
  }

  /**
   * Get Codex workspace quota snapshots
   * @param {boolean} refresh - Whether to force refresh before listing
   * @returns {Promise<Object>} Quota snapshot response
   */
  async getCodexQuotaSnapshots(refresh = false) {
    return this.getLocalCodexQuotaSnapshots(refresh);
  }

  /**
   * Refresh Codex workspace quota snapshots
   * @param {string} authId - Optional auth filter
   * @param {string} workspaceId - Optional workspace filter
   * @returns {Promise<Object>} Quota snapshot response
   */
  async refreshCodexQuotaSnapshots(authId = "", workspaceId = "") {
    return this.refreshLocalCodexQuotaSnapshots(authId, workspaceId);
  }

  /**
   * Upload authentication files
   * @param {File|Array<File>} files - Files to upload
   * @returns {Promise<Object>} Upload result
   */
  async uploadAuthFiles(files) {
    return this.uploadLocalAuthFiles(files);
  }

  /**
   * Delete authentication files
   * @param {string|Array<string>} filenames - Filenames to delete
   * @returns {Promise<Object>} Delete result
   */
  async deleteAuthFiles(filenames) {
    return this.deleteLocalAuthFiles(filenames);
  }

  /**
   * Download authentication files
   * @param {string|Array<string>} filenames - Filenames to download
   * @returns {Promise<Object>} Download result
   */
  async downloadAuthFiles(filenames) {
    return this.downloadLocalAuthFiles(filenames);
  }

  /**
   * Update editable authentication file fields
   * @param {string} name - Auth file name or ID
   * @param {Object} fields - Editable fields to update
   * @returns {Promise<Object>} Update result
   */
  async updateAuthFileFields(name, fields = {}) {
    return this.updateLocalAuthFileFields(name, fields);
  }

  /**
   * Save Gemini Web tokens
   * @param {string} secure1psid - Secure-1PSID cookie value
   * @param {string} secure1psidts - Secure-1PSIDTS cookie value
   * @param {string} email - Email address (used as label)
   * @returns {Promise<Object>} Save result
   */
  async saveGeminiWebTokens(secure1psid, secure1psidts, email) {
    return this.saveLocalGeminiWebTokens(secure1psid, secure1psidts, email);
  }

  /**
   * Import Vertex credential using service account JSON
   * @param {File} file - Service account JSON file
   * @param {string} location - Vertex location, defaults to us-central1
   * @returns {Promise<Object>} Import result
   */
  async importVertexCredential(file, location = "us-central1") {
    if (!file) {
      return { success: false, error: "No file selected" };
    }
    return this.importLocalVertexCredential(file, location);
  }

  // ==================== Local Mode Implementation ====================

  /**
   * Get local configuration
   * @returns {Promise<Object>} Configuration object
   */
  async getLocalConfig() {
    try {
      if (window.__TAURI__?.core?.invoke) {
        const config = await window.__TAURI__.core.invoke("read_config_yaml");
        return config || {};
      }
      const configStr = localStorage.getItem("config");
      return configStr ? JSON.parse(configStr) : {};
    } catch (error) {
      console.error("Error reading local config:", error);
      return {};
    }
  }

  /**
   * Update local configuration item
   * @param {string} endpoint - Configuration item path
   * @param {any} value - Configuration value
   * @param {boolean} isDelete - Whether to delete configuration item
   * @returns {Promise<boolean>} Whether operation was successful
   */
  async updateLocalSetting(endpoint, value, isDelete = false) {
    try {
      if (window.__TAURI__?.core?.invoke) {
        const result = await window.__TAURI__.core.invoke(
          "update_config_yaml",
          {
            endpoint,
            value,
            is_delete: isDelete,
          },
        );
        return !!(result && result.success);
      }
      // Fallback to localStorage (testing only)
      const configStr = localStorage.getItem("config");
      const config = configStr ? JSON.parse(configStr) : {};
      const key = endpoint.split("/").pop();
      if (isDelete) {
        delete config[key];
      } else {
        config[key] = value;
      }
      localStorage.setItem("config", JSON.stringify(config));
      return true;
    } catch (error) {
      console.error("Error updating local setting:", error);
      return false;
    }
  }

  /**
   * Get local API keys
   * @param {string} keyType - Key type
   * @returns {Promise<Array>} Key array
   */
  async getLocalApiKeys(keyType) {
    try {
      const config = await this.getLocalConfig();

      const keyMap = {
        gemini: "gemini-api-key",
        codex: "codex-api-key",
        claude: "claude-api-key",
        openai: "openai-compatibility",
        "access-token": "api-keys",
      };

      const key = keyMap[keyType];
      if (!key) {
        throw new Error(`Unknown key type: ${keyType}`);
      }

      const keys = config[key] || [];
      return Array.isArray(keys) ? keys : [];
    } catch (error) {
      console.error(`Error getting local ${keyType} keys:`, error);
      return [];
    }
  }

  /**
   * Update local API keys
   * @param {string} keyType - Key type
   * @param {Array} keys - Key array
   * @returns {Promise<boolean>} Whether operation was successful
   */
  async updateLocalApiKeys(keyType, keys) {
    try {
      const keyMap = {
        gemini: "gemini-api-key",
        codex: "codex-api-key",
        claude: "claude-api-key",
        openai: "openai-compatibility",
        "access-token": "api-keys",
      };

      const endpoint = keyMap[keyType];
      if (!endpoint) {
        throw new Error(`Unknown key type: ${keyType}`);
      }

      return await this.updateLocalSetting(endpoint, keys);
    } catch (error) {
      console.error(`Error updating local ${keyType} keys:`, error);
      return false;
    }
  }

  /**
   * Get local authentication file list
   * @returns {Promise<Array>} File list
   */
  async getLocalAuthFiles() {
    try {
      const { baseUrl, password } =
        await this.getLocalManagementConnectionInfo();
      return await this.getManagementAuthFiles(baseUrl, {
        "X-Management-Key": password,
      });
    } catch (error) {
      console.error("Error reading local auth files:", error);
      return [];
    }
  }

  /**
   * Resolve Local mode management API connection details
   * @returns {Promise<{baseUrl: string, password: string}>} Connection details
   */
  async getLocalManagementConnectionInfo() {
    const config = await this.getLocalConfig();
    const port = config.port || 8317;
    const password = localStorage.getItem("local-management-key") || "";

    if (!password) {
      throw new Error(
        "Missing local management key. Please restart the local runtime.",
      );
    }

    return {
      baseUrl: `http://127.0.0.1:${port}`,
      password,
    };
  }

  /**
   * Get local Codex quota snapshots through the local management API
   * @param {boolean} refresh - Whether to force refresh
   * @returns {Promise<Object>} Quota snapshot response
   */
  async getLocalCodexQuotaSnapshots(refresh = false) {
    try {
      const { baseUrl, password } =
        await this.getLocalManagementConnectionInfo();
      const apiUrl = new URL(
        baseUrl.endsWith("/")
          ? `${baseUrl}v0/management/codex/quota-snapshots`
          : `${baseUrl}/v0/management/codex/quota-snapshots`,
      );
      if (refresh) {
        apiUrl.searchParams.set("refresh", "1");
      }

      const response = await fetch(apiUrl.toString(), {
        method: "GET",
        headers: {
          "X-Management-Key": password,
          "Content-Type": "application/json",
        },
      });

      const data = await response.json().catch(() => ({}));
      if (!response.ok) {
        throw new Error(
          data.error || `HTTP ${response.status}: ${response.statusText}`,
        );
      }
      return {
        provider: data.provider || "codex",
        snapshots: Array.isArray(data.snapshots) ? data.snapshots : [],
      };
    } catch (error) {
      console.error("Error getting local Codex quota snapshots:", error);
      throw error;
    }
  }

  /**
   * Refresh local Codex quota snapshots through the local management API
   * @param {string} authId - Optional auth filter
   * @param {string} workspaceId - Optional workspace filter
   * @returns {Promise<Object>} Quota snapshot response
   */
  async refreshLocalCodexQuotaSnapshots(authId = "", workspaceId = "") {
    try {
      const { baseUrl, password } =
        await this.getLocalManagementConnectionInfo();
      const apiUrl = baseUrl.endsWith("/")
        ? `${baseUrl}v0/management/codex/quota-snapshots/refresh`
        : `${baseUrl}/v0/management/codex/quota-snapshots/refresh`;

      const response = await fetch(apiUrl, {
        method: "POST",
        headers: {
          "X-Management-Key": password,
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          auth_id: authId || "",
          workspace_id: workspaceId || "",
        }),
      });

      const data = await response.json().catch(() => ({}));
      if (!response.ok) {
        throw new Error(
          data.error || `HTTP ${response.status}: ${response.statusText}`,
        );
      }
      return {
        provider: data.provider || "codex",
        snapshots: Array.isArray(data.snapshots) ? data.snapshots : [],
      };
    } catch (error) {
      console.error("Error refreshing local Codex quota snapshots:", error);
      throw error;
    }
  }

  /**
   * Upload local authentication files
   * @param {File|Array<File>} files - Files to upload
   * @returns {Promise<Object>} Upload result
   */
  async uploadLocalAuthFiles(files) {
    try {
      const { baseUrl, password } =
        await this.getLocalManagementConnectionInfo();
      return await this.uploadAuthFilesViaManagement(
        baseUrl,
        { "X-Management-Key": password },
        files,
      );
    } catch (error) {
      console.error("Error uploading local auth files:", error);
      return {
        success: false,
        error: error.message,
      };
    }
  }

  /**
   * Read file content as text
   * @param {File} file - File object
   * @returns {Promise<string>} File content
   */
  readFileAsText(file) {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = (e) => resolve(e.target.result);
      reader.onerror = (e) => reject(new Error("File read failed"));
      reader.readAsText(file);
    });
  }

  /**
   * Delete local authentication files
   * @param {string|Array<string>} filenames - Filenames to delete
   * @returns {Promise<Object>} Delete result
   */
  async deleteLocalAuthFiles(filenames) {
    try {
      const { baseUrl, password } =
        await this.getLocalManagementConnectionInfo();
      return await this.deleteAuthFilesViaManagement(
        baseUrl,
        { "X-Management-Key": password },
        filenames,
      );
    } catch (error) {
      console.error("Error deleting local auth files:", error);
      return {
        success: false,
        error: error.message,
      };
    }
  }

  /**
   * Download local authentication files
   * @param {string|Array<string>} filenames - Filenames to download
   * @returns {Promise<Object>} Download result
   */
  async downloadLocalAuthFiles(filenames) {
    try {
      const { baseUrl, password } =
        await this.getLocalManagementConnectionInfo();
      return await this.downloadAuthFilesViaManagement(
        baseUrl,
        { "X-Management-Key": password },
        filenames,
      );
    } catch (error) {
      if (error && error.name === "AbortError") {
        return {
          success: false,
          error: "User cancelled directory selection",
        };
      }
      console.error("Error downloading local auth files:", error);
      return {
        success: false,
        error: error?.message || String(error),
      };
    }
  }

  /**
   * Update local authentication file fields
   * @param {string} name - Auth file name or ID
   * @param {Object} fields - Editable fields to update
   * @returns {Promise<Object>} Update result
   */
  async updateLocalAuthFileFields(name, fields = {}) {
    try {
      const { baseUrl, password } =
        await this.getLocalManagementConnectionInfo();
      return await this.patchAuthFileFieldsViaManagement(
        baseUrl,
        { "X-Management-Key": password },
        name,
        fields,
      );
    } catch (error) {
      console.error("Error updating local auth file fields:", error);
      return {
        success: false,
        error: error?.message || String(error),
      };
    }
  }

  /**
   * Save local Gemini Web tokens
   * @param {string} secure1psid - Secure-1PSID cookie value
   * @param {string} secure1psidts - Secure-1PSIDTS cookie value
   * @param {string} email - Email address (used as label)
   * @returns {Promise<Object>} Save result
   */
  async saveLocalGeminiWebTokens(secure1psid, secure1psidts, email) {
    try {
      const config = await this.getLocalConfig();
      const port = config.port || 8317;
      const baseUrl = `http://127.0.0.1:${port}`;

      // In local mode, use the random password from localStorage (set during local runtime startup)
      const password = localStorage.getItem("local-management-key") || "";

      if (!password) {
        throw new Error(
          "Missing local management key. Please restart the local runtime.",
        );
      }

      const apiUrl = baseUrl.endsWith("/")
        ? `${baseUrl}v0/management/gemini-web-token`
        : `${baseUrl}/v0/management/gemini-web-token`;

      const response = await fetch(apiUrl, {
        method: "POST",
        headers: {
          "X-Management-Key": password,
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          secure_1psid: secure1psid,
          secure_1psidts: secure1psidts,
          label: email,
        }),
      });

      if (response.ok) {
        const data = await response.json();
        return {
          success: true,
          file: data.file,
        };
      } else {
        const errorData = await response.json().catch(() => ({}));
        return {
          success: false,
          error:
            errorData.error ||
            `HTTP ${response.status}: ${response.statusText}`,
        };
      }
    } catch (error) {
      console.error("Error saving local Gemini Web tokens:", error);
      return {
        success: false,
        error: error.message,
      };
    }
  }

  /**
   * Import Vertex credential in Local mode
   * @param {File} file - Service account JSON file
   * @param {string} location - Vertex location
   * @returns {Promise<Object>} Import result
   */
  async importLocalVertexCredential(file, location = "us-central1") {
    try {
      const config = await this.getLocalConfig();
      const port = config.port || 8317;
      const baseUrl = `http://127.0.0.1:${port}`;
      const password = localStorage.getItem("local-management-key") || "";

      if (!password) {
        throw new Error(
          "Missing local management key. Please restart the local runtime.",
        );
      }

      const apiUrl = baseUrl.endsWith("/")
        ? `${baseUrl}v0/management/vertex/import`
        : `${baseUrl}/v0/management/vertex/import`;

      const formData = new FormData();
      formData.append("file", file);
      const normalizedLocation =
        location && location.trim() ? location.trim() : "us-central1";
      formData.append("location", normalizedLocation);

      const response = await fetch(apiUrl, {
        method: "POST",
        headers: {
          "X-Management-Key": password,
        },
        body: formData,
      });

      const data = await response.json().catch(() => ({}));

      if (response.ok) {
        return { success: true, data };
      }

      return {
        success: false,
        error: data.error || `HTTP ${response.status}: ${response.statusText}`,
      };
    } catch (error) {
      console.error("Error importing local Vertex credential:", error);
      return {
        success: false,
        error: error.message,
      };
    }
  }

  /**
   * Upload single file
   * @param {File} file - File to upload
   * @param {string} apiUrl - API URL
   * @returns {Promise<void>}
   */
  async uploadSingleFile(file, apiUrl, authHeaders = {}) {
    const formData = new FormData();
    formData.append("file", file);

    const response = await fetch(apiUrl, {
      method: "POST",
      headers: { ...authHeaders },
      body: formData,
    });

    if (!response.ok) {
      let errorMessage = "Upload failed";

      if (response.status === 401) {
        errorMessage = "Authentication failed";
      } else if (response.status === 403) {
        errorMessage = "Access denied";
      } else if (response.status === 413) {
        errorMessage = "File too large";
      } else if (response.status >= 500) {
        errorMessage = "Server error";
      }

      throw new Error(errorMessage);
    }
  }

  /**
   * Download single file to directory
   * @param {string} filename - Filename
   * @param {FileSystemDirectoryHandle} directoryHandle - Directory handle
   * @returns {Promise<void>}
   */
  async downloadSingleFile(
    filename,
    directoryHandle,
    apiUrl,
    authHeaders = {},
  ) {
    const response = await fetch(apiUrl, {
      method: "GET",
      headers: { ...authHeaders },
    });

    if (!response.ok) {
      throw new Error(`HTTP ${response.status}: ${response.statusText}`);
    }

    // Get file content
    const fileContent = await response.blob();

    // Create file in directory
    const fileHandle = await directoryHandle.getFileHandle(filename, {
      create: true,
    });
    const writable = await fileHandle.createWritable();
    await writable.write(fileContent);
    await writable.close();
  }

  buildManagementApiUrl(baseUrl, path) {
    return baseUrl.endsWith("/") ? `${baseUrl}${path}` : `${baseUrl}/${path}`;
  }

  async getManagementAuthFiles(baseUrl, authHeaders) {
    const apiUrl = this.buildManagementApiUrl(
      baseUrl,
      "v0/management/auth-files",
    );
    const response = await fetch(apiUrl, {
      method: "GET",
      headers: {
        ...authHeaders,
        "Content-Type": "application/json",
      },
    });

    const data = await response.json().catch(() => ({}));
    if (!response.ok) {
      throw new Error(
        data.error || `HTTP ${response.status}: ${response.statusText}`,
      );
    }

    return Array.isArray(data.files) ? data.files : [];
  }

  async uploadAuthFilesViaManagement(baseUrl, authHeaders, files) {
    const apiUrl = this.buildManagementApiUrl(
      baseUrl,
      "v0/management/auth-files",
    );
    const fileArray = Array.isArray(files) ? files : [files];
    let successCount = 0;
    let errorCount = 0;
    const errors = [];

    for (const file of fileArray) {
      try {
        await this.uploadSingleFile(file, apiUrl, authHeaders);
        successCount++;
      } catch (error) {
        console.error(`Error uploading ${file.name}:`, error);
        errorCount++;
        errors.push(`${file.name}: ${error.message}`);
      }
    }

    return {
      success: successCount > 0,
      successCount,
      errorCount,
      errors: errors.length > 0 ? errors : undefined,
    };
  }

  async deleteAuthFilesViaManagement(baseUrl, authHeaders, filenames) {
    const filenameArray = Array.isArray(filenames) ? filenames : [filenames];
    let successCount = 0;
    let errorCount = 0;

    for (const filename of filenameArray) {
      try {
        const apiUrl = `${this.buildManagementApiUrl(baseUrl, "v0/management/auth-files")}?name=${encodeURIComponent(filename)}`;
        const response = await fetch(apiUrl, {
          method: "DELETE",
          headers: { ...authHeaders },
        });

        if (response.ok) {
          successCount++;
        } else {
          errorCount++;
        }
      } catch (error) {
        console.error(`Error deleting ${filename}:`, error);
        errorCount++;
      }
    }

    return {
      success: successCount > 0,
      successCount,
      errorCount,
    };
  }

  async downloadAuthFilesViaManagement(baseUrl, authHeaders, filenames) {
    const filenameArray = Array.isArray(filenames) ? filenames : [filenames];
    let successCount = 0;
    let errorCount = 0;

    if (window.__TAURI__?.core?.invoke) {
      try {
        const files = [];
        for (const filename of filenameArray) {
          const apiUrl = `${this.buildManagementApiUrl(baseUrl, "v0/management/auth-files/download")}?name=${encodeURIComponent(filename)}`;
          const response = await fetch(apiUrl, {
            method: "GET",
            headers: { ...authHeaders },
          });
          if (!response.ok) {
            throw new Error(`HTTP ${response.status}: ${response.statusText}`);
          }
          const content = await response.text();
          files.push({ name: filename, content });
        }
        return await window.__TAURI__.core.invoke("save_files_to_directory", {
          files,
        });
      } catch (error) {
        if (String(error).includes("User cancelled directory selection")) {
          return {
            success: false,
            error: "User cancelled directory selection",
          };
        }
        console.error(
          "Tauri save_files_to_directory fallback failed; trying browser options:",
          error,
        );
      }
    }

    if (typeof window.showDirectoryPicker === "function") {
      try {
        const directoryHandle = await window.showDirectoryPicker({
          mode: "readwrite",
        });

        if (!directoryHandle) {
          return {
            success: false,
            error: "User cancelled directory selection",
          };
        }

        for (const filename of filenameArray) {
          try {
            const apiUrl = `${this.buildManagementApiUrl(baseUrl, "v0/management/auth-files/download")}?name=${encodeURIComponent(filename)}`;
            await this.downloadSingleFile(
              filename,
              directoryHandle,
              apiUrl,
              authHeaders,
            );
            successCount++;
          } catch (error) {
            console.error(`Error downloading ${filename}:`, error);
            errorCount++;
          }
        }

        return {
          success: successCount > 0,
          successCount,
          errorCount,
        };
      } catch (error) {
        if (error.name === "AbortError") {
          return {
            success: false,
            error: "User cancelled directory selection",
          };
        }
        console.error(
          "Directory picker unavailable or failed; falling back:",
          error,
        );
      }
    }

    const files = [];
    for (const filename of filenameArray) {
      try {
        const apiUrl = `${this.buildManagementApiUrl(baseUrl, "v0/management/auth-files/download")}?name=${encodeURIComponent(filename)}`;
        const response = await fetch(apiUrl, {
          method: "GET",
          headers: { ...authHeaders },
        });
        if (!response.ok) {
          throw new Error(`HTTP ${response.status}: ${response.statusText}`);
        }
        const content = await response.text();
        files.push({ name: filename, content });
      } catch (error) {
        console.error(`Error downloading ${filename}:`, error);
        errorCount++;
      }
    }

    if (files.length > 0) {
      const zipName = `auth-files-${new Date().toISOString().replace(/[:T]/g, "-").split(".")[0]}.zip`;
      const res = await this.saveFilesAsZip(files, zipName);
      if (res.success) {
        return { success: true, successCount: files.length, errorCount };
      }
      return {
        success: false,
        error: res.error || "Failed to save ZIP",
        errorCount,
      };
    }

    return { success: false, error: "No file downloaded", errorCount };
  }

  async patchAuthFileFieldsViaManagement(
    baseUrl,
    authHeaders,
    name,
    fields = {},
  ) {
    const trimmedName = String(name || "").trim();
    if (!trimmedName) {
      return { success: false, error: "Auth file name is required" };
    }

    const payload = { name: trimmedName };
    for (const [key, value] of Object.entries(fields || {})) {
      if (value !== undefined) {
        payload[key] = value;
      }
    }

    const apiUrl = this.buildManagementApiUrl(
      baseUrl,
      "v0/management/auth-files/fields",
    );
    const response = await fetch(apiUrl, {
      method: "PATCH",
      headers: {
        ...authHeaders,
        "Content-Type": "application/json",
      },
      body: JSON.stringify(payload),
    });

    const data = await response.json().catch(() => ({}));
    if (!response.ok) {
      return {
        success: false,
        error: data.error || `HTTP ${response.status}: ${response.statusText}`,
      };
    }

    return { success: true, data };
  }

  /**
   * Refresh connection information
   */
  refreshConnection() {
    localStorage.setItem("type", "local");
    localStorage.removeItem("base-url");
    localStorage.removeItem("password");
    this.type = "local";
    this.baseUrl = null;
    this.password = null;
    localStorage.removeItem("config");
  }
}

// Create global instance
window.configManager = new ConfigManager();
