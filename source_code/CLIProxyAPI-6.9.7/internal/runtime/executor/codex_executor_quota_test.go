package executor

import (
	"bytes"
	"context"
	"errors"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"

	"github.com/router-for-me/CLIProxyAPI/v6/internal/config"
	"github.com/router-for-me/CLIProxyAPI/v6/internal/quota"
	cliproxyauth "github.com/router-for-me/CLIProxyAPI/v6/sdk/cliproxy/auth"
	cliproxyexecutor "github.com/router-for-me/CLIProxyAPI/v6/sdk/cliproxy/executor"
	sdktranslator "github.com/router-for-me/CLIProxyAPI/v6/sdk/translator"
)

type executorQuotaSource struct {
	listWorkspacesFn         func(context.Context, *quota.CodexAuthContext) ([]quota.WorkspaceRef, error)
	fetchWorkspaceSnapshotFn func(context.Context, *quota.CodexAuthContext, quota.WorkspaceRef) (*quota.RateLimitSnapshot, error)
}

func (s *executorQuotaSource) ListWorkspaces(ctx context.Context, auth *quota.CodexAuthContext) ([]quota.WorkspaceRef, error) {
	if s != nil && s.listWorkspacesFn != nil {
		return s.listWorkspacesFn(ctx, auth)
	}
	return nil, nil
}

func (s *executorQuotaSource) FetchWorkspaceSnapshot(ctx context.Context, auth *quota.CodexAuthContext, ws quota.WorkspaceRef) (*quota.RateLimitSnapshot, error) {
	if s != nil && s.fetchWorkspaceSnapshotFn != nil {
		return s.fetchWorkspaceSnapshotFn(ctx, auth, ws)
	}
	return nil, nil
}

type executorQuotaAuthEnumerator struct {
	listFn func(context.Context) ([]*quota.CodexAuthContext, error)
}

func (s *executorQuotaAuthEnumerator) ListCodexAuths(ctx context.Context) ([]*quota.CodexAuthContext, error) {
	if s != nil && s.listFn != nil {
		return s.listFn(ctx)
	}
	return nil, nil
}

func TestCodexExecutorExecuteCapturesInlineQuotaSnapshot(t *testing.T) {
	service := quota.NewCodexQuotaServiceWithDeps(quota.NewSnapshotCache(), nil, nil)
	restoreDefaultQuotaService(t, service)

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "text/event-stream")
		_, _ = w.Write([]byte("data: {\"type\":\"codex.rate_limits\",\"plan_type\":\"pro\",\"rate_limits\":{\"primary\":{\"used_percent\":40,\"window_minutes\":60,\"reset_at\":1234}}}\n\n"))
		_, _ = w.Write([]byte("data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\"}}\n\n"))
	}))
	defer server.Close()

	exec := NewCodexExecutor(&config.Config{})
	resp, err := exec.Execute(context.Background(), newCodexQuotaTestAuth(server.URL), cliproxyexecutor.Request{
		Model:   "gpt-5-codex",
		Payload: []byte(`{"model":"gpt-5-codex","input":[]}`),
	}, cliproxyexecutor.Options{
		SourceFormat: sdktranslator.FromString("codex"),
	})
	if err != nil {
		t.Fatalf("Execute returned error: %v", err)
	}
	if !bytes.Contains(resp.Payload, []byte(`"response.completed"`)) {
		t.Fatalf("unexpected response payload: %s", resp.Payload)
	}

	snapshots, err := service.ListSnapshotsWithOptions(context.Background(), quota.ListOptions{})
	if err != nil {
		t.Fatalf("ListSnapshotsWithOptions returned error: %v", err)
	}
	if len(snapshots) != 1 {
		t.Fatalf("expected 1 snapshot, got %d", len(snapshots))
	}
	if snapshots[0].Source != quota.SourceInlineRateLimits {
		t.Fatalf("unexpected source: %q", snapshots[0].Source)
	}
	if snapshots[0].Snapshot == nil || snapshots[0].Snapshot.PlanType == nil || *snapshots[0].Snapshot.PlanType != "pro" {
		t.Fatalf("unexpected quota snapshot: %+v", snapshots[0].Snapshot)
	}
}

func TestCodexExecutorExecuteIgnoresInvalidQuotaEvent(t *testing.T) {
	service := quota.NewCodexQuotaServiceWithDeps(quota.NewSnapshotCache(), nil, nil)
	restoreDefaultQuotaService(t, service)

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "text/event-stream")
		_, _ = w.Write([]byte("data: not-json\n\n"))
		_, _ = w.Write([]byte("data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\"}}\n\n"))
	}))
	defer server.Close()

	exec := NewCodexExecutor(&config.Config{})
	resp, err := exec.Execute(context.Background(), newCodexQuotaTestAuth(server.URL), cliproxyexecutor.Request{
		Model:   "gpt-5-codex",
		Payload: []byte(`{"model":"gpt-5-codex","input":[]}`),
	}, cliproxyexecutor.Options{
		SourceFormat: sdktranslator.FromString("codex"),
	})
	if err != nil {
		t.Fatalf("Execute returned error: %v", err)
	}
	if !bytes.Contains(resp.Payload, []byte(`"response.completed"`)) {
		t.Fatalf("unexpected response payload: %s", resp.Payload)
	}

	snapshots, err := service.ListSnapshotsWithOptions(context.Background(), quota.ListOptions{})
	if err != nil {
		t.Fatalf("ListSnapshotsWithOptions returned error: %v", err)
	}
	if len(snapshots) != 0 {
		t.Fatalf("expected no snapshots for invalid event, got %d", len(snapshots))
	}
}

