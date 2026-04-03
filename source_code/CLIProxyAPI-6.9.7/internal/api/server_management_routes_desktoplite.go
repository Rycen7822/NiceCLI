//go:build desktoplite

package api

import "github.com/gin-gonic/gin"

func (s *Server) registerDesktopOnlyManagementRoutes(*gin.RouterGroup) {}
