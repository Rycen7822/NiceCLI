package quota

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"errors"
	"testing"
	"time"

	coreauth "github.com/router-for-me/CLIProxyAPI/v6/sdk/cliproxy/auth"
)

type stubQuotaSource struct {
	listWorkspacesFn         func(context.Context, *CodexAuthContext) ([]WorkspaceRef, error)
	fetchWorkspaceSnapshotFn func(context.Context, *CodexAuthContext, WorkspaceRef) (*RateLimitSnapshot, error)
}

func (s *stubQuotaSource) ListWorkspaces(ctx context.Context, auth *CodexAuthContext) ([]WorkspaceRef, error) {
	if s != nil && s.listWorkspacesFn != nil {
		return s.listWorkspacesFn(ctx, auth)
	}
	return nil, nil
}

func (s *stubQuotaSource) FetchWorkspaceSnapshot(ctx context.Context, auth *CodexAuthContext, ws WorkspaceRef) (*RateLimitSnapshot, error) {
	if s != nil && s.fetchWorkspaceSnapshotFn != nil {
		return s.fetchWorkspaceSnapshotFn(ctx, auth, ws)
	}
	return nil, nil
}

type stubAuthEnumerator struct {
	listFn func(context.Context) ([]*CodexAuthContext, error)
}

func (s *stubAuthEnumerator) ListCodexAuths(ctx context.Context) ([]*CodexAuthContext, error) {
	if s != nil && s.listFn != nil {
		return s.listFn(ctx)
	}
	return nil, nil
}

func TestBuildCodexAuthContextFiltersAndExtractsMetadata(t *testing.T) {
	validIDToken := testCodexJWT(t, map[string]any{
		"email": "user@example.com",
		"https://api.openai.com/auth": map[string]any{
			"chatgpt_account_id": "acct-1",
			"chatgpt_plan_type":  "business",
			"organizations": []map[string]any{
				{"id": "ws-1", "title": "Workspace A"},
			},
		},
	})

	auth := &coreauth.Auth{
		ID:       "auth-1",
		Provider: ProviderCodex,
		Label:    "Primary",
		ProxyURL: "http://proxy.example.com:8080",
		Attributes: map[string]string{
			"base_url": "https://chatgpt.com/backend-api/codex",
		},
		Metadata: map[string]any{
			"email":         "user@example.com",
			"account_id":    "acct-1",
			"access_token":  "access-1",
			"refresh_token": "refresh-1",
			"id_token":      validIDToken,
			"note":          "Work Laptop",
			"cookies": map[string]any{
				"cookie_a": "value-a",
			},
		},
	}

	got, ok := buildCodexAuthContext(nil, auth)
	if !ok || got == nil {
		t.Fatal("expected codex auth context to be built")
	}
	if got.AuthID != "auth-1" || got.AuthLabel != "Primary" {
		t.Fatalf("unexpected auth identity: %+v", got)
	}
	if got.AuthNote != "Work Laptop" {
		t.Fatalf("unexpected auth note: %+v", got)
	}
	if got.AccountEmail != "user@example.com" || got.AccountID != "acct-1" {
		t.Fatalf("unexpected account info: %+v", got)
	}
	if got.AccessToken != "access-1" || got.RefreshToken != "refresh-1" || got.IDToken != validIDToken {
		t.Fatalf("unexpected token fields: %+v", got)
	}
	if got.BaseURL != "https://chatgpt.com/backend-api/codex" || got.ProxyURL != "http://proxy.example.com:8080" {
		t.Fatalf("unexpected urls: %+v", got)
	}
	if got.Cookies["cookie_a"] != "value-a" {
		t.Fatalf("expected cookies to be extracted, got %+v", got.Cookies)
	}

	if _, ok := buildCodexAuthContext(nil, &coreauth.Auth{
		ID:       "auth-2",
		Provider: ProviderCodex,
		Attributes: map[string]string{
			"api_key": "sk-test",
		},
		Metadata: map[string]any{
			"access_token": "ignored",
		},
	}); ok {
		t.Fatal("api key auth should not enter codex quota flow")
	}

	if _, ok := buildCodexAuthContext(nil, &coreauth.Auth{
		ID:       "auth-3",
		Provider: "openai",
		Metadata: map[string]any{
			"access_token": "ignored",
		},
	}); ok {
		t.Fatal("non-codex auth should not enter codex quota flow")
	}
}

