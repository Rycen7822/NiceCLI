package quota

import (
	"context"
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestCodexProviderFetchWorkspaceSnapshotUsesCodexAPIUsageEndpoint(t *testing.T) {
	var gotPath string
	var gotAuthz string
	var gotAccount string
	var gotUserAgent string
	var gotCookie string

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		gotPath = r.URL.Path
		gotAuthz = r.Header.Get("Authorization")
		gotAccount = r.Header.Get("ChatGPT-Account-Id")
		gotUserAgent = r.Header.Get("User-Agent")
		gotCookie = r.Header.Get("Cookie")
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{
			"plan_type": "business",
			"rate_limit": {
				"primary_window": {"used_percent": 25, "limit_window_seconds": 3600, "reset_at": 1000}
			},
			"credits": {"has_credits": true, "unlimited": false, "balance": "12"}
		}`))
	}))
	defer server.Close()

	provider := newCodexProvider(nil)
	snapshot, err := provider.FetchWorkspaceSnapshot(context.Background(), &CodexAuthContext{
		AccountID:   "acct-fallback",
		AccessToken: "access-1",
		BaseURL:     server.URL,
		Cookies: map[string]string{
			"foo": "bar",
		},
	}, WorkspaceRef{ID: "ws-1"})
	if err != nil {
		t.Fatalf("FetchWorkspaceSnapshot returned error: %v", err)
	}

	if gotPath != "/api/codex/usage" {
		t.Fatalf("path = %q, want %q", gotPath, "/api/codex/usage")
	}
	if gotAuthz != "Bearer access-1" {
		t.Fatalf("Authorization = %q, want %q", gotAuthz, "Bearer access-1")
	}
	if gotAccount != "ws-1" {
		t.Fatalf("ChatGPT-Account-Id = %q, want %q", gotAccount, "ws-1")
	}
	if gotUserAgent != codexQuotaUserAgent {
		t.Fatalf("User-Agent = %q, want %q", gotUserAgent, codexQuotaUserAgent)
	}
	if gotCookie != "foo=bar" {
		t.Fatalf("Cookie = %q, want %q", gotCookie, "foo=bar")
	}
	if snapshot == nil || snapshot.PlanType == nil || *snapshot.PlanType != "business" {
		t.Fatalf("unexpected snapshot: %+v", snapshot)
	}
	if snapshot.Credits == nil || snapshot.Credits.Balance == nil || *snapshot.Credits.Balance != "12" {
		t.Fatalf("expected credits to be normalized, got %+v", snapshot.Credits)
	}
}

func TestCodexProviderFetchWorkspaceSnapshotUsesChatGPTBackendUsageEndpoint(t *testing.T) {
	var gotPath string
	var gotAccount string

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		gotPath = r.URL.Path
		gotAccount = r.Header.Get("ChatGPT-Account-Id")
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{
			"plan_type": "pro",
			"rate_limit": {
				"primary_window": {"used_percent": 10, "limit_window_seconds": 1800, "reset_at": 2000}
			},
			"additional_rate_limits": [
				{
					"limit_name": "codex_extra",
					"metered_feature": "codex_extra",
					"rate_limit": {
						"primary_window": {"used_percent": 90, "limit_window_seconds": 600, "reset_at": 3000}
					}
				}
			]
		}`))
	}))
	defer server.Close()

	provider := newCodexProvider(nil)
	snapshot, err := provider.FetchWorkspaceSnapshot(context.Background(), &CodexAuthContext{
		AccountID:   "acct-current",
		AccessToken: "access-2",
		BaseURL:     server.URL + "/backend-api/codex",
	}, WorkspaceRef{ID: DefaultWorkspaceID})
	if err != nil {
		t.Fatalf("FetchWorkspaceSnapshot returned error: %v", err)
	}

	if gotPath != "/backend-api/wham/usage" {
		t.Fatalf("path = %q, want %q", gotPath, "/backend-api/wham/usage")
	}
	if gotAccount != "acct-current" {
		t.Fatalf("ChatGPT-Account-Id = %q, want %q", gotAccount, "acct-current")
	}
	if snapshot == nil || snapshot.PlanType == nil || *snapshot.PlanType != "pro" {
		t.Fatalf("unexpected snapshot: %+v", snapshot)
	}
	if snapshot.Primary == nil || snapshot.Primary.WindowMinutes == nil || *snapshot.Primary.WindowMinutes != 30 {
		t.Fatalf("expected primary window to be normalized, got %+v", snapshot.Primary)
	}
}
