const workspaceQuotaList = document.getElementById("workspace-quota-list");
const workspaceQuotaRefreshBtn = document.getElementById(
  "workspace-quota-refresh-btn",
);
const workspaceQuotaAccountFilter = document.getElementById(
  "workspace-quota-account-filter",
);
const workspaceQuotaWorkspaceFilter = document.getElementById(
  "workspace-quota-workspace-filter",
);
const workspaceQuotaStaleOnly = document.getElementById(
  "workspace-quota-stale-only",
);
const WORKSPACE_QUOTA_AUTO_REFRESH_MS = 10 * 60 * 1000;

let workspaceQuotaSnapshots = [];
let workspaceQuotaAuthFiles = [];
let workspaceQuotaLoadedOnce = false;
let workspaceQuotaLoading = false;
let workspaceQuotaError = "";
let workspaceQuotaAutoRefreshTimer = null;

async function loadWorkspaceQuotaSnapshots(forceRefresh = false) {
  if (!workspaceQuotaList) {
    return;
  }

  workspaceQuotaLoading = true;
  workspaceQuotaError = "";
  renderWorkspaceQuotaSnapshots();

  try {
    const shouldRefresh = forceRefresh || !workspaceQuotaLoadedOnce;
    const [quotaResult, authFilesResult] = await Promise.allSettled([
      configManager.getCodexQuotaSnapshots(shouldRefresh),
      configManager.getAuthFiles(),
    ]);
    const response =
      quotaResult.status === "fulfilled"
        ? quotaResult.value
        : { snapshots: [] };
    workspaceQuotaAuthFiles =
      authFilesResult.status === "fulfilled" &&
      Array.isArray(authFilesResult.value)
        ? authFilesResult.value
        : [];
    workspaceQuotaSnapshots = mergeWorkspaceQuotaAuthNotes(
      Array.isArray(response?.snapshots) ? response.snapshots : [],
      workspaceQuotaAuthFiles,
    );
    workspaceQuotaLoadedOnce = true;
    syncWorkspaceQuotaFilters();
  } catch (error) {
    console.error("Error loading workspace quota snapshots:", error);
    workspaceQuotaSnapshots = [];
    workspaceQuotaError =
      error?.message || nicecliT("workspaceQuota.loadFailed");
  } finally {
    workspaceQuotaLoading = false;
    renderWorkspaceQuotaSnapshots();
    updateActionButtons();
  }
}

async function refreshWorkspaceQuotaSnapshots(options = {}) {
  if (!workspaceQuotaRefreshBtn) {
    return;
  }
  if (workspaceQuotaLoading) {
    return;
  }

  const { silent = false } = options;

  if (!silent) {
    workspaceQuotaRefreshBtn.disabled = true;
    workspaceQuotaRefreshBtn.textContent = nicecliT("common.refreshing");
  }
  workspaceQuotaError = "";

  try {
    const authId = workspaceQuotaAccountFilter?.value || "";
    const workspaceId = workspaceQuotaWorkspaceFilter?.value || "";
    const [quotaResult, authFilesResult] = await Promise.allSettled([
      configManager.refreshCodexQuotaSnapshots(authId, workspaceId),
      configManager.getAuthFiles(),
    ]);
    const response =
      quotaResult.status === "fulfilled"
        ? quotaResult.value
        : { snapshots: [] };
    workspaceQuotaAuthFiles =
      authFilesResult.status === "fulfilled" &&
      Array.isArray(authFilesResult.value)
        ? authFilesResult.value
        : workspaceQuotaAuthFiles;
    workspaceQuotaSnapshots = mergeWorkspaceQuotaAuthNotes(
      Array.isArray(response?.snapshots) ? response.snapshots : [],
      workspaceQuotaAuthFiles,
    );
    workspaceQuotaLoadedOnce = true;
    syncWorkspaceQuotaFilters();
    renderWorkspaceQuotaSnapshots();
    if (!silent) {
      showSuccessMessage(nicecliT("workspaceQuota.refreshSuccess"));
    }
  } catch (error) {
    console.error("Error refreshing workspace quota snapshots:", error);
    workspaceQuotaError =
      error?.message || nicecliT("workspaceQuota.loadFailed");
    renderWorkspaceQuotaSnapshots();
    if (!silent) {
      showError(workspaceQuotaError);
    }
  } finally {
    if (!silent) {
      workspaceQuotaRefreshBtn.disabled = false;
      workspaceQuotaRefreshBtn.textContent = nicecliT("common.refresh");
    }
  }
}

