//go:build desktoplite

package cliproxy

import "github.com/router-for-me/CLIProxyAPI/v6/sdk/config"

func defaultWatcherFactory(string, string, func(*config.Config)) (*WatcherWrapper, error) {
	return &WatcherWrapper{}, nil
}
