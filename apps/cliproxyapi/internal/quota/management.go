package quota

import "strings"

type ListOptions struct {
	Refresh     bool
	AuthID      string
	WorkspaceID string
}

type RefreshOptions struct {
	AuthID      string
	WorkspaceID string
}

type SnapshotListResponse struct {
	Provider  string                       `json:"provider"`
	Snapshots []*CodexQuotaSnapshotEnvelope `json:"snapshots"`
}

func NewSnapshotListResponse(snapshots []*CodexQuotaSnapshotEnvelope) SnapshotListResponse {
	if snapshots == nil {
		snapshots = make([]*CodexQuotaSnapshotEnvelope, 0)
	}
	return SnapshotListResponse{
		Provider:  ProviderCodex,
		Snapshots: snapshots,
	}
}

func NormalizeOptionsAuthID(value string) string {
	return strings.TrimSpace(value)
}

func NormalizeOptionsWorkspaceID(value string) string {
	return strings.TrimSpace(value)
}
