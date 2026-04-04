package management

import (
	"bytes"
	"encoding/json"
	"net/http"
	"strings"

	"github.com/gin-gonic/gin"
	"github.com/router-for-me/CLIProxyAPI/v6/internal/quota"
)

type codexQuotaRefreshRequest struct {
	AuthID      string `json:"auth_id"`
	WorkspaceID string `json:"workspace_id"`
}

func (h *Handler) GetCodexQuotaSnapshots(c *gin.Context) {
	if h == nil || h.quotaService == nil {
		c.JSON(http.StatusOK, quota.NewSnapshotListResponse(nil))
		return
	}

	snapshots, err := h.quotaService.ListSnapshotsWithOptions(c.Request.Context(), quota.ListOptions{
		Refresh:     parseManagementBool(c.Query("refresh")),
		AuthID:      c.Query("auth_id"),
		WorkspaceID: c.Query("workspace_id"),
	})
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}

	c.JSON(http.StatusOK, quota.NewSnapshotListResponse(snapshots))
}

func (h *Handler) RefreshCodexQuotaSnapshots(c *gin.Context) {
	if h == nil || h.quotaService == nil {
		c.JSON(http.StatusOK, quota.NewSnapshotListResponse(nil))
		return
	}

	var req codexQuotaRefreshRequest
	if raw := readOptionalJSONBody(c); raw != nil {
		if err := json.Unmarshal(raw, &req); err != nil {
			c.JSON(http.StatusBadRequest, gin.H{"error": "invalid body"})
			return
		}
	}

	snapshots, err := h.quotaService.RefreshWithOptions(c.Request.Context(), quota.RefreshOptions{
		AuthID:      req.AuthID,
		WorkspaceID: req.WorkspaceID,
	})
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}

	c.JSON(http.StatusOK, quota.NewSnapshotListResponse(snapshots))
}

func parseManagementBool(raw string) bool {
	switch strings.ToLower(strings.TrimSpace(raw)) {
	case "1", "true", "yes", "on":
		return true
	default:
		return false
	}
}

func readOptionalJSONBody(c *gin.Context) []byte {
	if c == nil || c.Request == nil || c.Request.Body == nil {
		return nil
	}
	raw, err := c.GetRawData()
	if err != nil {
		return []byte{}
	}
	raw = bytes.TrimSpace(raw)
	if len(raw) == 0 {
		return nil
	}
	return raw
}
