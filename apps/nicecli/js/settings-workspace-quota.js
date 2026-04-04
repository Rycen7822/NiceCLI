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
const workspaceQuotaDescription = document.querySelector(
  ".workspace-quota-description",
);
const WORKSPACE_QUOTA_AUTO_REFRESH_MS = 10 * 60 * 1000;
const WORKSPACE_QUOTA_COUNTDOWN_REFRESH_MS = 60 * 1000;

let workspaceQuotaSnapshots = [];
let workspaceQuotaAuthFiles = [];
let workspaceQuotaLoadedOnce = false;
let workspaceQuotaLoading = false;
let workspaceQuotaError = "";
let workspaceQuotaAutoRefreshTimer = null;
let workspaceQuotaCountdownTimer = null;

function invalidateWorkspaceQuotaSnapshotsCache() {
  workspaceQuotaLoadedOnce = false;
  window.__nicecliWorkspaceQuotaDirty = true;
}

window.invalidateWorkspaceQuotaSnapshotsCache =
  invalidateWorkspaceQuotaSnapshotsCache;

async function loadWorkspaceQuotaSnapshots(forceRefresh = false) {
  if (!workspaceQuotaList) {
    return;
  }

  workspaceQuotaLoading = true;
  workspaceQuotaError = "";
  renderWorkspaceQuotaSnapshots();

  try {
    const shouldRefresh =
      forceRefresh ||
      window.__nicecliWorkspaceQuotaDirty === true ||
      !workspaceQuotaLoadedOnce;
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
    window.__nicecliWorkspaceQuotaDirty = false;
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
    const accountKey = workspaceQuotaAccountFilter?.value || "";
    const workspaceKey = workspaceQuotaWorkspaceFilter?.value || "";
    const refreshTargets = getWorkspaceQuotaRefreshTargets(accountKey, workspaceKey);
    const [refreshResults, authFilesResult] = await Promise.all([
      Promise.allSettled(
        refreshTargets.map((target) =>
          configManager.refreshCodexQuotaSnapshots(
            target.authId,
            target.workspaceId,
          ),
        ),
      ),
      configManager.getAuthFiles(),
    ]);
    const firstRefreshError = refreshResults.find(
      (result) => result.status === "rejected",
    );
    const hasRefreshSuccess = refreshResults.some(
      (result) => result.status === "fulfilled",
    );
    if (!hasRefreshSuccess && firstRefreshError?.status === "rejected") {
      throw firstRefreshError.reason;
    }
    const response = await configManager.getCodexQuotaSnapshots(false);
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
    window.__nicecliWorkspaceQuotaDirty = false;
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

function stopWorkspaceQuotaCountdownRefresh() {
  if (workspaceQuotaCountdownTimer) {
    clearInterval(workspaceQuotaCountdownTimer);
    workspaceQuotaCountdownTimer = null;
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

function startWorkspaceQuotaCountdownRefresh() {
  if (workspaceQuotaCountdownTimer) {
    return;
  }

  workspaceQuotaCountdownTimer = setInterval(() => {
    if (
      !isWorkspaceQuotaTabActive() ||
      document.hidden ||
      workspaceQuotaLoading ||
      workspaceQuotaError
    ) {
      return;
    }

    renderWorkspaceQuotaSnapshots();
  }, WORKSPACE_QUOTA_COUNTDOWN_REFRESH_MS);
}

function setWorkspaceQuotaAutoRefreshActive(active) {
  if (active) {
    startWorkspaceQuotaAutoRefresh();
    startWorkspaceQuotaCountdownRefresh();
    renderWorkspaceQuotaSnapshots();
  } else {
    stopWorkspaceQuotaAutoRefresh();
    stopWorkspaceQuotaCountdownRefresh();
  }
}

function renderWorkspaceQuotaSnapshots() {
  updateWorkspaceQuotaDescription();

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
  const workspaceName = formatWorkspaceQuotaWorkspaceTitle(snapshot);
  const showWorkspaceTitle = shouldDisplayWorkspaceQuotaWorkspaceTitle(snapshot);
  const snapshotData = snapshot.snapshot || {};
  const planLabel = formatWorkspaceQuotaPlanLabel(snapshot);
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
                    ${showWorkspaceTitle ? `
                    <div class="workspace-quota-card-title-row">
                        <h3>${escapeWorkspaceQuotaHtml(workspaceName)}</h3>
                    </div>
                    ` : ""}
                    <div class="workspace-quota-card-subtitle">
                        <span>${escapeWorkspaceQuotaHtml(planLabel)}</span>
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
        </article>
    `;
}

function getFilteredWorkspaceQuotaSnapshots() {
  const accountKey = workspaceQuotaAccountFilter?.value || "";
  const workspaceKey = workspaceQuotaWorkspaceFilter?.value || "";
  const staleOnly = !!workspaceQuotaStaleOnly?.checked;

  return workspaceQuotaSnapshots.filter((snapshot) => {
    if (accountKey && getWorkspaceQuotaAccountKey(snapshot) !== accountKey) {
      return false;
    }
    if (
      workspaceKey &&
      getWorkspaceQuotaWorkspaceFilterKey(snapshot) !== workspaceKey
    ) {
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
    const accountKey = getWorkspaceQuotaAccountKey(snapshot) || "unknown-auth";
    if (!groups.has(accountKey)) {
      groups.set(accountKey, {
        accountKey,
        title: formatWorkspaceQuotaAuthTitle(snapshot),
        subtitle: formatWorkspaceQuotaAuthSubtitle(snapshot),
        snapshots: [],
      });
    }
    groups.get(accountKey).snapshots.push(snapshot);
  });

  return Array.from(groups.values()).map((group) => {
    group.title = formatWorkspaceQuotaAuthTitle(group.snapshots[0]);
    group.subtitle = formatWorkspaceQuotaGroupSubtitle(group.snapshots);
    group.snapshots.sort((left, right) => {
      const leftName = formatWorkspaceQuotaWorkspaceTitle(left);
      const rightName = formatWorkspaceQuotaWorkspaceTitle(right);
      return leftName.localeCompare(rightName);
    });
    return group;
  });
}

function syncWorkspaceQuotaFilters() {
  if (!workspaceQuotaAccountFilter || !workspaceQuotaWorkspaceFilter) {
    return;
  }

  const selectedAccountKey = workspaceQuotaAccountFilter.value || "";
  const selectedWorkspaceId = workspaceQuotaWorkspaceFilter.value || "";

  const authOptions = Array.from(
    new Map(
      workspaceQuotaSnapshots.map((snapshot) => [
        getWorkspaceQuotaAccountKey(snapshot),
        {
          value: getWorkspaceQuotaAccountKey(snapshot),
          label: formatWorkspaceQuotaAuthTitle(snapshot),
        },
      ]),
    ).values(),
  )
    .filter((option) => option.value)
    .sort((left, right) => left.label.localeCompare(right.label));

  workspaceQuotaAccountFilter.innerHTML = buildWorkspaceQuotaOptionMarkup(
    nicecliT("workspaceQuota.allAccounts"),
    authOptions,
    selectedAccountKey,
  );
  if (
    selectedAccountKey &&
    !authOptions.some((option) => option.value === selectedAccountKey)
  ) {
    workspaceQuotaAccountFilter.value = "";
  }

  const activeAccountKey = workspaceQuotaAccountFilter.value || "";
  const workspaceOptionSnapshots = new Map();
  workspaceQuotaSnapshots
    .filter(
      (snapshot) =>
        !activeAccountKey ||
        getWorkspaceQuotaAccountKey(snapshot) === activeAccountKey,
    )
    .forEach((snapshot) => {
      const key = getWorkspaceQuotaWorkspaceFilterKey(snapshot);
      if (!key) {
        return;
      }
      if (!workspaceOptionSnapshots.has(key)) {
        workspaceOptionSnapshots.set(key, []);
      }
      workspaceOptionSnapshots.get(key).push(snapshot);
    });

  const workspaceOptions = Array.from(workspaceOptionSnapshots.entries())
    .map(([value, snapshots]) => ({
      value,
      label: formatWorkspaceQuotaWorkspaceFilterLabel(
        snapshots,
        !activeAccountKey,
      ),
    }))
    .filter((option) => option.value)
    .sort((left, right) => left.label.localeCompare(right.label));

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
  const email = getWorkspaceQuotaAccountEmail(snapshot);
  const authLabel = normalizeWorkspaceQuotaText(snapshot?.auth_label);
  const authId = normalizeWorkspaceQuotaText(snapshot?.auth_id);

  return pickFirstWorkspaceQuotaText(
    email,
    authLabel,
    authId,
    nicecliT("workspaceQuota.unknownAuth"),
  );
}

function formatWorkspaceQuotaAuthSubtitle(snapshot) {
  const email = getWorkspaceQuotaAccountEmail(snapshot);
  const authLabel = normalizeWorkspaceQuotaText(snapshot?.auth_label);
  const authId = normalizeWorkspaceQuotaText(snapshot?.auth_id);
  const title = formatWorkspaceQuotaAuthTitle(snapshot);

  if (authLabel && email && authLabel !== email) {
    return authLabel;
  }

  if (authId && authId !== title) {
    return authId;
  }

  return "";
}

function formatWorkspaceQuotaGroupSubtitle(snapshots) {
  if (!Array.isArray(snapshots) || snapshots.length !== 1) {
    return "";
  }
  return formatWorkspaceQuotaAuthSubtitle(snapshots[0]);
}

function mergeWorkspaceQuotaAuthNotes(snapshots, authFiles) {
  if (!Array.isArray(snapshots) || snapshots.length === 0) {
    return [];
  }
  const noteIndex = buildWorkspaceQuotaAuthNoteIndex(authFiles || []);
  return snapshots.map((snapshot) => {
    const matchedMetadata = resolveWorkspaceQuotaAuthMetadata(snapshot, noteIndex);
    const matchedNote =
      normalizeWorkspaceQuotaText(snapshot?.auth_note) ||
      matchedMetadata.note;
    const matchedEmail =
      normalizeWorkspaceQuotaText(snapshot?.account_email) ||
      matchedMetadata.email;
    const matchedFileName =
      normalizeWorkspaceQuotaText(snapshot?.auth_file_name) ||
      matchedMetadata.fileName;
    const matchedPlan =
      normalizeWorkspaceQuotaPlanTier(snapshot?.account_plan) ||
      matchedMetadata.plan;
    if (!matchedNote && !matchedEmail && !matchedFileName && !matchedPlan) {
      return {
        ...snapshot,
        account_email: getWorkspaceQuotaAccountEmail(snapshot),
      };
    }
    return {
      ...snapshot,
      auth_note: matchedNote || snapshot?.auth_note || "",
      account_email: matchedEmail || snapshot?.account_email || "",
      auth_file_name: matchedFileName || snapshot?.auth_file_name || "",
      account_plan: matchedPlan || snapshot?.account_plan || "",
    };
  });
}

function buildWorkspaceQuotaAuthNoteIndex(authFiles) {
  const byIdentity = new Map();

  authFiles.forEach((file) => {
    const note = normalizeWorkspaceQuotaText(file?.note);
    const email = resolveWorkspaceQuotaAuthFileEmail(file);
    const fileName = resolveWorkspaceQuotaAuthFileName(file);
    const plan = extractWorkspaceQuotaPlanFromFilename(fileName);
    if (!note && !email && !fileName && !plan) {
      return;
    }

    const identityValues = [file?.id, file?.name, file?.auth_id];
    identityValues.forEach((value) => {
      const normalized = normalizeWorkspaceQuotaLookupKey(value);
      if (normalized && !byIdentity.has(normalized)) {
        byIdentity.set(normalized, { note, email, fileName, plan });
      }
    });
  });

  return { byIdentity };
}

function resolveWorkspaceQuotaAuthMetadata(snapshot, noteIndex) {
  if (!snapshot || !noteIndex) {
    return { note: "", email: "", fileName: "", plan: "" };
  }

  const identityCandidates = [
    snapshot.auth_id,
    snapshot.auth_label,
    snapshot.account_email,
  ];
  for (const candidate of identityCandidates) {
    const normalized = normalizeWorkspaceQuotaLookupKey(candidate);
    if (!normalized) {
      continue;
    }
    const metadata = noteIndex.byIdentity.get(normalized);
    if (metadata) {
      return metadata;
    }
  }

  return { note: "", email: "", fileName: "", plan: "" };
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
  const resetLabel = formatWorkspaceQuotaResetSummary(windowData.resets_at);
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
            <div class="workspace-quota-metric-subtitle">${escapeWorkspaceQuotaHtml(resetLabel)}</div>
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

function getWorkspaceQuotaDescriptionText(fetchedValue) {
  const currentLanguage = window.NiceCLIi18n?.getCurrentLanguage?.();
  if (currentLanguage === "zh-CN") {
    return `查看最新的 Codex quota 获取时间 ${fetchedValue}`;
  }
  return `View latest Codex quota fetched ${fetchedValue}`;
}

function getWorkspaceQuotaLatestFetchedAt() {
  let latestTimestamp = 0;

  workspaceQuotaSnapshots.forEach((snapshot) => {
    const timestamp = new Date(snapshot?.fetched_at || "").getTime();
    if (Number.isFinite(timestamp) && timestamp > latestTimestamp) {
      latestTimestamp = timestamp;
    }
  });

  return latestTimestamp > 0 ? latestTimestamp : null;
}

function updateWorkspaceQuotaDescription() {
  if (!workspaceQuotaDescription) {
    return;
  }

  const latestFetchedAt = getWorkspaceQuotaLatestFetchedAt();
  const fetchedValue = latestFetchedAt
    ? formatWorkspaceQuotaDate(latestFetchedAt)
    : nicecliT("common.unknown");
  workspaceQuotaDescription.textContent =
    getWorkspaceQuotaDescriptionText(fetchedValue);
}

function normalizeWorkspaceQuotaText(value) {
  return String(value ?? "").trim();
}

function normalizeWorkspaceQuotaLookupKey(value) {
  return normalizeWorkspaceQuotaText(value).toLowerCase();
}

function extractWorkspaceQuotaEmail(value) {
  const text = normalizeWorkspaceQuotaText(value);
  if (!text) {
    return "";
  }
  const match = text.match(/[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}/i);
  return match ? normalizeWorkspaceQuotaText(match[0]) : "";
}

function resolveWorkspaceQuotaAuthFileEmail(file) {
  return pickFirstWorkspaceQuotaText(
    normalizeWorkspaceQuotaText(file?.email),
    extractWorkspaceQuotaEmail(file?.name),
    extractWorkspaceQuotaEmail(file?.id),
    extractWorkspaceQuotaEmail(file?.auth_id),
  );
}

function resolveWorkspaceQuotaAuthFileName(file) {
  return pickFirstWorkspaceQuotaText(
    normalizeWorkspaceQuotaText(file?.name),
    normalizeWorkspaceQuotaText(file?.id),
    normalizeWorkspaceQuotaText(file?.auth_id),
  );
}

function extractWorkspaceQuotaPlanFromFilename(value) {
  const text = normalizeWorkspaceQuotaText(value);
  if (!text) {
    return "";
  }
  const fileName = text.split(/[\\/]/).pop() || text;
  const match = fileName.match(/-(team|pro|plus)(?:\.[^.]+)?$/i);
  return match ? match[1].toLowerCase() : "";
}

function normalizeWorkspaceQuotaPlanTier(value) {
  const normalized = normalizeWorkspaceQuotaLookupKey(value);
  if (
    normalized === "team" ||
    normalized === "pro" ||
    normalized === "plus"
  ) {
    return normalized;
  }
  return "";
}

function getWorkspaceQuotaAccountEmail(snapshot) {
  return pickFirstWorkspaceQuotaText(
    normalizeWorkspaceQuotaText(snapshot?.account_email),
    extractWorkspaceQuotaEmail(snapshot?.auth_id),
    extractWorkspaceQuotaEmail(snapshot?.auth_label),
  );
}

function getWorkspaceQuotaAccountKey(snapshot) {
  const email = getWorkspaceQuotaAccountEmail(snapshot);
  if (email) {
    return `email:${normalizeWorkspaceQuotaLookupKey(email)}`;
  }
  const fallback = pickFirstWorkspaceQuotaText(
    snapshot?.auth_id,
    snapshot?.auth_label,
  );
  return fallback ? `auth:${normalizeWorkspaceQuotaLookupKey(fallback)}` : "";
}

function formatWorkspaceQuotaWorkspaceTitle(snapshot) {
  const workspaceName = normalizeWorkspaceQuotaText(snapshot?.workspace_name);
  const authNote = normalizeWorkspaceQuotaText(snapshot?.auth_note);
  const workspaceId = normalizeWorkspaceQuotaText(snapshot?.workspace_id);
  const currentWorkspace = nicecliT("workspaceQuota.currentWorkspace");

  if (workspaceName && !isWorkspaceQuotaGenericWorkspaceValue(workspaceName)) {
    return workspaceName;
  }
  return pickFirstWorkspaceQuotaText(
    authNote,
    workspaceName,
    workspaceId,
    currentWorkspace,
  );
}

function isWorkspaceQuotaGenericWorkspaceValue(value) {
  const normalized = normalizeWorkspaceQuotaLookupKey(value);
  if (!normalized) {
    return true;
  }

  return (
    normalized === "personal" ||
    normalized === "business" ||
    normalized === normalizeWorkspaceQuotaLookupKey(
      nicecliT("workspaceQuota.currentWorkspace"),
    ) ||
    normalized === normalizeWorkspaceQuotaLookupKey(nicecliT("common.unknown"))
  );
}

function shouldDisplayWorkspaceQuotaWorkspaceTitle(snapshot) {
  const title = formatWorkspaceQuotaWorkspaceTitle(snapshot);
  if (!title) {
    return false;
  }

  const workspaceId = normalizeWorkspaceQuotaText(snapshot?.workspace_id);
  if (
    workspaceId &&
    normalizeWorkspaceQuotaLookupKey(title) ===
      normalizeWorkspaceQuotaLookupKey(workspaceId)
  ) {
    return true;
  }

  return !isWorkspaceQuotaGenericWorkspaceValue(title);
}

function resolveWorkspaceQuotaPlanType(snapshot) {
  return pickFirstWorkspaceQuotaText(
    normalizeWorkspaceQuotaPlanTier(snapshot?.account_plan),
    extractWorkspaceQuotaPlanFromFilename(snapshot?.auth_file_name),
    extractWorkspaceQuotaPlanFromFilename(snapshot?.auth_id),
    extractWorkspaceQuotaPlanFromFilename(snapshot?.auth_label),
    normalizeWorkspaceQuotaPlanTier(snapshot?.snapshot?.plan_type),
  );
}

function formatWorkspaceQuotaPlanLabel(snapshot) {
  const planType =
    resolveWorkspaceQuotaPlanType(snapshot) ||
    nicecliT("workspaceQuota.unknownPlan");
  return nicecliT("workspaceQuota.plan", { value: planType });
}

function getWorkspaceQuotaWorkspaceIdentity(snapshot) {
  return pickFirstWorkspaceQuotaText(
    snapshot?.workspace_id,
    formatWorkspaceQuotaWorkspaceTitle(snapshot),
  );
}

function getWorkspaceQuotaWorkspaceFilterKey(snapshot) {
  const accountKey = getWorkspaceQuotaAccountKey(snapshot) || "account:unknown";
  const workspaceIdentity = normalizeWorkspaceQuotaLookupKey(
    getWorkspaceQuotaWorkspaceIdentity(snapshot),
  );
  return workspaceIdentity ? `${accountKey}::${workspaceIdentity}` : accountKey;
}

function resolveWorkspaceQuotaWorkspaceFilterSnapshot(snapshots) {
  const snapshotList = Array.isArray(snapshots)
    ? snapshots.filter(Boolean)
    : [snapshots].filter(Boolean);
  if (snapshotList.length === 0) {
    return null;
  }

  return (
    snapshotList.find((snapshot) => {
      const workspaceName = normalizeWorkspaceQuotaText(snapshot?.workspace_name);
      return workspaceName && !isWorkspaceQuotaGenericWorkspaceValue(workspaceName);
    }) ||
    snapshotList.find((snapshot) => normalizeWorkspaceQuotaText(snapshot?.auth_note)) ||
    snapshotList.find((snapshot) => normalizeWorkspaceQuotaText(snapshot?.workspace_id)) ||
    snapshotList[0]
  );
}

function formatWorkspaceQuotaWorkspaceFilterTitle(snapshot) {
  if (!snapshot) {
    return nicecliT("workspaceQuota.currentWorkspace");
  }

  const workspaceName = normalizeWorkspaceQuotaText(snapshot?.workspace_name);
  const authNote = normalizeWorkspaceQuotaText(snapshot?.auth_note);
  const workspaceId = normalizeWorkspaceQuotaText(snapshot?.workspace_id);
  if (workspaceName && !isWorkspaceQuotaGenericWorkspaceValue(workspaceName)) {
    return workspaceName;
  }

  if (
    authNote &&
    workspaceId &&
    normalizeWorkspaceQuotaLookupKey(authNote) !==
      normalizeWorkspaceQuotaLookupKey(workspaceId)
  ) {
    return `${authNote}（${workspaceId}）`;
  }

  return pickFirstWorkspaceQuotaText(
    authNote,
    workspaceId,
    workspaceName,
    nicecliT("workspaceQuota.currentWorkspace"),
  );
}

function formatWorkspaceQuotaWorkspaceFilterLabel(
  snapshots,
  includeAccountLabel = false,
) {
  const snapshot = resolveWorkspaceQuotaWorkspaceFilterSnapshot(snapshots);
  const workspaceTitle = formatWorkspaceQuotaWorkspaceFilterTitle(snapshot);
  if (!includeAccountLabel) {
    return workspaceTitle;
  }
  const accountTitle = formatWorkspaceQuotaAuthTitle(snapshot || {});
  if (!accountTitle || accountTitle === workspaceTitle) {
    return workspaceTitle;
  }
  return `${workspaceTitle}（${accountTitle}）`;
}

function getWorkspaceQuotaRefreshTargets(accountKey, workspaceKey) {
  const normalizedAccountKey = normalizeWorkspaceQuotaText(accountKey);
  const normalizedWorkspaceKey = normalizeWorkspaceQuotaText(workspaceKey);

  if (!normalizedAccountKey && !normalizedWorkspaceKey) {
    return [{ authId: "", workspaceId: "" }];
  }

  const matchingSnapshots = workspaceQuotaSnapshots.filter((snapshot) => {
    if (
      normalizedAccountKey &&
      getWorkspaceQuotaAccountKey(snapshot) !== normalizedAccountKey
    ) {
      return false;
    }
    if (
      normalizedWorkspaceKey &&
      getWorkspaceQuotaWorkspaceFilterKey(snapshot) !== normalizedWorkspaceKey
    ) {
      return false;
    }
    return true;
  });

  if (!normalizedWorkspaceKey) {
    const targets = Array.from(
      new Map(
        matchingSnapshots
          .map((snapshot) => normalizeWorkspaceQuotaText(snapshot?.auth_id))
          .filter(Boolean)
          .map((authId) => [authId, { authId, workspaceId: "" }]),
      ).values(),
    );
    return targets.length > 0 ? targets : [{ authId: "", workspaceId: "" }];
  }

  const targets = Array.from(
    new Map(
      matchingSnapshots
        .map((snapshot) => ({
          authId: normalizeWorkspaceQuotaText(snapshot?.auth_id),
          workspaceId: normalizeWorkspaceQuotaText(snapshot?.workspace_id),
        }))
        .filter((target) => target.authId || target.workspaceId)
        .map((target) => [
          `${target.authId}::${target.workspaceId}`,
          target,
        ]),
    ).values(),
  );
  return targets.length > 0 ? targets : [{ authId: "", workspaceId: "" }];
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

function formatWorkspaceQuotaResetSummary(unixSeconds) {
  if (!unixSeconds) {
    return nicecliT("workspaceQuota.resetUnavailable");
  }

  const resetTimestamp = Number(unixSeconds) * 1000;
  if (!Number.isFinite(resetTimestamp)) {
    return nicecliT("workspaceQuota.resetUnavailable");
  }

  const resetAt = formatWorkspaceQuotaResetTimestamp(resetTimestamp);
  const remainingMs = resetTimestamp - Date.now();
  if (remainingMs <= 0) {
    return nicecliT("workspaceQuota.nextResetWithCountdown", {
      value: resetAt,
      countdown: nicecliT("workspaceQuota.resetSoon"),
    });
  }

  const totalMinutes = Math.max(1, Math.ceil(remainingMs / 60000));
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  const countdown = nicecliT("workspaceQuota.countdownHoursMinutes", {
    hours,
    minutes,
  });
  return nicecliT("workspaceQuota.nextResetWithCountdown", {
    value: resetAt,
    countdown,
  });
}

function formatWorkspaceQuotaResetTimestamp(timestampMs) {
  const date = new Date(timestampMs);
  if (Number.isNaN(date.getTime())) {
    return nicecliT("common.unknown");
  }

  const year = String(date.getFullYear());
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  const hours = String(date.getHours()).padStart(2, "0");
  const minutes = String(date.getMinutes()).padStart(2, "0");
  return `${year}/${month}/${day}/${hours}:${minutes}`;
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

document.addEventListener("visibilitychange", () => {
  if (!document.hidden && isWorkspaceQuotaTabActive()) {
    renderWorkspaceQuotaSnapshots();
  }
});

window.addEventListener("beforeunload", () => {
  stopWorkspaceQuotaAutoRefresh();
  stopWorkspaceQuotaCountdownRefresh();
});
