package management

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"github.com/gin-gonic/gin"
	"github.com/router-for-me/CLIProxyAPI/v6/internal/config"
	coreauth "github.com/router-for-me/CLIProxyAPI/v6/sdk/cliproxy/auth"
)

func TestPatchAuthFileFieldsUpdatesNote(t *testing.T) {
	t.Setenv("MANAGEMENT_PASSWORD", "")
	gin.SetMode(gin.TestMode)

	store := &memoryAuthStore{}
	manager := coreauth.NewManager(store, nil, nil)
	record := &coreauth.Auth{
		ID:       "auth-1",
		FileName: "codex-user.json",
		Provider: "codex",
		Attributes: map[string]string{
			"path": t.TempDir() + "/codex-user.json",
		},
		Metadata: map[string]any{
			"email": "user@example.com",
		},
	}
	if _, err := manager.Register(context.Background(), record); err != nil {
		t.Fatalf("failed to register auth record: %v", err)
	}

	h := NewHandlerWithoutConfigFilePath(&config.Config{AuthDir: t.TempDir()}, manager)
	h.tokenStore = store

	body := strings.NewReader(`{"name":"codex-user.json","note":"Prod Account"}`)
	recorder := httptest.NewRecorder()
	ctx, _ := gin.CreateTestContext(recorder)
	req := httptest.NewRequest(http.MethodPatch, "/v0/management/auth-files/fields", body)
	req.Header.Set("Content-Type", "application/json")
	ctx.Request = req
	h.PatchAuthFileFields(ctx)

	if recorder.Code != http.StatusOK {
		t.Fatalf("expected status %d, got %d with body %s", http.StatusOK, recorder.Code, recorder.Body.String())
	}

	updated, ok := manager.GetByID("auth-1")
	if !ok || updated == nil {
		t.Fatal("expected updated auth record to be available")
	}
	if got := strings.TrimSpace(updated.Attributes["note"]); got != "Prod Account" {
		t.Fatalf("expected auth attribute note to be updated, got %q", got)
	}
	if got := strings.TrimSpace(updated.Metadata["note"].(string)); got != "Prod Account" {
		t.Fatalf("expected auth metadata note to be updated, got %q", got)
	}

	listRecorder := httptest.NewRecorder()
	listCtx, _ := gin.CreateTestContext(listRecorder)
	listReq := httptest.NewRequest(http.MethodGet, "/v0/management/auth-files", nil)
	listCtx.Request = listReq
	h.ListAuthFiles(listCtx)

	if listRecorder.Code != http.StatusOK {
		t.Fatalf("expected list status %d, got %d with body %s", http.StatusOK, listRecorder.Code, listRecorder.Body.String())
	}

	var payload struct {
		Files []map[string]any `json:"files"`
	}
	if err := json.Unmarshal(listRecorder.Body.Bytes(), &payload); err != nil {
		t.Fatalf("failed to decode list payload: %v", err)
	}
	if len(payload.Files) != 1 {
		t.Fatalf("expected 1 auth file entry, got %d", len(payload.Files))
	}
	if note, _ := payload.Files[0]["note"].(string); note != "Prod Account" {
		t.Fatalf("expected list payload note to match, got %#v", payload.Files[0]["note"])
	}
}
