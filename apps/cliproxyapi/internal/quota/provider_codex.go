package quota

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"sort"
	"strings"
	"time"

	codexauth "github.com/router-for-me/CLIProxyAPI/v6/internal/auth/codex"
	"github.com/router-for-me/CLIProxyAPI/v6/internal/config"
	coreauth "github.com/router-for-me/CLIProxyAPI/v6/sdk/cliproxy/auth"
	"github.com/router-for-me/CLIProxyAPI/v6/sdk/proxyutil"
)

const codexQuotaUserAgent = "codex_cli_rs/0.116.0 (Mac OS 26.0.1; arm64) Apple_Terminal/464"

type codexAuthEnumerator struct {
	cfg         *config.Config
	authManager *coreauth.Manager
}

func newCodexAuthEnumerator(cfg *config.Config, authManager *coreauth.Manager) *codexAuthEnumerator {
	return &codexAuthEnumerator{
		cfg:         cfg,
		authManager: authManager,
	}
}

func (e *codexAuthEnumerator) SetConfig(cfg *config.Config) {
	e.cfg = cfg
}

func (e *codexAuthEnumerator) SetAuthManager(authManager *coreauth.Manager) {
	e.authManager = authManager
}

func (e *codexAuthEnumerator) ListCodexAuths(ctx context.Context) ([]*CodexAuthContext, error) {
	_ = ctx
	if e == nil || e.authManager == nil {
		return nil, nil
	}

	auths := e.authManager.List()
	out := make([]*CodexAuthContext, 0, len(auths))
	for _, auth := range auths {
		if authCtx, ok := buildCodexAuthContext(e.cfg, auth); ok {
			out = append(out, authCtx)
		}
	}

	sort.Slice(out, func(i, j int) bool {
		if out[i].AuthID != out[j].AuthID {
			return out[i].AuthID < out[j].AuthID
		}
		return out[i].AccountEmail < out[j].AccountEmail
	})
	return out, nil
}

type codexProvider struct {
	cfg        *config.Config
	httpClient *http.Client
}

func newCodexProvider(cfg *config.Config) *codexProvider {
	return &codexProvider{cfg: cfg}
}

func (p *codexProvider) SetConfig(cfg *config.Config) {
	p.cfg = cfg
}

func (p *codexProvider) ListWorkspaces(ctx context.Context, auth *CodexAuthContext) ([]WorkspaceRef, error) {
	_ = ctx
	if auth == nil {
		return nil, fmt.Errorf("codex quota: auth context is nil")
	}

	claims, _ := parseCodexClaims(auth.IDToken)
	workspaces := workspacesFromClaims(claims)
	if len(workspaces) > 0 {
		return workspaces, nil
	}

	return []WorkspaceRef{fallbackWorkspace(auth, claims)}, nil
}

func (p *codexProvider) FetchWorkspaceSnapshot(ctx context.Context, auth *CodexAuthContext, ws WorkspaceRef) (*RateLimitSnapshot, error) {
	if auth == nil {
		return nil, fmt.Errorf("codex quota: auth context is nil")
	}
	if strings.TrimSpace(auth.AccessToken) == "" {
		return nil, fmt.Errorf("codex quota: access token is empty")
	}

	baseURL, pathStyle := normalizeCodexQuotaBaseURL(auth.BaseURL)
	usageURL := buildCodexUsageURL(baseURL, pathStyle)
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, usageURL, nil)
	if err != nil {
		return nil, fmt.Errorf("codex quota: create request failed: %w", err)
	}

	req.Header.Set("Authorization", "Bearer "+strings.TrimSpace(auth.AccessToken))
	req.Header.Set("User-Agent", codexQuotaUserAgent)
	if accountID := accountIDForWorkspace(auth, ws); accountID != "" {
		req.Header.Set("ChatGPT-Account-Id", accountID)
	}
	if cookieHeader := cookieHeaderValue(auth.Cookies); cookieHeader != "" {
		req.Header.Set("Cookie", cookieHeader)
	}

	client := p.newHTTPClient(auth.ProxyURL)
	resp, err := client.Do(req)
	if err != nil {
		return nil, fmt.Errorf("codex quota: usage request failed: %w", err)
	}
	defer func() {
		_ = resp.Body.Close()
	}()

	body, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, fmt.Errorf("codex quota: read usage response failed: %w", err)
	}
	if resp.StatusCode < http.StatusOK || resp.StatusCode >= http.StatusMultipleChoices {
		return nil, fmt.Errorf("codex quota: usage request returned %d: %s", resp.StatusCode, strings.TrimSpace(string(body)))
	}

	snapshot, err := NormalizeCodexUsage(body)
	if err != nil {
		return nil, fmt.Errorf("codex quota: normalize usage failed: %w", err)
	}
	return snapshot, nil
}