function isWorkspaceQuotaTabActive() {
  const activeTab = document.querySelector(".tab.active");
  return activeTab?.getAttribute("data-tab") === "workspace-quota";
}

function stopWorkspaceQuotaAutoRefresh() {
  if (workspaceQuotaAutoRefreshTimer) {
    clearInterval(workspaceQuotaAutoRefreshTimer);
    workspaceQuotaAutoRefreshTimer = null;
  }
}

function startWorkspaceQuotaAutoRefresh() {
  if (workspaceQuotaAutoRefreshTimer) {
    return;
  }

  workspaceQuotaAutoRefreshTimer = setInterval(() => {
    if (!isWorkspaceQuotaTabActive() || document.hidden) {
      return;
    }

    refreshWorkspaceQuotaSnapshots({ silent: true }).catch((error) => {
      console.error("Error auto-refreshing workspace quota snapshots:", error);
    });
  }, WORKSPACE_QUOTA_AUTO_REFRESH_MS);
}

function setWorkspaceQuotaAutoRefreshActive(active) {
  if (active) {
    startWorkspaceQuotaAutoRefresh();
  } else {
    stopWorkspaceQuotaAutoRefresh();
  }
}

function renderWorkspaceQuotaSnapshots() {
  if (!workspaceQuotaList) {
    return;
  }

  if (workspaceQuotaLoading) {
    workspaceQuotaList.innerHTML = `
            <div class="loading-state">
                <div class="loading-spinner"></div>
                <span>${escapeWorkspaceQuotaHtml(nicecliT("workspaceQuota.loading"))}</span>
            </div>
        `;
    return;
  }

  if (workspaceQuotaError) {
    workspaceQuotaList.innerHTML = `
            <div class="empty-state workspace-quota-empty-state workspace-quota-error-state">
                <div class="empty-state-icon">&#9888;</div>
                <div class="empty-state-text">${escapeWorkspaceQuotaHtml(nicecliT("workspaceQuota.loadFailed"))}</div>
                <div class="empty-state-subtitle">${escapeWorkspaceQuotaHtml(workspaceQuotaError)}</div>
            </div>
        `;
    return;
  }

  if (
    !Array.isArray(workspaceQuotaSnapshots) ||
    workspaceQuotaSnapshots.length === 0
  ) {
    workspaceQuotaList.innerHTML = `
            <div class="empty-state workspace-quota-empty-state">
                <div class="empty-state-icon">&#128202;</div>
                <div class="empty-state-text">${escapeWorkspaceQuotaHtml(nicecliT("workspaceQuota.emptyTitle"))}</div>
                <div class="empty-state-subtitle">${escapeWorkspaceQuotaHtml(nicecliT("workspaceQuota.emptySubtitle"))}</div>
            </div>
        `;
    return;
  }

  const filtered = getFilteredWorkspaceQuotaSnapshots();
  if (filtered.length === 0) {
    workspaceQuotaList.innerHTML = `
            <div class="empty-state workspace-quota-empty-state">
                <div class="empty-state-icon">&#128269;</div>
                <div class="empty-state-text">${escapeWorkspaceQuotaHtml(nicecliT("workspaceQuota.noMatchTitle"))}</div>
                <div class="empty-state-subtitle">${escapeWorkspaceQuotaHtml(nicecliT("workspaceQuota.noMatchSubtitle"))}</div>
            </div>
        `;
    return;
  }

  const groups = groupWorkspaceQuotaSnapshots(filtered);
  const staleCount = filtered.filter((snapshot) => snapshot.stale).length;
  const errorCount = filtered.filter((snapshot) => snapshot.error).length;

  const groupMarkup = groups
    .map((group) => {
      const cards = group.snapshots.map(renderWorkspaceQuotaCard).join("");
      return `
            <section class="workspace-quota-group">
                <div class="workspace-quota-group-header">
                    <div>
                        <div class="workspace-quota-group-title">${escapeWorkspaceQuotaHtml(group.title)}</div>
                        ${group.subtitle ? `<div class="workspace-quota-group-subtitle">${escapeWorkspaceQuotaHtml(group.subtitle)}</div>` : ""}
                    </div>
                    <div class="workspace-quota-group-meta">${escapeWorkspaceQuotaHtml(nicecliT("workspaceQuota.workspaceCount", { count: group.snapshots.length }))}</div>
                </div>
                <div class="workspace-quota-card-grid">
                    ${cards}
                </div>
            </section>
        `;
    })
    .join("");

  workspaceQuotaList.innerHTML = `
        <div class="workspace-quota-summary-bar">
            <div class="workspace-quota-summary-pill">${escapeWorkspaceQuotaHtml(nicecliT("workspaceQuota.accountsSummary", { count: groups.length }))}</div>
            <div class="workspace-quota-summary-pill">${escapeWorkspaceQuotaHtml(nicecliT("workspaceQuota.snapshotsSummary", { count: filtered.length }))}</div>
            <div class="workspace-quota-summary-pill ${staleCount > 0 ? "is-stale" : ""}">${escapeWorkspaceQuotaHtml(nicecliT("workspaceQuota.staleSummary", { count: staleCount }))}</div>
            <div class="workspace-quota-summary-pill ${errorCount > 0 ? "has-error" : ""}">${escapeWorkspaceQuotaHtml(nicecliT("workspaceQuota.errorsSummary", { count: errorCount }))}</div>
        </div>
        <div class="workspace-quota-groups">
            ${groupMarkup}
        </div>
    `;
}

