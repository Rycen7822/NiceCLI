//go:build !desktoplite

package cliproxy

import (
	"context"

	"github.com/router-for-me/CLIProxyAPI/v6/internal/watcher"
	"github.com/router-for-me/CLIProxyAPI/v6/sdk/config"
)

func defaultWatcherFactory(configPath, authDir string, reload func(*config.Config)) (*WatcherWrapper, error) {
	w, err := watcher.NewWatcher(configPath, authDir, reload)
	if err != nil {
		return nil, err
	}

	return &WatcherWrapper{
		start: func(ctx context.Context) error {
			return w.Start(ctx)
		},
		stop: func() error {
			return w.Stop()
		},
		setConfig: func(cfg *config.Config) {
			w.SetConfig(cfg)
		},
		setUpdateQueue: func(queue chan<- AuthUpdate) {
			if queue == nil {
				w.SetAuthUpdateQueue(nil)
				return
			}
			bridge := make(chan watcher.AuthUpdate, 256)
			go func() {
				for update := range bridge {
					queue <- AuthUpdate{
						Action: AuthUpdateAction(update.Action),
						ID:     update.ID,
						Auth:   update.Auth,
					}
				}
			}()
			w.SetAuthUpdateQueue(bridge)
		},
		dispatchRuntimeUpdate: func(update AuthUpdate) bool {
			return w.DispatchRuntimeAuthUpdate(watcher.AuthUpdate{
				Action: watcher.AuthUpdateAction(update.Action),
				ID:     update.ID,
				Auth:   update.Auth,
			})
		},
	}, nil
}