func TestCodexProviderListWorkspacesAndFallback(t *testing.T) {
	provider := newCodexProvider(nil)

	workspaces, err := provider.ListWorkspaces(context.Background(), &CodexAuthContext{
		AccountID: "acct-current",
		IDToken: testCodexJWT(t, map[string]any{
			"https://api.openai.com/auth": map[string]any{
				"chatgpt_account_id": "acct-current",
				"chatgpt_plan_type":  "enterprise",
				"organizations": []map[string]any{
					{"id": "ws-b", "title": "Workspace B"},
					{"id": "ws-a", "title": "Workspace A"},
				},
			},
		}),
	})
	if err != nil {
		t.Fatalf("ListWorkspaces returned error: %v", err)
	}
	if len(workspaces) != 2 {
		t.Fatalf("expected 2 workspaces, got %d", len(workspaces))
	}
	if workspaces[0].ID != "ws-a" || workspaces[0].Name != "Workspace A" || workspaces[0].Type != "enterprise" {
		t.Fatalf("unexpected first workspace: %+v", workspaces[0])
	}

	fallback, err := provider.ListWorkspaces(context.Background(), &CodexAuthContext{
		AccountID:    "acct-fallback",
		AccountEmail: "fallback@example.com",
	})
	if err != nil {
		t.Fatalf("fallback ListWorkspaces returned error: %v", err)
	}
	if len(fallback) != 1 {
		t.Fatalf("expected 1 fallback workspace, got %d", len(fallback))
	}
	if fallback[0].ID != "acct-fallback" || fallback[0].Name != "fallback@example.com" || fallback[0].Type != "personal" {
		t.Fatalf("unexpected fallback workspace: %+v", fallback[0])
	}
}

func TestCodexQuotaServiceRefreshWithOptionsFiltersAuthAndWorkspace(t *testing.T) {
	var fetched []string
	service := NewCodexQuotaServiceWithDeps(
		NewSnapshotCache(),
		&stubQuotaSource{
			listWorkspacesFn: func(ctx context.Context, auth *CodexAuthContext) ([]WorkspaceRef, error) {
				switch auth.AuthID {
				case "auth-1":
					return []WorkspaceRef{
						{ID: "ws-1", Name: "Workspace 1", Type: "business"},
						{ID: "ws-2", Name: "Workspace 2", Type: "business"},
					}, nil
				case "auth-2":
					return []WorkspaceRef{{ID: "ws-3", Name: "Workspace 3", Type: "personal"}}, nil
				default:
					return nil, nil
				}
			},
			fetchWorkspaceSnapshotFn: func(ctx context.Context, auth *CodexAuthContext, ws WorkspaceRef) (*RateLimitSnapshot, error) {
				fetched = append(fetched, auth.AuthID+":"+ws.ID)
				return &RateLimitSnapshot{
					LimitID:  stringPointer(CodexPrimaryLimitID),
					PlanType: stringPointer("pro"),
				}, nil
			},
		},
		&stubAuthEnumerator{
			listFn: func(context.Context) ([]*CodexAuthContext, error) {
				return []*CodexAuthContext{
					{AuthID: "auth-1", AuthLabel: "Primary", AuthNote: "Main Account", AccountEmail: "a@example.com", AccountID: "acct-1"},
					{AuthID: "auth-2", AuthLabel: "Backup", AccountEmail: "b@example.com", AccountID: "acct-2"},
				}, nil
			},
		},
	)

	snapshots, err := service.RefreshWithOptions(context.Background(), RefreshOptions{
		AuthID:      "auth-1",
		WorkspaceID: "ws-2",
	})
	if err != nil {
		t.Fatalf("RefreshWithOptions returned error: %v", err)
	}
	if len(snapshots) != 1 {
		t.Fatalf("expected 1 snapshot, got %d", len(snapshots))
	}
	if snapshots[0].AuthID != "auth-1" || snapshots[0].WorkspaceID != "ws-2" {
		t.Fatalf("unexpected snapshot: %+v", snapshots[0])
	}
	if snapshots[0].AuthNote != "Main Account" {
		t.Fatalf("expected auth note to flow into snapshot, got %+v", snapshots[0])
	}
	if len(fetched) != 1 || fetched[0] != "auth-1:ws-2" {
		t.Fatalf("unexpected fetch calls: %+v", fetched)
	}
}