function renderWorkspaceQuotaCard(snapshot) {
  const workspaceName =
    snapshot.workspace_name ||
    snapshot.workspace_id ||
    nicecliT("workspaceQuota.currentWorkspace");
  const workspaceType = snapshot.workspace_type || nicecliT("common.unknown");
  const snapshotData = snapshot.snapshot || {};
  const planType =
    snapshotData.plan_type || nicecliT("workspaceQuota.unknownPlan");
  const limitName = snapshotData.limit_name || snapshotData.limit_id || "codex";
  const staleClass = snapshot.stale ? "is-stale" : "";
  const errorClass = snapshot.error ? "has-error" : "";
  const primary = formatWorkspaceQuotaWindow(
    snapshotData.primary,
    nicecliT("workspaceQuota.primary"),
  );
  const secondary = formatWorkspaceQuotaWindow(
    snapshotData.secondary,
    nicecliT("workspaceQuota.secondary"),
  );
  const credits = formatWorkspaceQuotaCredits(snapshotData.credits);
  const creditsMarkup = credits
    ? `
                <div class="workspace-quota-metric is-credits">
                    <div class="workspace-quota-metric-label">${escapeWorkspaceQuotaHtml(nicecliT("workspaceQuota.credits"))}</div>
                    <div class="workspace-quota-metric-value">${escapeWorkspaceQuotaHtml(credits.value)}</div>
                    <div class="workspace-quota-metric-subtitle">${escapeWorkspaceQuotaHtml(credits.subtitle)}</div>
                </div>
            `
    : "";

  return `
        <article class="workspace-quota-card ${staleClass} ${errorClass}">
            <div class="workspace-quota-card-header">
                <div class="workspace-quota-card-heading">
                    <div class="workspace-quota-card-title-row">
                        <h3>${escapeWorkspaceQuotaHtml(workspaceName)}</h3>
                        <span class="workspace-quota-chip">${escapeWorkspaceQuotaHtml(workspaceType)}</span>
                    </div>
                    <div class="workspace-quota-card-subtitle">
                        <span>${escapeWorkspaceQuotaHtml(nicecliT("workspaceQuota.plan", { value: planType }))}</span>
                        <span>${escapeWorkspaceQuotaHtml(nicecliT("workspaceQuota.limit", { value: limitName }))}</span>
                    </div>
                </div>
                <div class="workspace-quota-card-flags">
                    ${snapshot.stale ? `<span class="workspace-quota-flag is-stale">${escapeWorkspaceQuotaHtml(nicecliT("workspaceQuota.stale"))}</span>` : ""}
                    ${snapshot.error ? `<span class="workspace-quota-flag has-error">${escapeWorkspaceQuotaHtml(nicecliT("workspaceQuota.error"))}</span>` : ""}
                </div>
            </div>
            <div class="workspace-quota-metrics">
                ${primary}
                ${secondary}
                ${creditsMarkup}
            </div>
            ${snapshot.error ? `<div class="workspace-quota-error-banner">${escapeWorkspaceQuotaHtml(snapshot.error)}</div>` : ""}
            <div class="workspace-quota-card-footer">
                <span>${escapeWorkspaceQuotaHtml(nicecliT("workspaceQuota.source", { value: snapshot.source || nicecliT("common.unknown") }))}</span>
                <span>${escapeWorkspaceQuotaHtml(nicecliT("workspaceQuota.fetched", { value: formatWorkspaceQuotaDate(snapshot.fetched_at) }))}</span>
            </div>
        </article>
    `;
}

