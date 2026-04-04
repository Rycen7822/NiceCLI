// Package cliproxy provides the core service implementation for the CLI Proxy API.
// It includes service lifecycle management, authentication handling, file watching,
// and integration with various AI service providers through a unified interface.
package cliproxy

import (
	"context"

	"github.com/router-for-me/CLIProxyAPI/v6/sdk/config"
)

// WatcherFactory creates a watcher for configuration and token changes.
// The reload callback receives the updated configuration when changes are detected.
//
// Parameters:
//   - configPath: The path to the configuration file to watch
//   - authDir: The directory containing authentication tokens to watch
//   - reload: The callback function to call when changes are detected
//
// Returns:
//   - *WatcherWrapper: A watcher wrapper instance
//   - error: An error if watcher creation fails
type WatcherFactory func(configPath, authDir string, reload func(*config.Config)) (*WatcherWrapper, error)

// WatcherWrapper exposes the subset of watcher methods required by the SDK.
type WatcherWrapper struct {
	start func(ctx context.Context) error
	stop  func() error

	setConfig             func(cfg *config.Config)
	setUpdateQueue        func(queue chan<- AuthUpdate)
	dispatchRuntimeUpdate func(update AuthUpdate) bool
}

// Start proxies to the underlying watcher Start implementation.
func (w *WatcherWrapper) Start(ctx context.Context) error {
	if w == nil || w.start == nil {
		return nil
	}
	return w.start(ctx)
}

// Stop proxies to the underlying watcher Stop implementation.
func (w *WatcherWrapper) Stop() error {
	if w == nil || w.stop == nil {
		return nil
	}
	return w.stop()
}

// SetConfig updates the watcher configuration cache.
func (w *WatcherWrapper) SetConfig(cfg *config.Config) {
	if w == nil || w.setConfig == nil {
		return
	}
	w.setConfig(cfg)
}

// DispatchRuntimeAuthUpdate forwards runtime auth updates (e.g., websocket providers)
// into the watcher-managed auth update queue when available.
// Returns true if the update was enqueued successfully.
func (w *WatcherWrapper) DispatchRuntimeAuthUpdate(update AuthUpdate) bool {
	if w == nil || w.dispatchRuntimeUpdate == nil {
		return false
	}
	return w.dispatchRuntimeUpdate(update)
}

// SetAuthUpdateQueue registers the channel used to propagate auth updates.
func (w *WatcherWrapper) SetAuthUpdateQueue(queue chan<- AuthUpdate) {
	if w == nil || w.setUpdateQueue == nil {
		return
	}
	w.setUpdateQueue(queue)
}
