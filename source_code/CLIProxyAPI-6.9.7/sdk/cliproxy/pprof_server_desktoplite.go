//go:build desktoplite

package cliproxy

import (
	"context"

	"github.com/router-for-me/CLIProxyAPI/v6/sdk/config"
)

type pprofServer struct{}

func (s *Service) applyPprofConfig(*config.Config) {}

func (s *Service) shutdownPprof(context.Context) error { return nil }