function getFilteredWorkspaceQuotaSnapshots() {
  const authId = workspaceQuotaAccountFilter?.value || "";
  const workspaceId = workspaceQuotaWorkspaceFilter?.value || "";
  const staleOnly = !!workspaceQuotaStaleOnly?.checked;

  return workspaceQuotaSnapshots.filter((snapshot) => {
    if (authId && snapshot.auth_id !== authId) {
      return false;
    }
    if (workspaceId && snapshot.workspace_id !== workspaceId) {
      return false;
    }
    if (staleOnly && !snapshot.stale) {
      return false;
    }
    return true;
  });
}

function groupWorkspaceQuotaSnapshots(snapshots) {
  const groups = new Map();

  snapshots.forEach((snapshot) => {
    const authId = snapshot.auth_id || "unknown-auth";
    if (!groups.has(authId)) {
      groups.set(authId, {
        authId,
        title: formatWorkspaceQuotaAuthTitle(snapshot),
        subtitle: formatWorkspaceQuotaAuthSubtitle(snapshot),
        snapshots: [],
      });
    }
    groups.get(authId).snapshots.push(snapshot);
  });

  return Array.from(groups.values()).map((group) => {
    group.snapshots.sort((left, right) => {
      const leftName = left.workspace_name || left.workspace_id || "";
      const rightName = right.workspace_name || right.workspace_id || "";
      return leftName.localeCompare(rightName);
    });
    return group;
  });
}

function syncWorkspaceQuotaFilters() {
  if (!workspaceQuotaAccountFilter || !workspaceQuotaWorkspaceFilter) {
    return;
  }

  const selectedAuthId = workspaceQuotaAccountFilter.value || "";
  const selectedWorkspaceId = workspaceQuotaWorkspaceFilter.value || "";

  const authOptions = Array.from(
    new Map(
      workspaceQuotaSnapshots.map((snapshot) => [
        snapshot.auth_id,
        {
          value: snapshot.auth_id,
          label: formatWorkspaceQuotaAuthTitle(snapshot),
        },
      ]),
    ).values(),
  ).filter((option) => option.value);

  workspaceQuotaAccountFilter.innerHTML = buildWorkspaceQuotaOptionMarkup(
    nicecliT("workspaceQuota.allAccounts"),
    authOptions,
    selectedAuthId,
  );
  if (
    selectedAuthId &&
    !authOptions.some((option) => option.value === selectedAuthId)
  ) {
    workspaceQuotaAccountFilter.value = "";
  }

  const activeAuthId = workspaceQuotaAccountFilter.value || "";
  const workspaceOptions = Array.from(
    new Map(
      workspaceQuotaSnapshots
        .filter(
          (snapshot) => !activeAuthId || snapshot.auth_id === activeAuthId,
        )
        .map((snapshot) => [
          snapshot.workspace_id,
          {
            value: snapshot.workspace_id,
            label:
              snapshot.workspace_name ||
              snapshot.workspace_id ||
              nicecliT("workspaceQuota.currentWorkspace"),
          },
        ]),
    ).values(),
  ).filter((option) => option.value);

  workspaceQuotaWorkspaceFilter.innerHTML = buildWorkspaceQuotaOptionMarkup(
    nicecliT("workspaceQuota.allWorkspaces"),
    workspaceOptions,
    selectedWorkspaceId,
  );
  if (
    selectedWorkspaceId &&
    !workspaceOptions.some((option) => option.value === selectedWorkspaceId)
  ) {
    workspaceQuotaWorkspaceFilter.value = "";
  }
}