func TestCodexQuotaServiceRefreshMarksStaleAndRetainsPreviousSnapshot(t *testing.T) {
	cache := NewSnapshotCache()
	cache.Upsert(&CodexQuotaSnapshotEnvelope{
		AuthID:        "auth-1",
		AuthLabel:     "Primary",
		AuthNote:      "Old Note",
		AccountEmail:  "user@example.com",
		WorkspaceID:   "ws-1",
		WorkspaceName: "Workspace 1",
		WorkspaceType: "business",
		Snapshot: &RateLimitSnapshot{
			LimitID:  stringPointer(CodexPrimaryLimitID),
			PlanType: stringPointer("business"),
		},
		Source:    SourceInlineRateLimits,
		FetchedAt: time.Unix(100, 0).UTC(),
	})

	service := NewCodexQuotaServiceWithDeps(
		cache,
		&stubQuotaSource{
			listWorkspacesFn: func(context.Context, *CodexAuthContext) ([]WorkspaceRef, error) {
				return []WorkspaceRef{{ID: "ws-1", Name: "Workspace 1", Type: "business"}}, nil
			},
			fetchWorkspaceSnapshotFn: func(context.Context, *CodexAuthContext, WorkspaceRef) (*RateLimitSnapshot, error) {
				return nil, errors.New("upstream failed")
			},
		},
		&stubAuthEnumerator{
			listFn: func(context.Context) ([]*CodexAuthContext, error) {
				return []*CodexAuthContext{
					{AuthID: "auth-1", AuthLabel: "Primary", AuthNote: "Main Account", AccountEmail: "user@example.com", AccountID: "acct-1"},
				}, nil
			},
		},
	)

	snapshots, err := service.RefreshWithOptions(context.Background(), RefreshOptions{
		AuthID:      "auth-1",
		WorkspaceID: "ws-1",
	})
	if err != nil {
		t.Fatalf("RefreshWithOptions returned error: %v", err)
	}
	if len(snapshots) != 1 {
		t.Fatalf("expected 1 snapshot, got %d", len(snapshots))
	}
	if !snapshots[0].Stale {
		t.Fatal("expected snapshot to be marked stale")
	}
	if snapshots[0].Error != "upstream failed" {
		t.Fatalf("unexpected error message: %q", snapshots[0].Error)
	}
	if snapshots[0].Snapshot == nil || snapshots[0].Snapshot.PlanType == nil || *snapshots[0].Snapshot.PlanType != "business" {
		t.Fatalf("expected previous snapshot to be retained, got %+v", snapshots[0].Snapshot)
	}
	if snapshots[0].Source != SourceInlineRateLimits {
		t.Fatalf("expected source to stay on cached snapshot, got %q", snapshots[0].Source)
	}
	if snapshots[0].AuthNote != "Main Account" {
		t.Fatalf("expected auth note to stay on cached snapshot, got %+v", snapshots[0])
	}
}

func TestCodexQuotaServiceListSnapshotsRefreshesAuthMetadataFromEnumerator(t *testing.T) {
	cache := NewSnapshotCache()
	cache.Upsert(&CodexQuotaSnapshotEnvelope{
		AuthID:       "auth-1",
		AuthLabel:    "Old Label",
		AuthNote:     "Old Note",
		AccountEmail: "old@example.com",
		WorkspaceID:  "ws-1",
		Source:       SourceInlineRateLimits,
		FetchedAt:    time.Unix(100, 0).UTC(),
	})

	service := NewCodexQuotaServiceWithDeps(
		cache,
		nil,
		&stubAuthEnumerator{
			listFn: func(context.Context) ([]*CodexAuthContext, error) {
				return []*CodexAuthContext{
					{AuthID: "auth-1", AuthLabel: "Primary", AuthNote: "Main Account", AccountEmail: "user@example.com"},
				}, nil
			},
		},
	)

	snapshots, err := service.ListSnapshotsWithOptions(context.Background(), ListOptions{
		AuthID: "auth-1",
	})
	if err != nil {
		t.Fatalf("ListSnapshotsWithOptions returned error: %v", err)
	}
	if len(snapshots) != 1 {
		t.Fatalf("expected 1 snapshot, got %d", len(snapshots))
	}
	if snapshots[0].AuthLabel != "Primary" {
		t.Fatalf("expected auth label to be refreshed from enumerator, got %+v", snapshots[0])
	}
	if snapshots[0].AuthNote != "Main Account" {
		t.Fatalf("expected auth note to be refreshed from enumerator, got %+v", snapshots[0])
	}
	if snapshots[0].AccountEmail != "user@example.com" {
		t.Fatalf("expected account email to be refreshed from enumerator, got %+v", snapshots[0])
	}
}

