package test

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"

	"github.com/gin-gonic/gin"
	management "github.com/router-for-me/CLIProxyAPI/v6/internal/api/handlers/management"
	"github.com/router-for-me/CLIProxyAPI/v6/internal/config"
	"github.com/router-for-me/CLIProxyAPI/v6/internal/quota"
)

type testQuotaSource struct {
	listWorkspacesFn         func(context.Context, *quota.CodexAuthContext) ([]quota.WorkspaceRef, error)
	fetchWorkspaceSnapshotFn func(context.Context, *quota.CodexAuthContext, quota.WorkspaceRef) (*quota.RateLimitSnapshot, error)
}

func (s *testQuotaSource) ListWorkspaces(ctx context.Context, auth *quota.CodexAuthContext) ([]quota.WorkspaceRef, error) {
	if s != nil && s.listWorkspacesFn != nil {
		return s.listWorkspacesFn(ctx, auth)
	}
	return nil, nil
}

func (s *testQuotaSource) FetchWorkspaceSnapshot(ctx context.Context, auth *quota.CodexAuthContext, ws quota.WorkspaceRef) (*quota.RateLimitSnapshot, error) {
	if s != nil && s.fetchWorkspaceSnapshotFn != nil {
		return s.fetchWorkspaceSnapshotFn(ctx, auth, ws)
	}
	return nil, nil
}

type testQuotaAuthEnumerator struct {
	listFn func(context.Context) ([]*quota.CodexAuthContext, error)
}

func (s *testQuotaAuthEnumerator) ListCodexAuths(ctx context.Context) ([]*quota.CodexAuthContext, error) {
	if s != nil && s.listFn != nil {
		return s.listFn(ctx)
	}
	return nil, nil
}

func TestCodexQuotaManagementEmptyResponse(t *testing.T) {
	handler := newQuotaManagementHandler(t)
	router := setupQuotaManagementRouter(handler)

	req := httptest.NewRequest(http.MethodGet, "/v0/management/codex/quota-snapshots", nil)
	recorder := httptest.NewRecorder()
	router.ServeHTTP(recorder, req)

	if recorder.Code != http.StatusOK {
		t.Fatalf("expected status %d, got %d", http.StatusOK, recorder.Code)
	}

	var resp quota.SnapshotListResponse
	if err := json.Unmarshal(recorder.Body.Bytes(), &resp); err != nil {
		t.Fatalf("failed to unmarshal response: %v", err)
	}
	if resp.Provider != quota.ProviderCodex {
		t.Fatalf("provider = %q, want %q", resp.Provider, quota.ProviderCodex)
	}
	if len(resp.Snapshots) != 0 {
		t.Fatalf("expected empty snapshots, got %d", len(resp.Snapshots))
	}
}

func TestCodexQuotaManagementGetSupportsRefreshAndFilters(t *testing.T) {
	handler := newQuotaManagementHandler(t)
	service := quota.NewCodexQuotaServiceWithDeps(
		quota.NewSnapshotCache(),
		&testQuotaSource{
			listWorkspacesFn: func(context.Context, *quota.CodexAuthContext) ([]quota.WorkspaceRef, error) {
				return []quota.WorkspaceRef{
					{ID: "ws-1", Name: "Workspace 1", Type: "business"},
					{ID: "ws-2", Name: "Workspace 2", Type: "business"},
				}, nil
			},
			fetchWorkspaceSnapshotFn: func(ctx context.Context, auth *quota.CodexAuthContext, ws quota.WorkspaceRef) (*quota.RateLimitSnapshot, error) {
				return &quota.RateLimitSnapshot{
					LimitID:  stringPointer("codex"),
					PlanType: stringPointer("pro"),
				}, nil
			},
		},
		&testQuotaAuthEnumerator{
			listFn: func(context.Context) ([]*quota.CodexAuthContext, error) {
				return []*quota.CodexAuthContext{
					{AuthID: "auth-1", AuthLabel: "Primary", AuthNote: "Main Account", AccountEmail: "user@example.com", AccountID: "acct-1"},
				}, nil
			},
		},
	)
	handler.SetQuotaService(service)
	router := setupQuotaManagementRouter(handler)

	req := httptest.NewRequest(http.MethodGet, "/v0/management/codex/quota-snapshots?refresh=1&auth_id=auth-1&workspace_id=ws-2", nil)
	recorder := httptest.NewRecorder()
	router.ServeHTTP(recorder, req)

	if recorder.Code != http.StatusOK {
		t.Fatalf("expected status %d, got %d: %s", http.StatusOK, recorder.Code, recorder.Body.String())
	}

	var resp quota.SnapshotListResponse
	if err := json.Unmarshal(recorder.Body.Bytes(), &resp); err != nil {
		t.Fatalf("failed to unmarshal response: %v", err)
	}
	if len(resp.Snapshots) != 1 {
		t.Fatalf("expected 1 snapshot, got %d", len(resp.Snapshots))
	}
	if resp.Snapshots[0].AuthID != "auth-1" || resp.Snapshots[0].WorkspaceID != "ws-2" {
		t.Fatalf("unexpected snapshot: %+v", resp.Snapshots[0])
	}
	if resp.Snapshots[0].AuthNote != "Main Account" {
		t.Fatalf("expected auth note in response, got %+v", resp.Snapshots[0])
	}
}

