package quota

import (
	"context"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"github.com/router-for-me/CLIProxyAPI/v6/internal/config"
	coreauth "github.com/router-for-me/CLIProxyAPI/v6/sdk/cliproxy/auth"
	"golang.org/x/sync/errgroup"
)

const (
	codexQuotaAuthConcurrency      = 4
	codexQuotaWorkspaceConcurrency = 2
)

var defaultCodexQuotaService atomic.Pointer[CodexQuotaService]

type CodexQuotaService struct {
	mu     sync.RWMutex
	cache  *SnapshotCache
	source CodexQuotaSource
	auths  AuthEnumerator
}

func NewCodexQuotaService(cfg *config.Config, authManager *coreauth.Manager) *CodexQuotaService {
	service := NewCodexQuotaServiceWithDeps(NewSnapshotCache(), newCodexProvider(cfg), newCodexAuthEnumerator(cfg, authManager))
	SetDefaultCodexQuotaService(service)
	return service
}

func NewCodexQuotaServiceWithDeps(cache *SnapshotCache, source CodexQuotaSource, auths AuthEnumerator) *CodexQuotaService {
	if cache == nil {
		cache = NewSnapshotCache()
	}
	return &CodexQuotaService{
		cache:  cache,
		source: source,
		auths:  auths,
	}
}

func DefaultCodexQuotaService() *CodexQuotaService {
	return defaultCodexQuotaService.Load()
}

func SetDefaultCodexQuotaService(service *CodexQuotaService) {
	defaultCodexQuotaService.Store(service)
}

func (s *CodexQuotaService) SetConfig(cfg *config.Config) {
	if s == nil {
		return
	}

	s.mu.Lock()
	defer s.mu.Unlock()

	if setter, ok := s.source.(interface{ SetConfig(*config.Config) }); ok {
		setter.SetConfig(cfg)
	}
	if setter, ok := s.auths.(interface{ SetConfig(*config.Config) }); ok {
		setter.SetConfig(cfg)
	}
}

func (s *CodexQuotaService) SetAuthManager(authManager *coreauth.Manager) {
	if s == nil {
		return
	}

	s.mu.Lock()
	defer s.mu.Unlock()

	if setter, ok := s.auths.(interface{ SetAuthManager(*coreauth.Manager) }); ok {
		setter.SetAuthManager(authManager)
	}
}

func (s *CodexQuotaService) RefreshAll(ctx context.Context) error {
	_, err := s.RefreshWithOptions(ctx, RefreshOptions{})
	return err
}

func (s *CodexQuotaService) ListSnapshots(ctx context.Context, refresh bool) ([]*CodexQuotaSnapshotEnvelope, error) {
	return s.ListSnapshotsWithOptions(ctx, ListOptions{Refresh: refresh})
}

func (s *CodexQuotaService) ListSnapshotsWithOptions(ctx context.Context, options ListOptions) ([]*CodexQuotaSnapshotEnvelope, error) {
	if s == nil {
		return nil, nil
	}
	if ctx == nil {
		ctx = context.Background()
	}

	options.AuthID = NormalizeOptionsAuthID(options.AuthID)
	options.WorkspaceID = NormalizeOptionsWorkspaceID(options.WorkspaceID)

	if options.Refresh {
		if _, err := s.RefreshWithOptions(ctx, RefreshOptions{
			AuthID:      options.AuthID,
			WorkspaceID: options.WorkspaceID,
		}); err != nil {
			return nil, err
		}
	}

	if auths := s.authEnumerator(); auths != nil {
		if authContexts, err := auths.ListCodexAuths(ctx); err == nil {
			s.syncCacheWithCurrentAuths(authContexts, options.AuthID)
			snapshots := s.cache.List(options.AuthID, options.WorkspaceID)
			applyCurrentAuthMetadata(snapshots, authContexts)
			return snapshots, nil
		}
	}
	return s.cache.List(options.AuthID, options.WorkspaceID), nil
}