function buildWorkspaceQuotaOptionMarkup(defaultLabel, options, selectedValue) {
  const optionMarkup = options
    .map(
      (option) => `
        <option value="${escapeWorkspaceQuotaHtml(option.value)}" ${option.value === selectedValue ? "selected" : ""}>
            ${escapeWorkspaceQuotaHtml(option.label)}
        </option>
    `,
    )
    .join("");

  return `
        <option value="">${escapeWorkspaceQuotaHtml(defaultLabel)}</option>
        ${optionMarkup}
    `;
}

function formatWorkspaceQuotaAuthTitle(snapshot) {
  const note = normalizeWorkspaceQuotaText(snapshot?.auth_note);
  const email = normalizeWorkspaceQuotaText(snapshot?.account_email);
  const authLabel = normalizeWorkspaceQuotaText(snapshot?.auth_label);
  const authId = normalizeWorkspaceQuotaText(snapshot?.auth_id);

  if (note && email) {
    return `${note}（${email}）`;
  }
  return pickFirstWorkspaceQuotaText(
    note,
    email,
    authLabel,
    authId,
    nicecliT("workspaceQuota.unknownAuth"),
  );
}

function formatWorkspaceQuotaAuthSubtitle(snapshot) {
  const note = normalizeWorkspaceQuotaText(snapshot?.auth_note);
  const email = normalizeWorkspaceQuotaText(snapshot?.account_email);
  const authLabel = normalizeWorkspaceQuotaText(snapshot?.auth_label);
  const authId = normalizeWorkspaceQuotaText(snapshot?.auth_id);
  const title = formatWorkspaceQuotaAuthTitle(snapshot);

  if (note) {
    return pickFirstWorkspaceQuotaText(
      authId && authId !== title ? authId : "",
      authLabel && authLabel !== note ? authLabel : "",
    );
  }

  if (authLabel && email && authLabel !== email) {
    return email;
  }

  if (authId && authId !== title) {
    return authId;
  }

  return "";
}

function mergeWorkspaceQuotaAuthNotes(snapshots, authFiles) {
  if (!Array.isArray(snapshots) || snapshots.length === 0) {
    return [];
  }
  if (!Array.isArray(authFiles) || authFiles.length === 0) {
    return snapshots;
  }

  const noteIndex = buildWorkspaceQuotaAuthNoteIndex(authFiles);
  return snapshots.map((snapshot) => {
    const matchedNote =
      normalizeWorkspaceQuotaText(snapshot?.auth_note) ||
      resolveWorkspaceQuotaAuthNote(snapshot, noteIndex);
    if (!matchedNote) {
      return snapshot;
    }
    return {
      ...snapshot,
      auth_note: matchedNote,
    };
  });
}

function buildWorkspaceQuotaAuthNoteIndex(authFiles) {
  const byIdentity = new Map();

  authFiles.forEach((file) => {
    const note = normalizeWorkspaceQuotaText(file?.note);
    if (!note) {
      return;
    }

    const identityValues = [file?.id, file?.name, file?.auth_id];
    identityValues.forEach((value) => {
      const normalized = normalizeWorkspaceQuotaLookupKey(value);
      if (normalized && !byIdentity.has(normalized)) {
        byIdentity.set(normalized, note);
      }
    });
  });

  return { byIdentity };
}

function resolveWorkspaceQuotaAuthNote(snapshot, noteIndex) {
  if (!snapshot || !noteIndex) {
    return "";
  }

  const identityCandidates = [snapshot.auth_id, snapshot.auth_label];
  for (const candidate of identityCandidates) {
    const normalized = normalizeWorkspaceQuotaLookupKey(candidate);
    if (!normalized) {
      continue;
    }
    const note = noteIndex.byIdentity.get(normalized);
    if (note) {
      return note;
    }
  }

  return "";
}

