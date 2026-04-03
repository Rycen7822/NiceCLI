package quota

import (
	"context"
	"strings"
	"time"
)

const (
	ProviderCodex          = "codex"
	CodexPrimaryLimitID    = "codex"
	SourceInlineRateLimits = "inline_rate_limits"
	SourceUsageDashboard   = "usage_dashboard"
	DefaultWorkspaceID     = "current"
)

type RateLimitWindow struct {
	UsedPercent   float64 `json:"used_percent"`
	WindowMinutes *int64  `json:"window_minutes,omitempty"`
	ResetsAt      *int64  `json:"resets_at,omitempty"`
}

func (w *RateLimitWindow) Clone() *RateLimitWindow {
	if w == nil {
		return nil
	}
	copyWindow := *w
	if w.WindowMinutes != nil {
		value := *w.WindowMinutes
		copyWindow.WindowMinutes = &value
	}
	if w.ResetsAt != nil {
		value := *w.ResetsAt
		copyWindow.ResetsAt = &value
	}
	return &copyWindow
}

type CreditsSnapshot struct {
	HasCredits bool    `json:"has_credits"`
	Unlimited  bool    `json:"unlimited"`
	Balance    *string `json:"balance,omitempty"`
}

func (c *CreditsSnapshot) Clone() *CreditsSnapshot {
	if c == nil {
		return nil
	}
	copyCredits := *c
	if c.Balance != nil {
		value := *c.Balance
		copyCredits.Balance = &value
	}
	return &copyCredits
}

type RateLimitSnapshot struct {
	LimitID   *string          `json:"limit_id,omitempty"`
	LimitName *string          `json:"limit_name,omitempty"`
	Primary   *RateLimitWindow `json:"primary,omitempty"`
	Secondary *RateLimitWindow `json:"secondary,omitempty"`
	Credits   *CreditsSnapshot `json:"credits,omitempty"`
	PlanType  *string          `json:"plan_type,omitempty"`
}

func (s *RateLimitSnapshot) Clone() *RateLimitSnapshot {
	if s == nil {
		return nil
	}
	copySnapshot := *s
	if s.LimitID != nil {
		value := *s.LimitID
		copySnapshot.LimitID = &value
	}
	if s.LimitName != nil {
		value := *s.LimitName
		copySnapshot.LimitName = &value
	}
	copySnapshot.Primary = s.Primary.Clone()
	copySnapshot.Secondary = s.Secondary.Clone()
	copySnapshot.Credits = s.Credits.Clone()
	if s.PlanType != nil {
		value := *s.PlanType
		copySnapshot.PlanType = &value
	}
	return &copySnapshot
}

type CodexQuotaSnapshotEnvelope struct {
	Provider      string             `json:"provider"`
	AuthID        string             `json:"auth_id"`
	AuthLabel     string             `json:"auth_label,omitempty"`
	AuthNote      string             `json:"auth_note,omitempty"`
	AccountEmail  string             `json:"account_email,omitempty"`
	WorkspaceID   string             `json:"workspace_id,omitempty"`
	WorkspaceName string             `json:"workspace_name,omitempty"`
	WorkspaceType string             `json:"workspace_type,omitempty"`
	Snapshot      *RateLimitSnapshot `json:"snapshot,omitempty"`
	Source        string             `json:"source"`
	FetchedAt     time.Time          `json:"fetched_at"`
	Stale         bool               `json:"stale"`
	Error         string             `json:"error,omitempty"`
}

func (e *CodexQuotaSnapshotEnvelope) Clone() *CodexQuotaSnapshotEnvelope {
	if e == nil {
		return nil
	}
	copyEnvelope := *e
	copyEnvelope.Provider = normalizeProvider(copyEnvelope.Provider)
	copyEnvelope.Snapshot = e.Snapshot.Clone()
	return &copyEnvelope
}

type WorkspaceRef struct {
	ID   string `json:"id"`
	Name string `json:"name"`
	Type string `json:"type"`
}

type CodexAuthContext struct {
	AuthID       string
	AuthLabel    string
	AuthNote     string
	AccountEmail string
	AccountID    string
	Cookies      map[string]string
	AccessToken  string
	RefreshToken string
	IDToken      string
	BaseURL      string
	ProxyURL     string
}

func (c *CodexAuthContext) Clone() *CodexAuthContext {
	if c == nil {
		return nil
	}
	copyContext := *c
	if len(c.Cookies) > 0 {
		copyContext.Cookies = make(map[string]string, len(c.Cookies))
		for key, value := range c.Cookies {
			copyContext.Cookies[key] = value
		}
	}
	return &copyContext
}

type CodexQuotaSource interface {
	ListWorkspaces(ctx context.Context, auth *CodexAuthContext) ([]WorkspaceRef, error)
	FetchWorkspaceSnapshot(ctx context.Context, auth *CodexAuthContext, ws WorkspaceRef) (*RateLimitSnapshot, error)
}

type AuthEnumerator interface {
	ListCodexAuths(ctx context.Context) ([]*CodexAuthContext, error)
}

func normalizeProvider(provider string) string {
	provider = strings.TrimSpace(provider)
	if provider == "" {
		return ProviderCodex
	}
	return provider
}