func (s *CodexQuotaService) RefreshWithOptions(ctx context.Context, options RefreshOptions) ([]*CodexQuotaSnapshotEnvelope, error) {
	if s == nil {
		return nil, nil
	}
	if ctx == nil {
		ctx = context.Background()
	}

	options.AuthID = NormalizeOptionsAuthID(options.AuthID)
	options.WorkspaceID = NormalizeOptionsWorkspaceID(options.WorkspaceID)

	auths := s.authEnumerator()
	if auths == nil {
		return s.cache.List(options.AuthID, options.WorkspaceID), nil
	}

	authContexts, err := auths.ListCodexAuths(ctx)
	if err != nil {
		return nil, err
	}
	s.syncCacheWithCurrentAuths(authContexts, options.AuthID)

	group, groupCtx := errgroup.WithContext(ctx)
	group.SetLimit(codexQuotaAuthConcurrency)
	for _, auth := range authContexts {
		if auth == nil {
			continue
		}
		auth = auth.Clone()
		if options.AuthID != "" && auth.AuthID != options.AuthID {
			continue
		}
		group.Go(func() error {
			s.refreshAuth(groupCtx, auth, options.WorkspaceID)
			return nil
		})
	}
	if err := group.Wait(); err != nil {
		return nil, err
	}

	return s.cache.List(options.AuthID, options.WorkspaceID), nil
}

func (s *CodexQuotaService) RefreshAsync(authID, workspaceID string) {
	if s == nil {
		return
	}
	go func() {
		_, _ = s.RefreshWithOptions(context.Background(), RefreshOptions{
			AuthID:      authID,
			WorkspaceID: workspaceID,
		})
	}()
}

func (s *CodexQuotaService) CaptureInlineRateLimitEvent(auth *coreauth.Auth, raw any) bool {
	if s == nil {
		return false
	}
	snapshot, err := NormalizeCodexRateLimitEvent(raw)
	if err != nil || snapshot == nil {
		return false
	}
	return s.CaptureInlineSnapshot(auth, WorkspaceRef{}, snapshot)
}

func (s *CodexQuotaService) CaptureInlineSnapshot(auth *coreauth.Auth, workspace WorkspaceRef, snapshot *RateLimitSnapshot) bool {
	if s == nil || snapshot == nil {
		return false
	}

	authCtx, ok := buildCodexAuthContext(nil, auth)
	if !ok || authCtx == nil {
		return false
	}
	if strings.TrimSpace(workspace.ID) == "" {
		workspace = selectCurrentWorkspace(authCtx)
	}
	s.upsertSnapshot(authCtx, workspace, snapshot, SourceInlineRateLimits, false, "", time.Now().UTC())
	return true
}

func (s *CodexQuotaService) refreshAuth(ctx context.Context, auth *CodexAuthContext, requestedWorkspaceID string) {
	if auth == nil {
		return
	}

	source := s.snapshotSource()
	if source == nil {
		return
	}

	workspaces, err := source.ListWorkspaces(ctx, auth)
	if err != nil {
		s.markRefreshFailure(auth, workspaceForFailure(auth, requestedWorkspaceID, s.cache), err)
		return
	}

	targets := filterTargetWorkspaces(auth, workspaces, requestedWorkspaceID, s.cache)
	if len(targets) == 0 {
		targets = []WorkspaceRef{workspaceForFailure(auth, requestedWorkspaceID, s.cache)}
	}

	group, groupCtx := errgroup.WithContext(ctx)
	group.SetLimit(codexQuotaWorkspaceConcurrency)
	for _, ws := range targets {
		ws := normalizeWorkspaceRef(auth, ws)
		group.Go(func() error {
			snapshot, err := source.FetchWorkspaceSnapshot(groupCtx, auth, ws)
			if err != nil {
				s.markRefreshFailure(auth, ws, err)
				return nil
			}
			s.upsertSnapshot(auth, ws, snapshot, SourceUsageDashboard, false, "", time.Now().UTC())
			return nil
		})
	}
	_ = group.Wait()
}

