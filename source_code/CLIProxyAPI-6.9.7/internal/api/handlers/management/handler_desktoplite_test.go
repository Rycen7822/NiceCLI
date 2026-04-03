package management

import (
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"github.com/gin-gonic/gin"
	"github.com/router-for-me/CLIProxyAPI/v6/internal/buildinfo"
	"github.com/router-for-me/CLIProxyAPI/v6/internal/config"
)

func setDesktopLiteFlavor(t *testing.T) {
	t.Helper()

	original := buildinfo.Flavor
	buildinfo.Flavor = "desktop-lite"
	t.Cleanup(func() {
		buildinfo.Flavor = original
	})
}

func newDesktopLiteMiddlewareRouter(handler *Handler) *gin.Engine {
	gin.SetMode(gin.TestMode)

	engine := gin.New()
	engine.Use(handler.Middleware())
	engine.GET("/probe", func(c *gin.Context) {
		c.Status(http.StatusNoContent)
	})
	return engine
}

func TestMiddlewareDesktopLiteRejectsRemoteRequests(t *testing.T) {
	setDesktopLiteFlavor(t)
	t.Setenv("MANAGEMENT_PASSWORD", "env-password")

	handler := NewHandlerWithoutConfigFilePath(&config.Config{}, nil)
	handler.SetLocalPassword("local-password")

	req := httptest.NewRequest(http.MethodGet, "/probe", nil)
	req.RemoteAddr = "8.8.8.8:12345"
	req.Header.Set("Authorization", "Bearer env-password")

	rr := httptest.NewRecorder()
	newDesktopLiteMiddlewareRouter(handler).ServeHTTP(rr, req)

	if rr.Code != http.StatusForbidden {
		t.Fatalf("unexpected status code: got %d want %d; body=%s", rr.Code, http.StatusForbidden, rr.Body.String())
	}
	if !strings.Contains(rr.Body.String(), "remote management disabled") {
		t.Fatalf("unexpected body: %s", rr.Body.String())
	}
}

func TestMiddlewareDesktopLiteAcceptsLocalRuntimePassword(t *testing.T) {
	setDesktopLiteFlavor(t)

	handler := NewHandlerWithoutConfigFilePath(&config.Config{}, nil)
	handler.SetLocalPassword("local-password")

	req := httptest.NewRequest(http.MethodGet, "/probe", nil)
	req.RemoteAddr = "127.0.0.1:12345"
	req.Header.Set("Authorization", "Bearer local-password")

	rr := httptest.NewRecorder()
	newDesktopLiteMiddlewareRouter(handler).ServeHTTP(rr, req)

	if rr.Code != http.StatusNoContent {
		t.Fatalf("unexpected status code: got %d want %d; body=%s", rr.Code, http.StatusNoContent, rr.Body.String())
	}
}
