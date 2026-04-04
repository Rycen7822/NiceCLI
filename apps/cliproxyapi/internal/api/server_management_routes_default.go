//go:build !desktoplite

package api

import (
	"github.com/gin-gonic/gin"
	"github.com/router-for-me/CLIProxyAPI/v6/internal/buildinfo"
)

func (s *Server) registerDesktopOnlyManagementRoutes(mgmt *gin.RouterGroup) {
	if s == nil || s.mgmt == nil || mgmt == nil || buildinfo.IsDesktopLite() {
		return
	}

	mgmt.POST("/api-call", s.mgmt.APICall)
	mgmt.GET("/usage/export", s.mgmt.ExportUsageStatistics)
	mgmt.POST("/usage/import", s.mgmt.ImportUsageStatistics)
	mgmt.GET("/latest-version", s.mgmt.GetLatestVersion)

	mgmt.GET("/ampcode", s.mgmt.GetAmpCode)
	mgmt.GET("/ampcode/upstream-url", s.mgmt.GetAmpUpstreamURL)
	mgmt.PUT("/ampcode/upstream-url", s.mgmt.PutAmpUpstreamURL)
	mgmt.PATCH("/ampcode/upstream-url", s.mgmt.PutAmpUpstreamURL)
	mgmt.DELETE("/ampcode/upstream-url", s.mgmt.DeleteAmpUpstreamURL)
	mgmt.GET("/ampcode/upstream-api-key", s.mgmt.GetAmpUpstreamAPIKey)
	mgmt.PUT("/ampcode/upstream-api-key", s.mgmt.PutAmpUpstreamAPIKey)
	mgmt.PATCH("/ampcode/upstream-api-key", s.mgmt.PutAmpUpstreamAPIKey)
	mgmt.DELETE("/ampcode/upstream-api-key", s.mgmt.DeleteAmpUpstreamAPIKey)
	mgmt.GET("/ampcode/restrict-management-to-localhost", s.mgmt.GetAmpRestrictManagementToLocalhost)
	mgmt.PUT("/ampcode/restrict-management-to-localhost", s.mgmt.PutAmpRestrictManagementToLocalhost)
	mgmt.PATCH("/ampcode/restrict-management-to-localhost", s.mgmt.PutAmpRestrictManagementToLocalhost)
	mgmt.GET("/ampcode/model-mappings", s.mgmt.GetAmpModelMappings)
	mgmt.PUT("/ampcode/model-mappings", s.mgmt.PutAmpModelMappings)
	mgmt.PATCH("/ampcode/model-mappings", s.mgmt.PatchAmpModelMappings)
	mgmt.DELETE("/ampcode/model-mappings", s.mgmt.DeleteAmpModelMappings)
	mgmt.GET("/ampcode/force-model-mappings", s.mgmt.GetAmpForceModelMappings)
	mgmt.PUT("/ampcode/force-model-mappings", s.mgmt.PutAmpForceModelMappings)
	mgmt.PATCH("/ampcode/force-model-mappings", s.mgmt.PutAmpForceModelMappings)
	mgmt.GET("/ampcode/upstream-api-keys", s.mgmt.GetAmpUpstreamAPIKeys)
	mgmt.PUT("/ampcode/upstream-api-keys", s.mgmt.PutAmpUpstreamAPIKeys)
	mgmt.PATCH("/ampcode/upstream-api-keys", s.mgmt.PatchAmpUpstreamAPIKeys)
	mgmt.DELETE("/ampcode/upstream-api-keys", s.mgmt.DeleteAmpUpstreamAPIKeys)
	mgmt.GET("/kimi-auth-url", s.mgmt.RequestKimiToken)
}
