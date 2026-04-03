package executor

import (
	"errors"
	"net/http"
	"strings"

	"github.com/router-for-me/CLIProxyAPI/v6/internal/quota"
	cliproxyauth "github.com/router-for-me/CLIProxyAPI/v6/sdk/cliproxy/auth"
	cliproxyexecutor "github.com/router-for-me/CLIProxyAPI/v6/sdk/cliproxy/executor"
)

func captureCodexQuotaEvent(auth *cliproxyauth.Auth, payload []byte) {
	if auth == nil || len(payload) == 0 {
		return
	}
	service := quota.DefaultCodexQuotaService()
	if service == nil {
		return
	}
	_ = service.CaptureInlineRateLimitEvent(auth, payload)
}

func maybeTriggerCodexQuotaRefresh(auth *cliproxyauth.Auth, err error) {
	if auth == nil || err == nil {
		return
	}
	var statusErr cliproxyexecutor.StatusError
	if !errors.As(err, &statusErr) {
		return
	}
	if statusErr.StatusCode() != http.StatusTooManyRequests {
		return
	}
	service := quota.DefaultCodexQuotaService()
	if service == nil {
		return
	}
	service.RefreshAsync(strings.TrimSpace(auth.ID), "")
}