func (s *CodexQuotaService) markRefreshFailure(auth *CodexAuthContext, ws WorkspaceRef, err error) {
	ws = normalizeWorkspaceRef(auth, ws)
	existing, ok := s.cache.Get(auth.AuthID, ws.ID)
	if ok && existing != nil {
		existing.Stale = true
		existing.Error = errorString(err)
		existing.AuthLabel = strings.TrimSpace(auth.AuthLabel)
		existing.AuthNote = strings.TrimSpace(auth.AuthNote)
		existing.AccountEmail = firstNonEmptyString(auth.AccountEmail, existing.AccountEmail)
		existing.WorkspaceName = firstNonEmptyString(ws.Name, existing.WorkspaceName)
		existing.WorkspaceType = firstNonEmptyString(ws.Type, existing.WorkspaceType)
		existing.Provider = normalizeProvider(existing.Provider)
		if existing.Source == "" {
			existing.Source = SourceUsageDashboard
		}
		s.cache.Upsert(existing)
		return
	}

	envelope := &CodexQuotaSnapshotEnvelope{
		Provider:      ProviderCodex,
		AuthID:        auth.AuthID,
		AuthLabel:     auth.AuthLabel,
		AuthNote:      auth.AuthNote,
		AccountEmail:  auth.AccountEmail,
		WorkspaceID:   ws.ID,
		WorkspaceName: ws.Name,
		WorkspaceType: ws.Type,
		Source:        SourceUsageDashboard,
		FetchedAt:     time.Now().UTC(),
		Stale:         true,
		Error:         errorString(err),
	}
	s.cache.Upsert(envelope)
}

func (s *CodexQuotaService) upsertSnapshot(auth *CodexAuthContext, ws WorkspaceRef, snapshot *RateLimitSnapshot, source string, stale bool, errMsg string, fetchedAt time.Time) {
	if auth == nil {
		return
	}
	ws = normalizeWorkspaceRef(auth, ws)
	if fetchedAt.IsZero() {
		fetchedAt = time.Now().UTC()
	}

	envelope := &CodexQuotaSnapshotEnvelope{
		Provider:      ProviderCodex,
		AuthID:        auth.AuthID,
		AuthLabel:     auth.AuthLabel,
		AuthNote:      auth.AuthNote,
		AccountEmail:  auth.AccountEmail,
		WorkspaceID:   ws.ID,
		WorkspaceName: ws.Name,
		WorkspaceType: ws.Type,
		Snapshot:      snapshot.Clone(),
		Source:        strings.TrimSpace(source),
		FetchedAt:     fetchedAt.UTC(),
		Stale:         stale,
		Error:         strings.TrimSpace(errMsg),
	}
	if envelope.Source == "" {
		envelope.Source = SourceUsageDashboard
	}
	s.cache.Upsert(envelope)
}

func (s *CodexQuotaService) snapshotSource() CodexQuotaSource {
	s.mu.RLock()
	defer s.mu.RUnlock()
	return s.source
}

func (s *CodexQuotaService) authEnumerator() AuthEnumerator {
	s.mu.RLock()
	defer s.mu.RUnlock()
	return s.auths
}

func filterTargetWorkspaces(auth *CodexAuthContext, workspaces []WorkspaceRef, requestedWorkspaceID string, cache *SnapshotCache) []WorkspaceRef {
	if len(workspaces) == 0 {
		if strings.TrimSpace(requestedWorkspaceID) == "" {
			return nil
		}
		return []WorkspaceRef{workspaceForFailure(auth, requestedWorkspaceID, cache)}
	}

	requestedWorkspaceID = strings.TrimSpace(requestedWorkspaceID)
	if requestedWorkspaceID == "" {
		out := make([]WorkspaceRef, 0, len(workspaces))
		for _, ws := range workspaces {
			out = append(out, normalizeWorkspaceRef(auth, ws))
		}
		return out
	}

	out := make([]WorkspaceRef, 0, 1)
	for _, ws := range workspaces {
		if strings.TrimSpace(ws.ID) == requestedWorkspaceID {
			out = append(out, normalizeWorkspaceRef(auth, ws))
		}
	}
	if len(out) > 0 {
		return out
	}

	return []WorkspaceRef{workspaceForFailure(auth, requestedWorkspaceID, cache)}
}