func TestCodexQuotaServiceListSnapshotsPrunesRemovedAuthSnapshots(t *testing.T) {
	cache := NewSnapshotCache()
	cache.Upsert(&CodexQuotaSnapshotEnvelope{
		AuthID:      "auth-1",
		WorkspaceID: "ws-1",
		Source:      SourceInlineRateLimits,
		FetchedAt:   time.Unix(100, 0).UTC(),
	})
	cache.Upsert(&CodexQuotaSnapshotEnvelope{
		AuthID:      "auth-removed",
		WorkspaceID: "ws-old",
		Source:      SourceInlineRateLimits,
		FetchedAt:   time.Unix(100, 0).UTC(),
	})

	service := NewCodexQuotaServiceWithDeps(
		cache,
		nil,
		&stubAuthEnumerator{
			listFn: func(context.Context) ([]*CodexAuthContext, error) {
				return []*CodexAuthContext{
					{AuthID: "auth-1", AuthLabel: "Primary", AccountEmail: "user@example.com"},
				}, nil
			},
		},
	)

	snapshots, err := service.ListSnapshotsWithOptions(context.Background(), ListOptions{})
	if err != nil {
		t.Fatalf("ListSnapshotsWithOptions returned error: %v", err)
	}
	if len(snapshots) != 1 {
		t.Fatalf("expected only active auth snapshot to remain, got %d", len(snapshots))
	}
	if snapshots[0].AuthID != "auth-1" {
		t.Fatalf("expected auth-1 to remain, got %+v", snapshots[0])
	}
	if stale := cache.List("auth-removed", ""); len(stale) != 0 {
		t.Fatalf("expected removed auth snapshots to be pruned from cache, got %+v", stale)
	}
}

func TestCodexQuotaServiceRefreshWithOptionsRemovesDeletedRequestedAuthSnapshots(t *testing.T) {
	cache := NewSnapshotCache()
	cache.Upsert(&CodexQuotaSnapshotEnvelope{
		AuthID:      "auth-removed",
		WorkspaceID: "ws-old",
		Source:      SourceInlineRateLimits,
		FetchedAt:   time.Unix(100, 0).UTC(),
	})

	service := NewCodexQuotaServiceWithDeps(
		cache,
		&stubQuotaSource{},
		&stubAuthEnumerator{
			listFn: func(context.Context) ([]*CodexAuthContext, error) {
				return []*CodexAuthContext{
					{AuthID: "auth-1", AuthLabel: "Primary", AccountEmail: "user@example.com"},
				}, nil
			},
		},
	)

	snapshots, err := service.RefreshWithOptions(context.Background(), RefreshOptions{
		AuthID: "auth-removed",
	})
	if err != nil {
		t.Fatalf("RefreshWithOptions returned error: %v", err)
	}
	if len(snapshots) != 0 {
		t.Fatalf("expected removed requested auth snapshot to disappear, got %+v", snapshots)
	}
	if stale := cache.List("auth-removed", ""); len(stale) != 0 {
		t.Fatalf("expected removed requested auth snapshots to be pruned from cache, got %+v", stale)
	}
}

func testCodexJWT(t *testing.T, claims map[string]any) string {
	t.Helper()

	header := base64.RawURLEncoding.EncodeToString([]byte(`{"alg":"none","typ":"JWT"}`))
	payloadBytes, err := json.Marshal(claims)
	if err != nil {
		t.Fatalf("marshal claims: %v", err)
	}
	payload := base64.RawURLEncoding.EncodeToString(payloadBytes)
	return header + "." + payload + ".signature"
}
