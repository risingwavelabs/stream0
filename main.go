package main

import (
	"context"
	"flag"
	"net/http"
	"os"
	"os/signal"
	"syscall"
	"time"

	"github.com/sirupsen/logrus"
)

func main() {
	var configPath string
	flag.StringVar(&configPath, "config", "", "Path to config file")
	flag.Parse()

	// Load configuration
	cfg, err := LoadConfig(configPath)
	if err != nil {
		logrus.WithError(err).Fatal("Failed to load configuration")
	}

	// Setup logging
	setupLogging(cfg.Log)

	log := logrus.WithFields(logrus.Fields{
		"component": "main",
	})

	log.Info("stream0 starting")

	// Initialize database
	db, err := NewDatabase(cfg.DB.Path)
	if err != nil {
		log.WithError(err).Fatal("Failed to initialize database")
	}
	defer db.Close()

	// Create server
	if len(cfg.Auth.APIKeys) > 0 {
		log.WithField("keys", len(cfg.Auth.APIKeys)).Info("API key authentication enabled")
	} else {
		log.Warn("No API keys configured - all endpoints are unauthenticated")
	}
	server := NewServer(db, cfg.Server, cfg.Auth)

	// Setup graceful shutdown
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()

	// Start server in goroutine
	errChan := make(chan error, 1)
	go func() {
		addr := cfg.Server.Address()
		log.WithField("address", addr).Info("Server starting")
		if err := server.Run(addr); err != nil && err != http.ErrServerClosed {
			errChan <- err
		}
	}()

	// Wait for shutdown signal or error
	select {
	case <-ctx.Done():
		log.Info("Shutdown signal received, initiating graceful shutdown")
	case err := <-errChan:
		log.WithError(err).Fatal("Server error")
	}

	// Graceful shutdown with timeout
	shutdownCtx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	if err := server.Shutdown(shutdownCtx); err != nil {
		log.WithError(err).Error("Shutdown error")
	}

	log.Info("stream0 stopped")
}

func setupLogging(cfg LogConfig) {
	// Set level
	level, err := logrus.ParseLevel(cfg.Level)
	if err != nil {
		level = logrus.InfoLevel
	}
	logrus.SetLevel(level)

	// Set format
	if cfg.Format == "json" {
		logrus.SetFormatter(&logrus.JSONFormatter{
			TimestampFormat: time.RFC3339Nano,
		})
	} else {
		logrus.SetFormatter(&logrus.TextFormatter{
			FullTimestamp:   true,
			TimestampFormat: time.RFC3339,
		})
	}
}