function formatWorkspaceQuotaWindow(windowData, label) {
  if (!windowData) {
    return `
            <div class="workspace-quota-metric">
                <div class="workspace-quota-metric-label">${label}</div>
                <div class="workspace-quota-metric-value">${escapeWorkspaceQuotaHtml(nicecliT("workspaceQuota.noData"))}</div>
                <div class="workspace-quota-metric-subtitle">${escapeWorkspaceQuotaHtml(nicecliT("workspaceQuota.noWindowSnapshot"))}</div>
            </div>
        `;
  }

  const remainingValue =
    typeof windowData.used_percent === "number"
      ? Math.max(0, Math.min(100, 100 - windowData.used_percent))
      : null;
  const remainingPercent =
    remainingValue !== null
      ? nicecliT("workspaceQuota.remaining", {
          value: remainingValue.toFixed(
            windowData.used_percent % 1 === 0 ? 0 : 1,
          ),
        })
      : nicecliT("workspaceQuota.remainingUnavailable");
  const windowLabel = windowData.window_minutes
    ? nicecliT("workspaceQuota.windowMinutes", {
        count: windowData.window_minutes,
      })
    : nicecliT("workspaceQuota.windowUnavailable");
  const resetLabel = windowData.resets_at
    ? nicecliT("workspaceQuota.resetsAt", {
        value: formatWorkspaceQuotaReset(windowData.resets_at),
      })
    : nicecliT("workspaceQuota.resetUnavailable");
  const meterMarkup =
    remainingValue !== null
      ? `
            <div class="workspace-quota-meter" aria-hidden="true">
                <div class="workspace-quota-meter-fill" style="width: ${remainingValue.toFixed(2)}%"></div>
            </div>
        `
      : "";

  return `
        <div class="workspace-quota-metric">
            <div class="workspace-quota-metric-label">${label}</div>
            <div class="workspace-quota-metric-value">${escapeWorkspaceQuotaHtml(remainingPercent)}</div>
            ${meterMarkup}
            <div class="workspace-quota-metric-subtitle">${escapeWorkspaceQuotaHtml(`${windowLabel} · ${resetLabel}`)}</div>
        </div>
    `;
}

function formatWorkspaceQuotaCredits(credits) {
  if (!credits) {
    return null;
  }

  if (credits.unlimited) {
    return {
      value: nicecliT("workspaceQuota.unlimited"),
      subtitle: nicecliT("workspaceQuota.meteredCredits"),
    };
  }

  if (
    credits.balance !== null &&
    credits.balance !== undefined &&
    credits.balance !== ""
  ) {
    return {
      value: String(credits.balance),
      subtitle: credits.has_credits
        ? nicecliT("workspaceQuota.creditsAvailable")
        : nicecliT("workspaceQuota.balanceReported"),
    };
  }

  if (!credits.has_credits) {
    return null;
  }

  return {
    value: nicecliT("workspaceQuota.available"),
    subtitle: nicecliT("workspaceQuota.creditsAttached"),
  };
}

function formatWorkspaceQuotaDate(value) {
  if (!value) {
    return nicecliT("common.unknown");
  }
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return String(value);
  }
  return date.toLocaleString();
}

function normalizeWorkspaceQuotaText(value) {
  return String(value ?? "").trim();
}

function normalizeWorkspaceQuotaLookupKey(value) {
  return normalizeWorkspaceQuotaText(value).toLowerCase();
}

function pickFirstWorkspaceQuotaText(...values) {
  for (const value of values) {
    const normalized = normalizeWorkspaceQuotaText(value);
    if (normalized) {
      return normalized;
    }
  }
  return "";
}

function formatWorkspaceQuotaReset(unixSeconds) {
  if (!unixSeconds) {
    return nicecliT("common.unknown");
  }
  const date = new Date(unixSeconds * 1000);
  if (Number.isNaN(date.getTime())) {
    return nicecliT("common.unknown");
  }
  return date.toLocaleString();
}

function escapeWorkspaceQuotaHtml(value) {
  return String(value ?? "")
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

if (workspaceQuotaRefreshBtn) {
  workspaceQuotaRefreshBtn.addEventListener(
    "click",
    refreshWorkspaceQuotaSnapshots,
  );
}

if (workspaceQuotaAccountFilter) {
  workspaceQuotaAccountFilter.addEventListener("change", () => {
    syncWorkspaceQuotaFilters();
    renderWorkspaceQuotaSnapshots();
  });
}

if (workspaceQuotaWorkspaceFilter) {
  workspaceQuotaWorkspaceFilter.addEventListener(
    "change",
    renderWorkspaceQuotaSnapshots,
  );
}

if (workspaceQuotaStaleOnly) {
  workspaceQuotaStaleOnly.addEventListener(
    "change",
    renderWorkspaceQuotaSnapshots,
  );
}

window.addEventListener("nicecli:language-changed", () => {
  syncWorkspaceQuotaFilters();
  renderWorkspaceQuotaSnapshots();
});

window.addEventListener("beforeunload", stopWorkspaceQuotaAutoRefresh);
