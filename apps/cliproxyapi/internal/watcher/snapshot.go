package watcher

import (
	"github.com/router-for-me/CLIProxyAPI/v6/internal/config"
	coreauth "github.com/router-for-me/CLIProxyAPI/v6/sdk/cliproxy/auth"
)

// SnapshotCoreAuths returns the synthesized static auth snapshot without starting fsnotify watchers.
func SnapshotCoreAuths(cfg *config.Config, authDir string) []*coreauth.Auth {
	return snapshotCoreAuths(cfg, authDir)
}
