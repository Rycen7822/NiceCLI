package quota

import (
	"bytes"
	"encoding/json"
	"fmt"
	"strconv"
	"strings"
)

type codexUsagePayload struct {
	PlanType             string                      `json:"plan_type"`
	RateLimit            *codexUsageRateLimitDetails `json:"rate_limit"`
	Credits              *codexUsageCreditsDetails   `json:"credits"`
	AdditionalRateLimits []codexAdditionalRateLimit  `json:"additional_rate_limits"`
}

type codexUsageRateLimitDetails struct {
	Allowed         bool              `json:"allowed"`
	LimitReached    bool              `json:"limit_reached"`
	PrimaryWindow   *codexUsageWindow `json:"primary_window"`
	SecondaryWindow *codexUsageWindow `json:"secondary_window"`
}

type codexUsageCreditsDetails struct {
	HasCredits bool `json:"has_credits"`
	Unlimited  bool `json:"unlimited"`
	Balance    any  `json:"balance"`
}

type codexAdditionalRateLimit struct {
	LimitName      string                      `json:"limit_name"`
	MeteredFeature string                      `json:"metered_feature"`
	RateLimit      *codexUsageRateLimitDetails `json:"rate_limit"`
}

type codexUsageWindow struct {
	UsedPercent        float64 `json:"used_percent"`
	LimitWindowSeconds int64   `json:"limit_window_seconds"`
	ResetAfterSeconds  int64   `json:"reset_after_seconds"`
	ResetAt            int64   `json:"reset_at"`
}

type codexRateLimitEvent struct {
	Type             string                      `json:"type"`
	PlanType         string                      `json:"plan_type"`
	RateLimits       *codexRateLimitEventDetails `json:"rate_limits"`
	Credits          *codexUsageCreditsDetails   `json:"credits"`
	MeteredLimitName string                      `json:"metered_limit_name"`
	LimitName        string                      `json:"limit_name"`
}

type codexRateLimitEventDetails struct {
	Primary   *codexRateLimitEventWindow `json:"primary"`
	Secondary *codexRateLimitEventWindow `json:"secondary"`
}

type codexRateLimitEventWindow struct {
	UsedPercent   float64 `json:"used_percent"`
	WindowMinutes *int64  `json:"window_minutes"`
	ResetsAt      *int64  `json:"reset_at"`
}

func NormalizeCodexUsage(raw any) (*RateLimitSnapshot, error) {
	snapshots, err := normalizeCodexUsageSnapshots(raw)
	if err != nil {
		return nil, err
	}
	if len(snapshots) == 0 {
		return nil, nil
	}

	for _, snapshot := range snapshots {
		if snapshot != nil && snapshot.LimitID != nil && *snapshot.LimitID == CodexPrimaryLimitID {
			return snapshot.Clone(), nil
		}
	}

	return snapshots[0].Clone(), nil
}

func NormalizeCodexRateLimitEvent(raw any) (*RateLimitSnapshot, error) {
	event, err := decodeCodexRateLimitEvent(raw)
	if err != nil {
		return nil, err
	}
	if event == nil || event.RateLimits == nil {
		return nil, nil
	}
	if eventType := strings.TrimSpace(event.Type); eventType != "" && eventType != "codex.rate_limits" {
		return nil, nil
	}

	limitID := stringPointer(event.MeteredLimitName)
	if limitID == nil {
		limitID = stringPointer(event.LimitName)
	}
	if limitID == nil {
		limitID = stringPointer(CodexPrimaryLimitID)
	}

	snapshot := &RateLimitSnapshot{
		LimitID:  limitID,
		PlanType: stringPointer(event.PlanType),
		Credits:  normalizeCredits(event.Credits),
	}
	if event.RateLimits != nil {
		snapshot.Primary = normalizeRateLimitEventWindow(event.RateLimits.Primary)
		snapshot.Secondary = normalizeRateLimitEventWindow(event.RateLimits.Secondary)
	}
	return snapshot, nil
}

func normalizeCodexUsageSnapshots(raw any) ([]*RateLimitSnapshot, error) {
	payload, err := decodeCodexUsagePayload(raw)
	if err != nil {
		return nil, err
	}

	planType := stringPointer(payload.PlanType)
	snapshots := []*RateLimitSnapshot{
		makeRateLimitSnapshot(
			stringPointer(CodexPrimaryLimitID),
			nil,
			payload.RateLimit,
			payload.Credits,
			planType,
		),
	}

	for _, additional := range payload.AdditionalRateLimits {
		limitID := additionalLimitID(additional)
		snapshots = append(snapshots, makeRateLimitSnapshot(
			limitID,
			stringPointer(additional.LimitName),
			additional.RateLimit,
			nil,
			planType,
		))
	}

	return snapshots, nil
}

func decodeCodexUsagePayload(raw any) (*codexUsagePayload, error) {
	switch typed := raw.(type) {
	case nil:
		return nil, fmt.Errorf("codex usage payload is nil")
	case *codexUsagePayload:
		if typed == nil {
			return nil, fmt.Errorf("codex usage payload is nil")
		}
		copyPayload := *typed
		return &copyPayload, nil
	case codexUsagePayload:
		copyPayload := typed
		return &copyPayload, nil
	}

	data, err := marshalUsagePayload(raw)
	if err != nil {
		return nil, fmt.Errorf("marshal codex usage payload: %w", err)
	}

	var payload codexUsagePayload
	decoder := json.NewDecoder(bytes.NewReader(data))
	decoder.UseNumber()
	if err := decoder.Decode(&payload); err != nil {
		return nil, fmt.Errorf("decode codex usage payload: %w", err)
	}
	return &payload, nil
}

