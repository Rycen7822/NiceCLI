package quota

import "testing"

func TestNormalizeCodexUsageMapsPrimarySecondaryCreditsAndPlanType(t *testing.T) {
	raw := []byte(`{
		"plan_type": "pro",
		"rate_limit": {
			"allowed": true,
			"limit_reached": false,
			"primary_window": {
				"used_percent": 42,
				"limit_window_seconds": 3600,
				"reset_after_seconds": 120,
				"reset_at": 1735689720
			},
			"secondary_window": {
				"used_percent": 5,
				"limit_window_seconds": 86400,
				"reset_after_seconds": 600,
				"reset_at": 1735776000
			}
		},
		"credits": {
			"has_credits": true,
			"unlimited": false,
			"balance": "5"
		}
	}`)

	snapshot, err := NormalizeCodexUsage(raw)
	if err != nil {
		t.Fatalf("NormalizeCodexUsage returned error: %v", err)
	}
	if snapshot == nil {
		t.Fatal("expected snapshot")
	}
	if snapshot.LimitID == nil || *snapshot.LimitID != CodexPrimaryLimitID {
		t.Fatalf("expected limit_id %q, got %+v", CodexPrimaryLimitID, snapshot.LimitID)
	}
	if snapshot.Primary == nil || snapshot.Primary.WindowMinutes == nil || *snapshot.Primary.WindowMinutes != 60 {
		t.Fatalf("expected primary window to map to 60 minutes, got %+v", snapshot.Primary)
	}
	if snapshot.Primary.ResetsAt == nil || *snapshot.Primary.ResetsAt != 1735689720 {
		t.Fatalf("expected primary reset timestamp 1735689720, got %+v", snapshot.Primary)
	}
	if snapshot.Secondary == nil || snapshot.Secondary.WindowMinutes == nil || *snapshot.Secondary.WindowMinutes != 1440 {
		t.Fatalf("expected secondary window to map to 1440 minutes, got %+v", snapshot.Secondary)
	}
	if snapshot.Credits == nil || !snapshot.Credits.HasCredits || snapshot.Credits.Unlimited {
		t.Fatalf("expected credits to map, got %+v", snapshot.Credits)
	}
	if snapshot.Credits.Balance == nil || *snapshot.Credits.Balance != "5" {
		t.Fatalf("expected balance to map to %q, got %+v", "5", snapshot.Credits.Balance)
	}
	if snapshot.PlanType == nil || *snapshot.PlanType != "pro" {
		t.Fatalf("expected plan_type %q, got %+v", "pro", snapshot.PlanType)
	}
}

func TestNormalizeCodexUsageSnapshotsIncludesAdditionalRateLimits(t *testing.T) {
	raw := map[string]any{
		"plan_type": "plus",
		"rate_limit": map[string]any{
			"primary_window": map[string]any{
				"used_percent":         15,
				"limit_window_seconds": 300,
				"reset_at":             111,
			},
		},
		"additional_rate_limits": []any{
			map[string]any{
				"limit_name":      "codex_other",
				"metered_feature": "codex_other",
				"rate_limit": map[string]any{
					"primary_window": map[string]any{
						"used_percent":         70,
						"limit_window_seconds": 1800,
						"reset_at":             222,
					},
				},
			},
		},
	}

	snapshots, err := normalizeCodexUsageSnapshots(raw)
	if err != nil {
		t.Fatalf("normalizeCodexUsageSnapshots returned error: %v", err)
	}
	if len(snapshots) != 2 {
		t.Fatalf("expected 2 snapshots, got %d", len(snapshots))
	}
	if snapshots[0].LimitID == nil || *snapshots[0].LimitID != CodexPrimaryLimitID {
		t.Fatalf("expected first snapshot to be %q, got %+v", CodexPrimaryLimitID, snapshots[0].LimitID)
	}
	if snapshots[1].LimitID == nil || *snapshots[1].LimitID != "codex_other" {
		t.Fatalf("expected additional snapshot limit id %q, got %+v", "codex_other", snapshots[1].LimitID)
	}
	if snapshots[1].LimitName == nil || *snapshots[1].LimitName != "codex_other" {
		t.Fatalf("expected additional snapshot limit name %q, got %+v", "codex_other", snapshots[1].LimitName)
	}
	if snapshots[1].Primary == nil || snapshots[1].Primary.WindowMinutes == nil || *snapshots[1].Primary.WindowMinutes != 30 {
		t.Fatalf("expected additional snapshot primary window to map to 30 minutes, got %+v", snapshots[1].Primary)
	}
}

func TestNormalizeCodexUsageReturnsPrimaryCodexSnapshotWhenOnlyAdditionalExists(t *testing.T) {
	raw := `{
		"plan_type": "plus",
		"additional_rate_limits": [
			{
				"limit_name": "codex_other",
				"metered_feature": "codex_other",
				"rate_limit": {
					"primary_window": {
						"used_percent": 88,
						"limit_window_seconds": 900,
						"reset_at": 789
					}
				}
			}
		]
	}`

	snapshot, err := NormalizeCodexUsage(raw)
	if err != nil {
		t.Fatalf("NormalizeCodexUsage returned error: %v", err)
	}
	if snapshot == nil {
		t.Fatal("expected snapshot")
	}
	if snapshot.LimitID == nil || *snapshot.LimitID != CodexPrimaryLimitID {
		t.Fatalf("expected preferred snapshot limit id %q, got %+v", CodexPrimaryLimitID, snapshot.LimitID)
	}
	if snapshot.Primary != nil {
		t.Fatalf("expected preferred codex snapshot to keep nil primary when rate_limit is absent, got %+v", snapshot.Primary)
	}
	if snapshot.PlanType == nil || *snapshot.PlanType != "plus" {
		t.Fatalf("expected plan_type %q, got %+v", "plus", snapshot.PlanType)
	}
}

func TestNormalizeCodexUsageRejectsInvalidPayload(t *testing.T) {
	_, err := NormalizeCodexUsage(`{"plan_type":`)
	if err == nil {
		t.Fatal("expected invalid payload error")
	}
}