func TestCodexQuotaManagementRefreshEndpointMarksStaleError(t *testing.T) {
	handler := newQuotaManagementHandler(t)
	cache := quota.NewSnapshotCache()
	cache.Upsert(&quota.CodexQuotaSnapshotEnvelope{
		AuthID:        "auth-1",
		AuthLabel:     "Primary",
		AuthNote:      "Main Account",
		AccountEmail:  "user@example.com",
		WorkspaceID:   "ws-1",
		WorkspaceName: "Workspace 1",
		WorkspaceType: "business",
		Snapshot: &quota.RateLimitSnapshot{
			LimitID:  stringPointer("codex"),
			PlanType: stringPointer("business"),
		},
		Source: "inline_rate_limits",
	})
	service := quota.NewCodexQuotaServiceWithDeps(
		cache,
		&testQuotaSource{
			listWorkspacesFn: func(context.Context, *quota.CodexAuthContext) ([]quota.WorkspaceRef, error) {
				return []quota.WorkspaceRef{{ID: "ws-1", Name: "Workspace 1", Type: "business"}}, nil
			},
			fetchWorkspaceSnapshotFn: func(context.Context, *quota.CodexAuthContext, quota.WorkspaceRef) (*quota.RateLimitSnapshot, error) {
				return nil, errors.New("refresh failed")
			},
		},
		&testQuotaAuthEnumerator{
			listFn: func(context.Context) ([]*quota.CodexAuthContext, error) {
				return []*quota.CodexAuthContext{
					{AuthID: "auth-1", AuthLabel: "Primary", AuthNote: "Main Account", AccountEmail: "user@example.com", AccountID: "acct-1"},
				}, nil
			},
		},
	)
	handler.SetQuotaService(service)
	router := setupQuotaManagementRouter(handler)

	req := httptest.NewRequest(http.MethodPost, "/v0/management/codex/quota-snapshots/refresh", bytes.NewBufferString(`{"auth_id":"auth-1","workspace_id":"ws-1"}`))
	req.Header.Set("Content-Type", "application/json")
	recorder := httptest.NewRecorder()
	router.ServeHTTP(recorder, req)

	if recorder.Code != http.StatusOK {
		t.Fatalf("expected status %d, got %d: %s", http.StatusOK, recorder.Code, recorder.Body.String())
	}

	var resp quota.SnapshotListResponse
	if err := json.Unmarshal(recorder.Body.Bytes(), &resp); err != nil {
		t.Fatalf("failed to unmarshal response: %v", err)
	}
	if len(resp.Snapshots) != 1 {
		t.Fatalf("expected 1 snapshot, got %d", len(resp.Snapshots))
	}
	if !resp.Snapshots[0].Stale || resp.Snapshots[0].Error != "refresh failed" {
		t.Fatalf("expected stale error response, got %+v", resp.Snapshots[0])
	}
	if resp.Snapshots[0].Snapshot == nil || resp.Snapshots[0].Snapshot.PlanType == nil || *resp.Snapshots[0].Snapshot.PlanType != "business" {
		t.Fatalf("expected cached snapshot to be retained, got %+v", resp.Snapshots[0].Snapshot)
	}
	if resp.Snapshots[0].AuthNote != "Main Account" {
		t.Fatalf("expected auth note to be retained, got %+v", resp.Snapshots[0])
	}
}

func setupQuotaManagementRouter(handler *management.Handler) *gin.Engine {
	router := gin.New()
	group := router.Group("/v0/management")
	group.GET("/codex/quota-snapshots", handler.GetCodexQuotaSnapshots)
	group.POST("/codex/quota-snapshots/refresh", handler.RefreshCodexQuotaSnapshots)
	return router
}

func newQuotaManagementHandler(t *testing.T) *management.Handler {
	t.Helper()

	gin.SetMode(gin.TestMode)
	tmpDir := t.TempDir()
	configPath := filepath.Join(tmpDir, "config.yaml")
	if err := os.WriteFile(configPath, []byte("port: 8080\n"), 0o644); err != nil {
		t.Fatalf("failed to write config file: %v", err)
	}
	return management.NewHandler(&config.Config{}, configPath, nil)
}

func stringPointer(value string) *string {
	return &value
}
