package quota

import (
	"sort"
	"strings"
	"sync"
)

type SnapshotCache struct {
	mu    sync.RWMutex
	items map[string]*CodexQuotaSnapshotEnvelope
}

func NewSnapshotCache() *SnapshotCache {
	return &SnapshotCache{
		items: make(map[string]*CodexQuotaSnapshotEnvelope),
	}
}

func (c *SnapshotCache) Upsert(snapshot *CodexQuotaSnapshotEnvelope) {
	if c == nil || snapshot == nil {
		return
	}

	entry := snapshot.Clone()
	entry.Provider = normalizeProvider(entry.Provider)

	c.mu.Lock()
	defer c.mu.Unlock()

	if c.items == nil {
		c.items = make(map[string]*CodexQuotaSnapshotEnvelope)
	}
	c.items[snapshotCacheKey(entry.AuthID, entry.WorkspaceID)] = entry
}

func (c *SnapshotCache) Get(authID, workspaceID string) (*CodexQuotaSnapshotEnvelope, bool) {
	if c == nil {
		return nil, false
	}

	c.mu.RLock()
	defer c.mu.RUnlock()

	if c.items == nil {
		return nil, false
	}

	entry, ok := c.items[snapshotCacheKey(authID, workspaceID)]
	if !ok {
		return nil, false
	}
	return entry.Clone(), true
}

func (c *SnapshotCache) List(authID, workspaceID string) []*CodexQuotaSnapshotEnvelope {
	if c == nil {
		return nil
	}

	authID = strings.TrimSpace(authID)
	workspaceID = strings.TrimSpace(workspaceID)

	c.mu.RLock()
	defer c.mu.RUnlock()

	if len(c.items) == 0 {
		return nil
	}

	list := make([]*CodexQuotaSnapshotEnvelope, 0, len(c.items))
	for _, entry := range c.items {
		if authID != "" && entry.AuthID != authID {
			continue
		}
		if workspaceID != "" && entry.WorkspaceID != workspaceID {
			continue
		}
		list = append(list, entry.Clone())
	}

	sort.Slice(list, func(i, j int) bool {
		if list[i].AuthID != list[j].AuthID {
			return list[i].AuthID < list[j].AuthID
		}
		return list[i].WorkspaceID < list[j].WorkspaceID
	})

	return list
}

func (c *SnapshotCache) DeleteAuth(authID string) {
	if c == nil {
		return
	}

	authID = strings.TrimSpace(authID)
	if authID == "" {
		return
	}

	c.mu.Lock()
	defer c.mu.Unlock()

	for key, entry := range c.items {
		if entry == nil || strings.TrimSpace(entry.AuthID) == authID {
			delete(c.items, key)
		}
	}
}

func (c *SnapshotCache) RetainAuthIDs(authIDs []string) {
	if c == nil {
		return
	}

	allowed := make(map[string]struct{}, len(authIDs))
	for _, authID := range authIDs {
		if trimmed := strings.TrimSpace(authID); trimmed != "" {
			allowed[trimmed] = struct{}{}
		}
	}

	c.mu.Lock()
	defer c.mu.Unlock()

	for key, entry := range c.items {
		if entry == nil {
			delete(c.items, key)
			continue
		}
		if _, ok := allowed[strings.TrimSpace(entry.AuthID)]; !ok {
			delete(c.items, key)
		}
	}
}

func snapshotCacheKey(authID, workspaceID string) string {
	return ProviderCodex + ":" + strings.TrimSpace(authID) + ":" + strings.TrimSpace(workspaceID)
}