func TestCodexExecutorExecuteStreamWithoutQuotaEventKeepsOutput(t *testing.T) {
	service := quota.NewCodexQuotaServiceWithDeps(quota.NewSnapshotCache(), nil, nil)
	restoreDefaultQuotaService(t, service)

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "text/event-stream")
		_, _ = w.Write([]byte("data: {\"type\":\"response.output_text.delta\",\"delta\":\"hi\"}\n\n"))
		_, _ = w.Write([]byte("data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\"}}\n\n"))
	}))
	defer server.Close()

	exec := NewCodexExecutor(&config.Config{})
	result, err := exec.ExecuteStream(context.Background(), newCodexQuotaTestAuth(server.URL), cliproxyexecutor.Request{
		Model:   "gpt-5-codex",
		Payload: []byte(`{"model":"gpt-5-codex","input":[]}`),
	}, cliproxyexecutor.Options{
		SourceFormat: sdktranslator.FromString("codex"),
	})
	if err != nil {
		t.Fatalf("ExecuteStream returned error: %v", err)
	}

	var combined bytes.Buffer
	for chunk := range result.Chunks {
		if chunk.Err != nil {
			t.Fatalf("unexpected stream chunk error: %v", chunk.Err)
		}
		combined.Write(chunk.Payload)
	}
	if !bytes.Contains(combined.Bytes(), []byte(`"response.output_text.delta"`)) {
		t.Fatalf("stream output missing delta event: %s", combined.Bytes())
	}
	if !bytes.Contains(combined.Bytes(), []byte(`"response.completed"`)) {
		t.Fatalf("stream output missing completed event: %s", combined.Bytes())
	}

	snapshots, err := service.ListSnapshotsWithOptions(context.Background(), quota.ListOptions{})
	if err != nil {
		t.Fatalf("ListSnapshotsWithOptions returned error: %v", err)
	}
	if len(snapshots) != 0 {
		t.Fatalf("expected no snapshots without quota event, got %d", len(snapshots))
	}
}

func TestCodexExecutorExecuteTriggersAsyncQuotaRefreshOn429(t *testing.T) {
	refreshed := make(chan struct{}, 1)
	service := quota.NewCodexQuotaServiceWithDeps(
		quota.NewSnapshotCache(),
		&executorQuotaSource{
			listWorkspacesFn: func(context.Context, *quota.CodexAuthContext) ([]quota.WorkspaceRef, error) {
				return []quota.WorkspaceRef{{ID: "ws-1", Name: "Workspace 1", Type: "business"}}, nil
			},
			fetchWorkspaceSnapshotFn: func(context.Context, *quota.CodexAuthContext, quota.WorkspaceRef) (*quota.RateLimitSnapshot, error) {
				select {
				case refreshed <- struct{}{}:
				default:
				}
				return &quota.RateLimitSnapshot{LimitID: stringPointer("codex")}, nil
			},
		},
		&executorQuotaAuthEnumerator{
			listFn: func(context.Context) ([]*quota.CodexAuthContext, error) {
				return []*quota.CodexAuthContext{
					{AuthID: "auth-1", AuthLabel: "Primary", AccountEmail: "user@example.com", AccountID: "acct-1"},
				}, nil
			},
		},
	)
	restoreDefaultQuotaService(t, service)

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusTooManyRequests)
		_, _ = w.Write([]byte(`{"error":{"type":"usage_limit_reached","resets_in_seconds":30}}`))
	}))
	defer server.Close()

	exec := NewCodexExecutor(&config.Config{})
	_, err := exec.Execute(context.Background(), newCodexQuotaTestAuth(server.URL), cliproxyexecutor.Request{
		Model:   "gpt-5-codex",
		Payload: []byte(`{"model":"gpt-5-codex","input":[]}`),
	}, cliproxyexecutor.Options{
		SourceFormat: sdktranslator.FromString("codex"),
	})
	if err == nil {
		t.Fatal("expected Execute to return 429 error")
	}

	select {
	case <-refreshed:
	case <-time.After(2 * time.Second):
		t.Fatal("expected async quota refresh to be triggered")
	}
}

func TestMaybeTriggerCodexQuotaRefreshIgnoresNon429(t *testing.T) {
	service := quota.NewCodexQuotaServiceWithDeps(
		quota.NewSnapshotCache(),
		&executorQuotaSource{
			listWorkspacesFn: func(context.Context, *quota.CodexAuthContext) ([]quota.WorkspaceRef, error) {
				t.Fatal("refresh should not be triggered for non-429 errors")
				return nil, nil
			},
		},
		&executorQuotaAuthEnumerator{
			listFn: func(context.Context) ([]*quota.CodexAuthContext, error) {
				t.Fatal("refresh should not be triggered for non-429 errors")
				return nil, nil
			},
		},
	)
	restoreDefaultQuotaService(t, service)

	maybeTriggerCodexQuotaRefresh(newCodexQuotaTestAuth("https://example.com"), errors.New("boom"))
}

func restoreDefaultQuotaService(t *testing.T, service *quota.CodexQuotaService) {
	t.Helper()
	previous := quota.DefaultCodexQuotaService()
	quota.SetDefaultCodexQuotaService(service)
	t.Cleanup(func() {
		quota.SetDefaultCodexQuotaService(previous)
	})
}

func newCodexQuotaTestAuth(baseURL string) *cliproxyauth.Auth {
	return &cliproxyauth.Auth{
		ID:       "auth-1",
		Provider: "codex",
		Attributes: map[string]string{
			"base_url": baseURL,
		},
		Metadata: map[string]any{
			"access_token": "access-1",
			"account_id":   "acct-1",
			"email":        "user@example.com",
		},
	}
}

func stringPointer(value string) *string {
	return &value
}