func (p *codexProvider) newHTTPClient(proxyURL string) *http.Client {
	client := &http.Client{Timeout: 30 * time.Second}

	proxyURL = strings.TrimSpace(proxyURL)
	if proxyURL == "" && p != nil && p.cfg != nil {
		proxyURL = strings.TrimSpace(p.cfg.ProxyURL)
	}
	transport, _, err := proxyutil.BuildHTTPTransport(proxyURL)
	if err == nil && transport != nil {
		client.Transport = transport
	}

	return client
}

func buildCodexAuthContext(cfg *config.Config, auth *coreauth.Auth) (*CodexAuthContext, bool) {
	if auth == nil || auth.Disabled {
		return nil, false
	}
	if !strings.EqualFold(strings.TrimSpace(auth.Provider), ProviderCodex) {
		return nil, false
	}
	if auth.Metadata == nil {
		return nil, false
	}
	if _, accountType := auth.AccountInfo(); strings.TrimSpace(accountType) != "" {
		kind, _ := auth.AccountInfo()
		if strings.EqualFold(strings.TrimSpace(kind), "api_key") {
			return nil, false
		}
	}
	if strings.TrimSpace(authAttributeString(auth, "api_key")) != "" {
		return nil, false
	}

	authCtx := &CodexAuthContext{
		AuthID:    strings.TrimSpace(auth.ID),
		AuthLabel: strings.TrimSpace(auth.Label),
		AuthNote: firstNonEmptyString(
			metadataString(auth.Metadata, "note"),
			authAttributeString(auth, "note"),
		),
		AccountEmail: firstNonEmptyString(
			metadataString(auth.Metadata, "email"),
			authAttributeString(auth, "email"),
			authAttributeString(auth, "account_email"),
		),
		AccountID: firstNonEmptyString(
			metadataString(auth.Metadata, "account_id"),
			authAttributeString(auth, "account_id"),
		),
		AccessToken: firstNonEmptyString(
			metadataString(auth.Metadata, "access_token"),
			authAttributeString(auth, "access_token"),
		),
		RefreshToken: firstNonEmptyString(
			metadataString(auth.Metadata, "refresh_token"),
			authAttributeString(auth, "refresh_token"),
		),
		IDToken: firstNonEmptyString(
			metadataString(auth.Metadata, "id_token"),
			authAttributeString(auth, "id_token"),
		),
		BaseURL: strings.TrimSpace(authAttributeString(auth, "base_url")),
		ProxyURL: firstNonEmptyString(
			strings.TrimSpace(auth.ProxyURL),
			authAttributeString(auth, "proxy_url"),
		),
		Cookies: metadataCookies(auth.Metadata),
	}

	if authCtx.BaseURL == "" && cfg != nil {
		authCtx.BaseURL = "https://chatgpt.com/backend-api"
	}
	if authCtx.AccessToken == "" {
		return nil, false
	}
	if authCtx.AuthID == "" {
		return nil, false
	}
	return authCtx, true
}

func authAttributeString(auth *coreauth.Auth, key string) string {
	if auth == nil || auth.Attributes == nil {
		return ""
	}
	return strings.TrimSpace(auth.Attributes[key])
}

func metadataString(metadata map[string]any, key string) string {
	if metadata == nil {
		return ""
	}
	value, ok := metadata[key]
	if !ok {
		return ""
	}
	switch typed := value.(type) {
	case string:
		return strings.TrimSpace(typed)
	case json.Number:
		return strings.TrimSpace(typed.String())
	default:
		return strings.TrimSpace(fmt.Sprint(typed))
	}
}

