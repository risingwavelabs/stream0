package main

import (
	"fmt"
	"os"
	"strconv"

	"gopkg.in/yaml.v3"
)

// Config holds all configuration
type Config struct {
	Server ServerConfig `yaml:"server"`
	DB     DBConfig     `yaml:"database"`
	Log    LogConfig    `yaml:"log"`
	Auth   AuthConfig   `yaml:"auth"`
}

// AuthConfig holds authentication configuration
type AuthConfig struct {
	APIKeys []string `yaml:"api_keys"`
}

// ServerConfig holds server configuration
type ServerConfig struct {
	Host string `yaml:"host"`
	Port int    `yaml:"port"`
}

// DBConfig holds database configuration
type DBConfig struct {
	Path string `yaml:"path"`
}

// LogConfig holds logging configuration
type LogConfig struct {
	Level  string `yaml:"level"`
	Format string `yaml:"format"`
}

// LoadConfig loads configuration from file and environment variables
func LoadConfig(path string) (*Config, error) {
	cfg := Config{
		Server: ServerConfig{Host: "127.0.0.1", Port: 8080},
		DB:     DBConfig{Path: "./stream0.db"},
		Log:    LogConfig{Level: "info", Format: "json"},
	}

	// Load from file if provided
	if path != "" {
		data, err := os.ReadFile(path)
		if err == nil {
			if err := yaml.Unmarshal(data, &cfg); err != nil {
				return nil, fmt.Errorf("failed to parse config file: %w", err)
			}
		}
	}

	// Override with environment variables (only if set)
	if v := os.Getenv("STREAM0_SERVER_HOST"); v != "" {
		cfg.Server.Host = v
	}
	if v := os.Getenv("STREAM0_SERVER_PORT"); v != "" {
		if port, err := strconv.Atoi(v); err == nil {
			cfg.Server.Port = port
		}
	}
	if v := os.Getenv("STREAM0_DB_PATH"); v != "" {
		cfg.DB.Path = v
	}
	if v := os.Getenv("STREAM0_LOG_LEVEL"); v != "" {
		cfg.Log.Level = v
	}
	if v := os.Getenv("STREAM0_LOG_FORMAT"); v != "" {
		cfg.Log.Format = v
	}
	if v := os.Getenv("STREAM0_API_KEY"); v != "" {
		cfg.Auth.APIKeys = append(cfg.Auth.APIKeys, v)
	}

	return &cfg, nil
}

// Address returns the server address
func (c *ServerConfig) Address() string {
	return fmt.Sprintf("%s:%d", c.Host, c.Port)
}
