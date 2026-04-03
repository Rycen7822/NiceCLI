package quota

import (
	"testing"
	"time"
)

func TestSnapshotCacheUpsertAndGetReturnsClone(t *testing.T) {
	cache := NewSnapshotCache()
	cache.Upsert(&CodexQuotaSnapshotEnvelope{
		AuthID:      "auth-b",
		WorkspaceID: "ws-1",
		AuthLabel:   "Primary Auth",
		AuthNote:    "Prod Workspace",
		Snapshot: &RateLimitSnapshot{
			LimitID: stringPointer(CodexPrimaryLimitID),
			Primary: &RateLimitWindow{
				UsedPercent:   42,
				WindowMinutes: int64Pointer(60),
				ResetsAt:      int64Pointer(123),
			},
		},
		FetchedAt: time.Unix(100, 0).UTC(),
	})

	got, ok := cache.Get("auth-b", "ws-1")
	if !ok {
		t.Fatal("expected snapshot to be present")
	}
	if got.Provider != ProviderCodex {
		t.Fatalf("expected provider %q, got %q", ProviderCodex, got.Provider)
	}
	if got.Snapshot == nil || got.Snapshot.Primary == nil {
		t.Fatal("expected nested snapshot data")
	}

	got.AuthLabel = "mutated"
	got.AuthNote = "changed"
	got.Snapshot.Primary.UsedPercent = 99

	again, ok := cache.Get("auth-b", "ws-1")
	if !ok {
		t.Fatal("expected snapshot to remain present")
	}
	if again.AuthLabel != "Primary Auth" {
		t.Fatalf("expected cached auth label to remain unchanged, got %q", again.AuthLabel)
	}
	if again.AuthNote != "Prod Workspace" {
		t.Fatalf("expected cached auth note to remain unchanged, got %q", again.AuthNote)
	}
	if again.Snapshot.Primary.UsedPercent != 42 {
		t.Fatalf("expected cached used percent to remain unchanged, got %v", again.Snapshot.Primary.UsedPercent)
	}
}

func TestSnapshotCacheUpsertOverwritesExistingEntry(t *testing.T) {
	cache := NewSnapshotCache()
	cache.Upsert(&CodexQuotaSnapshotEnvelope{
		AuthID:      "auth-a",
		WorkspaceID: "ws-1",
		Snapshot: &RateLimitSnapshot{
			PlanType: stringPointer("plus"),
		},
	})
	cache.Upsert(&CodexQuotaSnapshotEnvelope{
		AuthID:      "auth-a",
		WorkspaceID: "ws-1",
		Snapshot: &RateLimitSnapshot{
			PlanType: stringPointer("pro"),
		},
	})

	got, ok := cache.Get("auth-a", "ws-1")
	if !ok {
		t.Fatal("expected overwritten snapshot to exist")
	}
	if got.Snapshot == nil || got.Snapshot.PlanType == nil || *got.Snapshot.PlanType != "pro" {
		t.Fatalf("expected overwritten plan type %q, got %+v", "pro", got.Snapshot)
	}
}

func TestSnapshotCacheListSupportsFilteringAndSorting(t *testing.T) {
	cache := NewSnapshotCache()
	cache.Upsert(&CodexQuotaSnapshotEnvelope{AuthID: "auth-b", WorkspaceID: "ws-2"})
	cache.Upsert(&CodexQuotaSnapshotEnvelope{AuthID: "auth-a", WorkspaceID: "ws-3"})
	cache.Upsert(&CodexQuotaSnapshotEnvelope{AuthID: "auth-a", WorkspaceID: "ws-1"})

	all := cache.List("", "")
	if len(all) != 3 {
		t.Fatalf("expected 3 snapshots, got %d", len(all))
	}
	if all[0].AuthID != "auth-a" || all[0].WorkspaceID != "ws-1" {
		t.Fatalf("expected first sorted snapshot to be auth-a/ws-1, got %s/%s", all[0].AuthID, all[0].WorkspaceID)
	}
	if all[1].AuthID != "auth-a" || all[1].WorkspaceID != "ws-3" {
		t.Fatalf("expected second sorted snapshot to be auth-a/ws-3, got %s/%s", all[1].AuthID, all[1].WorkspaceID)
	}
	if all[2].AuthID != "auth-b" || all[2].WorkspaceID != "ws-2" {
		t.Fatalf("expected third sorted snapshot to be auth-b/ws-2, got %s/%s", all[2].AuthID, all[2].WorkspaceID)
	}

	filteredByAuth := cache.List("auth-a", "")
	if len(filteredByAuth) != 2 {
		t.Fatalf("expected 2 snapshots for auth-a, got %d", len(filteredByAuth))
	}

	filteredByWorkspace := cache.List("", "ws-2")
	if len(filteredByWorkspace) != 1 || filteredByWorkspace[0].AuthID != "auth-b" {
		t.Fatalf("expected ws-2 filter to return auth-b, got %+v", filteredByWorkspace)
	}

	filteredExact := cache.List("auth-a", "ws-3")
	if len(filteredExact) != 1 || filteredExact[0].WorkspaceID != "ws-3" {
		t.Fatalf("expected exact filter auth-a/ws-3, got %+v", filteredExact)
	}
}

func TestSnapshotCacheDeleteAuthAndRetainAuthIDs(t *testing.T) {
	cache := NewSnapshotCache()
	cache.Upsert(&CodexQuotaSnapshotEnvelope{AuthID: "auth-a", WorkspaceID: "ws-1"})
	cache.Upsert(&CodexQuotaSnapshotEnvelope{AuthID: "auth-b", WorkspaceID: "ws-2"})
	cache.Upsert(&CodexQuotaSnapshotEnvelope{AuthID: "auth-c", WorkspaceID: "ws-3"})

	cache.DeleteAuth("auth-b")
	afterDelete := cache.List("", "")
	if len(afterDelete) != 2 {
		t.Fatalf("expected 2 snapshots after delete, got %d", len(afterDelete))
	}
	for _, snapshot := range afterDelete {
		if snapshot.AuthID == "auth-b" {
			t.Fatalf("expected auth-b to be removed, got %+v", snapshot)
		}
	}

	cache.RetainAuthIDs([]string{"auth-c"})
	afterRetain := cache.List("", "")
	if len(afterRetain) != 1 {
		t.Fatalf("expected 1 snapshot after retain, got %d", len(afterRetain))
	}
	if afterRetain[0].AuthID != "auth-c" {
		t.Fatalf("expected auth-c to remain, got %+v", afterRetain[0])
	}
}

func int64Pointer(value int64) *int64 {
	return &value
}