func metadataCookies(metadata map[string]any) map[string]string {
	if len(metadata) == 0 {
		return nil
	}

	if rawCookie := strings.TrimSpace(metadataString(metadata, "cookie")); rawCookie != "" {
		return parseCookieHeader(rawCookie)
	}
	if value, ok := metadata["cookies"]; ok {
		switch typed := value.(type) {
		case map[string]any:
			out := make(map[string]string, len(typed))
			for key, raw := range typed {
				out[strings.TrimSpace(key)] = strings.TrimSpace(fmt.Sprint(raw))
			}
			return out
		case map[string]string:
			out := make(map[string]string, len(typed))
			for key, raw := range typed {
				out[strings.TrimSpace(key)] = strings.TrimSpace(raw)
			}
			return out
		}
	}
	if rawCookie := strings.TrimSpace(metadataString(metadata, "cookies")); rawCookie != "" {
		return parseCookieHeader(rawCookie)
	}
	return nil
}

func parseCookieHeader(raw string) map[string]string {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return nil
	}

	out := make(map[string]string)
	parts := strings.Split(raw, ";")
	for _, part := range parts {
		pair := strings.SplitN(strings.TrimSpace(part), "=", 2)
		if len(pair) != 2 {
			continue
		}
		key := strings.TrimSpace(pair[0])
		value := strings.TrimSpace(pair[1])
		if key == "" || value == "" {
			continue
		}
		out[key] = value
	}
	if len(out) == 0 {
		return nil
	}
	return out
}

func cookieHeaderValue(cookies map[string]string) string {
	if len(cookies) == 0 {
		return ""
	}
	keys := make([]string, 0, len(cookies))
	for key := range cookies {
		if strings.TrimSpace(key) != "" && strings.TrimSpace(cookies[key]) != "" {
			keys = append(keys, key)
		}
	}
	sort.Strings(keys)
	parts := make([]string, 0, len(keys))
	for _, key := range keys {
		parts = append(parts, strings.TrimSpace(key)+"="+strings.TrimSpace(cookies[key]))
	}
	return strings.Join(parts, "; ")
}

func parseCodexClaims(idToken string) (*codexauth.JWTClaims, error) {
	idToken = strings.TrimSpace(idToken)
	if idToken == "" {
		return nil, fmt.Errorf("codex quota: id token is empty")
	}
	return codexauth.ParseJWTToken(idToken)
}

func workspacesFromClaims(claims *codexauth.JWTClaims) []WorkspaceRef {
	if claims == nil {
		return nil
	}

	planType := strings.TrimSpace(claims.CodexAuthInfo.ChatgptPlanType)
	workspaces := make([]WorkspaceRef, 0, len(claims.CodexAuthInfo.Organizations))
	for _, org := range claims.CodexAuthInfo.Organizations {
		if strings.TrimSpace(org.ID) == "" {
			continue
		}
		name := strings.TrimSpace(org.Title)
		if name == "" {
			name = strings.TrimSpace(org.ID)
		}
		workspaces = append(workspaces, WorkspaceRef{
			ID:   strings.TrimSpace(org.ID),
			Name: name,
			Type: classifyWorkspaceType(planType, true),
		})
	}

	sort.SliceStable(workspaces, func(i, j int) bool {
		if strings.EqualFold(workspaces[i].ID, workspaces[j].ID) {
			return workspaces[i].Name < workspaces[j].Name
		}
		return workspaces[i].Name < workspaces[j].Name
	})
	return uniqueWorkspaces(workspaces)
}

func selectCurrentWorkspace(auth *CodexAuthContext) WorkspaceRef {
	claims, _ := parseCodexClaims(auth.IDToken)
	workspaces := workspacesFromClaims(claims)
	accountID := strings.TrimSpace(auth.AccountID)
	for _, ws := range workspaces {
		if accountID != "" && ws.ID == accountID {
			return ws
		}
	}
	return fallbackWorkspace(auth, claims)
}

