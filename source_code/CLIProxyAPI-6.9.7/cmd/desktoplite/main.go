package main

import (
	"context"
	"flag"
	"fmt"
	"os"
	"os/signal"
	"path/filepath"
	"syscall"

	"github.com/router-for-me/CLIProxyAPI/v6/internal/buildinfo"
	"github.com/router-for-me/CLIProxyAPI/v6/internal/config"
	"github.com/router-for-me/CLIProxyAPI/v6/internal/logging"
	"github.com/router-for-me/CLIProxyAPI/v6/internal/usage"
	"github.com/router-for-me/CLIProxyAPI/v6/internal/util"
	sdkAuth "github.com/router-for-me/CLIProxyAPI/v6/sdk/auth"
	"github.com/router-for-me/CLIProxyAPI/v6/sdk/cliproxy"
	coreauth "github.com/router-for-me/CLIProxyAPI/v6/sdk/cliproxy/auth"
	log "github.com/sirupsen/logrus"
)

var (
	Version           = "dev"
	Commit            = "none"
	BuildDate         = "unknown"
	DefaultConfigPath = ""
)

func init() {
	logging.SetupBaseLogger()
	buildinfo.Version = Version
	buildinfo.Commit = Commit
	buildinfo.BuildDate = BuildDate
}

func main() {
	fmt.Printf("CLIProxyAPI Version: %s, Commit: %s, BuiltAt: %s\n", buildinfo.Version, buildinfo.Commit, buildinfo.BuildDate)

	var configPath string
	var password string

	flag.StringVar(&configPath, "config", DefaultConfigPath, "Configure File Path")
	flag.StringVar(&password, "password", "", "")
	flag.Parse()

	configFilePath, cfg, err := loadDesktopLiteConfig(configPath)
	if err != nil {
		log.Errorf("failed to load config: %v", err)
		return
	}

	if err := logging.ConfigureLogOutput(cfg); err != nil {
		log.Errorf("failed to configure log output: %v", err)
		return
	}
	util.SetLogLevel(cfg)

	usage.SetStatisticsEnabled(cfg.UsageStatisticsEnabled)
	coreauth.SetQuotaCooldownDisabled(cfg.DisableCooling)
	sdkAuth.RegisterTokenStore(sdkAuth.NewFileTokenStore())

	builder := cliproxy.NewBuilder().
		WithConfig(cfg).
		WithConfigPath(configFilePath).
		WithLocalManagementPassword(password)

	service, err := builder.Build()
	if err != nil {
		log.Errorf("failed to build proxy service: %v", err)
		return
	}

	ctx, cancel := signal.NotifyContext(context.Background(), syscall.SIGINT, syscall.SIGTERM)
	defer cancel()

	if err := service.Run(ctx); err != nil && err != context.Canceled {
		log.Errorf("proxy service exited with error: %v", err)
	}
}

func loadDesktopLiteConfig(configPath string) (string, *config.Config, error) {
	configFilePath := configPath
	if configFilePath == "" {
		wd, err := os.Getwd()
		if err != nil {
			return "", nil, err
		}
		configFilePath = filepath.Join(wd, "config.yaml")
	}

	cfg, err := config.LoadConfigOptional(configFilePath, false)
	if err != nil {
		return "", nil, err
	}
	if cfg == nil {
		cfg = &config.Config{}
	}

	resolvedAuthDir, err := util.ResolveAuthDir(cfg.AuthDir)
	if err != nil {
		return "", nil, err
	}
	cfg.AuthDir = resolvedAuthDir
	cfg.Host = "127.0.0.1"
	cfg.Pprof.Enable = false

	return configFilePath, cfg, nil
}