func workspaceForFailure(auth *CodexAuthContext, requestedWorkspaceID string, cache *SnapshotCache) WorkspaceRef {
	requestedWorkspaceID = strings.TrimSpace(requestedWorkspaceID)
	if requestedWorkspaceID == "" {
		return selectCurrentWorkspace(auth)
	}
	if cache != nil {
		if existing, ok := cache.Get(auth.AuthID, requestedWorkspaceID); ok && existing != nil {
			return WorkspaceRef{
				ID:   firstNonEmptyString(existing.WorkspaceID, requestedWorkspaceID),
				Name: existing.WorkspaceName,
				Type: existing.WorkspaceType,
			}
		}
	}
	ws := selectCurrentWorkspace(auth)
	ws.ID = requestedWorkspaceID
	if strings.TrimSpace(ws.Name) == "" {
		ws.Name = requestedWorkspaceID
	}
	return ws
}

func normalizeWorkspaceRef(auth *CodexAuthContext, ws WorkspaceRef) WorkspaceRef {
	ws.ID = strings.TrimSpace(ws.ID)
	ws.Name = strings.TrimSpace(ws.Name)
	ws.Type = strings.TrimSpace(ws.Type)
	if ws.ID == "" {
		fallback := selectCurrentWorkspace(auth)
		ws.ID = fallback.ID
		if ws.Name == "" {
			ws.Name = fallback.Name
		}
		if ws.Type == "" {
			ws.Type = fallback.Type
		}
	}
	if ws.Name == "" {
		ws.Name = ws.ID
	}
	if ws.Type == "" {
		ws.Type = "unknown"
	}
	return ws
}

func errorString(err error) string {
	if err == nil {
		return ""
	}
	return strings.TrimSpace(err.Error())
}

func applyCurrentAuthMetadata(snapshots []*CodexQuotaSnapshotEnvelope, auths []*CodexAuthContext) {
	if len(snapshots) == 0 || len(auths) == 0 {
		return
	}

	authByID := make(map[string]*CodexAuthContext, len(auths))
	for _, auth := range auths {
		if auth == nil || strings.TrimSpace(auth.AuthID) == "" {
			continue
		}
		authByID[strings.TrimSpace(auth.AuthID)] = auth
	}

	for _, snapshot := range snapshots {
		if snapshot == nil {
			continue
		}
		auth, ok := authByID[strings.TrimSpace(snapshot.AuthID)]
		if !ok || auth == nil {
			continue
		}
		snapshot.AuthLabel = strings.TrimSpace(auth.AuthLabel)
		snapshot.AuthNote = strings.TrimSpace(auth.AuthNote)
		snapshot.AccountEmail = firstNonEmptyString(auth.AccountEmail, snapshot.AccountEmail)
	}
}

func (s *CodexQuotaService) syncCacheWithCurrentAuths(auths []*CodexAuthContext, requestedAuthID string) {
	if s == nil || s.cache == nil {
		return
	}

	requestedAuthID = strings.TrimSpace(requestedAuthID)
	if requestedAuthID != "" {
		for _, auth := range auths {
			if auth != nil && strings.TrimSpace(auth.AuthID) == requestedAuthID {
				return
			}
		}
		s.cache.DeleteAuth(requestedAuthID)
		return
	}

	authIDs := make([]string, 0, len(auths))
	for _, auth := range auths {
		if auth == nil {
			continue
		}
		if authID := strings.TrimSpace(auth.AuthID); authID != "" {
			authIDs = append(authIDs, authID)
		}
	}
	s.cache.RetainAuthIDs(authIDs)
}