func fallbackWorkspace(auth *CodexAuthContext, claims *codexauth.JWTClaims) WorkspaceRef {
	planType := ""
	if claims != nil {
		planType = strings.TrimSpace(claims.CodexAuthInfo.ChatgptPlanType)
	}
	wsType := classifyWorkspaceType(planType, false)
	workspaceID := strings.TrimSpace(auth.AccountID)
	if workspaceID == "" {
		workspaceID = DefaultWorkspaceID
	}
	workspaceName := "Current Workspace"
	if wsType == "personal" && strings.TrimSpace(auth.AccountEmail) != "" {
		workspaceName = strings.TrimSpace(auth.AccountEmail)
	}
	return WorkspaceRef{
		ID:   workspaceID,
		Name: workspaceName,
		Type: wsType,
	}
}

func uniqueWorkspaces(workspaces []WorkspaceRef) []WorkspaceRef {
	if len(workspaces) == 0 {
		return nil
	}
	seen := make(map[string]struct{}, len(workspaces))
	out := make([]WorkspaceRef, 0, len(workspaces))
	for _, ws := range workspaces {
		if strings.TrimSpace(ws.ID) == "" {
			continue
		}
		if _, ok := seen[ws.ID]; ok {
			continue
		}
		seen[ws.ID] = struct{}{}
		out = append(out, ws)
	}
	return out
}

func classifyWorkspaceType(planType string, hasOrganization bool) string {
	plan := strings.ToLower(strings.TrimSpace(planType))
	switch {
	case strings.Contains(plan, "enterprise"):
		return "enterprise"
	case strings.Contains(plan, "edu"), strings.Contains(plan, "education"), strings.Contains(plan, "k12"), strings.Contains(plan, "quorum"):
		return "edu"
	case strings.Contains(plan, "business"), strings.Contains(plan, "team"), strings.Contains(plan, "workspace"):
		return "business"
	case hasOrganization:
		return "business"
	case plan == "", strings.Contains(plan, "free"), strings.Contains(plan, "plus"), strings.Contains(plan, "pro"), strings.Contains(plan, "go"), strings.Contains(plan, "guest"):
		return "personal"
	default:
		return "unknown"
	}
}

type codexQuotaPathStyle int

const (
	codexQuotaPathStyleCodexAPI codexQuotaPathStyle = iota
	codexQuotaPathStyleChatGPTAPI
)

func normalizeCodexQuotaBaseURL(raw string) (string, codexQuotaPathStyle) {
	baseURL := strings.TrimSpace(raw)
	if baseURL == "" {
		baseURL = "https://chatgpt.com/backend-api"
	}
	baseURL = strings.TrimRight(baseURL, "/")
	baseURL = strings.TrimSuffix(baseURL, "/codex")
	baseURL = strings.TrimSuffix(baseURL, "/api/codex")

	if (strings.HasPrefix(baseURL, "https://chatgpt.com") || strings.HasPrefix(baseURL, "https://chat.openai.com")) &&
		!strings.Contains(baseURL, "/backend-api") {
		baseURL += "/backend-api"
	}
	if strings.Contains(baseURL, "/backend-api") {
		return baseURL, codexQuotaPathStyleChatGPTAPI
	}
	return baseURL, codexQuotaPathStyleCodexAPI
}

func buildCodexUsageURL(baseURL string, pathStyle codexQuotaPathStyle) string {
	switch pathStyle {
	case codexQuotaPathStyleChatGPTAPI:
		return strings.TrimRight(baseURL, "/") + "/wham/usage"
	default:
		return strings.TrimRight(baseURL, "/") + "/api/codex/usage"
	}
}

func accountIDForWorkspace(auth *CodexAuthContext, ws WorkspaceRef) string {
	workspaceID := strings.TrimSpace(ws.ID)
	if workspaceID != "" && workspaceID != DefaultWorkspaceID {
		return workspaceID
	}
	return strings.TrimSpace(auth.AccountID)
}

func firstNonEmptyString(values ...string) string {
	for _, value := range values {
		if trimmed := strings.TrimSpace(value); trimmed != "" {
			return trimmed
		}
	}
	return ""
}
