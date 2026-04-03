//go:build desktoplite

package api

import "github.com/router-for-me/CLIProxyAPI/v6/internal/config"

func (s *Server) configureManagementAssets(*config.Config) {}

func (s *Server) registerManagementControlPanelRoute() {}