func decodeCodexRateLimitEvent(raw any) (*codexRateLimitEvent, error) {
	switch typed := raw.(type) {
	case nil:
		return nil, fmt.Errorf("codex rate limit event is nil")
	case *codexRateLimitEvent:
		if typed == nil {
			return nil, fmt.Errorf("codex rate limit event is nil")
		}
		copyEvent := *typed
		return &copyEvent, nil
	case codexRateLimitEvent:
		copyEvent := typed
		return &copyEvent, nil
	}

	data, err := marshalUsagePayload(raw)
	if err != nil {
		return nil, fmt.Errorf("marshal codex rate limit event: %w", err)
	}

	var event codexRateLimitEvent
	decoder := json.NewDecoder(bytes.NewReader(data))
	decoder.UseNumber()
	if err := decoder.Decode(&event); err != nil {
		return nil, fmt.Errorf("decode codex rate limit event: %w", err)
	}
	return &event, nil
}

func marshalUsagePayload(raw any) ([]byte, error) {
	switch typed := raw.(type) {
	case []byte:
		return typed, nil
	case json.RawMessage:
		return []byte(typed), nil
	case string:
		return []byte(typed), nil
	default:
		return json.Marshal(raw)
	}
}

func makeRateLimitSnapshot(limitID, limitName *string, rateLimit *codexUsageRateLimitDetails, credits *codexUsageCreditsDetails, planType *string) *RateLimitSnapshot {
	var primary *RateLimitWindow
	var secondary *RateLimitWindow
	if rateLimit != nil {
		primary = normalizeWindow(rateLimit.PrimaryWindow)
		secondary = normalizeWindow(rateLimit.SecondaryWindow)
	}

	return &RateLimitSnapshot{
		LimitID:   cloneStringPointer(limitID),
		LimitName: cloneStringPointer(limitName),
		Primary:   primary,
		Secondary: secondary,
		Credits:   normalizeCredits(credits),
		PlanType:  cloneStringPointer(planType),
	}
}

func normalizeWindow(window *codexUsageWindow) *RateLimitWindow {
	if window == nil {
		return nil
	}

	return &RateLimitWindow{
		UsedPercent:   window.UsedPercent,
		WindowMinutes: minutesPointer(window.LimitWindowSeconds),
		ResetsAt:      unixPointer(window.ResetAt),
	}
}

func normalizeRateLimitEventWindow(window *codexRateLimitEventWindow) *RateLimitWindow {
	if window == nil {
		return nil
	}

	return &RateLimitWindow{
		UsedPercent:   window.UsedPercent,
		WindowMinutes: cloneInt64Pointer(window.WindowMinutes),
		ResetsAt:      cloneInt64Pointer(window.ResetsAt),
	}
}

func normalizeCredits(credits *codexUsageCreditsDetails) *CreditsSnapshot {
	if credits == nil {
		return nil
	}

	return &CreditsSnapshot{
		HasCredits: credits.HasCredits,
		Unlimited:  credits.Unlimited,
		Balance:    stringPointerFromAny(credits.Balance),
	}
}

func additionalLimitID(limit codexAdditionalRateLimit) *string {
	if candidate := stringPointer(limit.MeteredFeature); candidate != nil {
		return candidate
	}
	return stringPointer(limit.LimitName)
}

func stringPointer(value string) *string {
	value = strings.TrimSpace(value)
	if value == "" {
		return nil
	}
	return &value
}

func cloneStringPointer(value *string) *string {
	if value == nil {
		return nil
	}
	copyValue := *value
	return &copyValue
}

func cloneInt64Pointer(value *int64) *int64 {
	if value == nil {
		return nil
	}
	copyValue := *value
	return &copyValue
}

func minutesPointer(seconds int64) *int64 {
	if seconds <= 0 {
		return nil
	}
	minutes := (seconds + 59) / 60
	return &minutes
}

func unixPointer(ts int64) *int64 {
	if ts <= 0 {
		return nil
	}
	return &ts
}

func stringPointerFromAny(value any) *string {
	switch typed := value.(type) {
	case nil:
		return nil
	case string:
		return stringPointer(typed)
	case json.Number:
		formatted := typed.String()
		return stringPointer(formatted)
	case float64:
		formatted := strconv.FormatFloat(typed, 'f', -1, 64)
		return stringPointer(formatted)
	case float32:
		formatted := strconv.FormatFloat(float64(typed), 'f', -1, 32)
		return stringPointer(formatted)
	case int:
		formatted := strconv.Itoa(typed)
		return stringPointer(formatted)
	case int64:
		formatted := strconv.FormatInt(typed, 10)
		return stringPointer(formatted)
	case int32:
		formatted := strconv.FormatInt(int64(typed), 10)
		return stringPointer(formatted)
	case uint:
		formatted := strconv.FormatUint(uint64(typed), 10)
		return stringPointer(formatted)
	case uint64:
		formatted := strconv.FormatUint(typed, 10)
		return stringPointer(formatted)
	case bool:
		formatted := strconv.FormatBool(typed)
		return stringPointer(formatted)
	default:
		formatted := fmt.Sprint(typed)
		return stringPointer(formatted)
	}
}
