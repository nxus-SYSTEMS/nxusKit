package nxuskit

import (
	"testing"
)

// Pro stub no-license contract tests (asserting ErrLicenseRequired) are in
// pro_no_license_test.go, gated behind //go:build no_license.
// Run on demand: go test -tags=no_license -run TestProContract ./...

func TestDefaultConfigs(t *testing.T) {
	t.Run("DefaultRetryPolicyConfig", func(t *testing.T) {
		cfg := DefaultRetryPolicyConfig()
		if cfg.MaxRetries != 3 {
			t.Errorf("expected MaxRetries 3, got %d", cfg.MaxRetries)
		}
		if cfg.Multiplier != 2.0 {
			t.Errorf("expected Multiplier 2.0, got %f", cfg.Multiplier)
		}
	})

	t.Run("DefaultBatcherConfig", func(t *testing.T) {
		cfg := DefaultBatcherConfig()
		if cfg.MaxBatchSize != 10 {
			t.Errorf("expected MaxBatchSize 10, got %d", cfg.MaxBatchSize)
		}
	})

	t.Run("DefaultCacheConfig", func(t *testing.T) {
		cfg := DefaultCacheConfig()
		if cfg.MaxEntries != 1000 {
			t.Errorf("expected MaxEntries 1000, got %d", cfg.MaxEntries)
		}
	})
}

func TestLoadBalanceStrategyValues(t *testing.T) {
	if LoadBalanceRoundRobin != 0 {
		t.Errorf("expected LoadBalanceRoundRobin to be 0, got %d", LoadBalanceRoundRobin)
	}
	if LoadBalanceWeighted != 1 {
		t.Errorf("expected LoadBalanceWeighted to be 1, got %d", LoadBalanceWeighted)
	}
	if LoadBalanceLeastConnections != 2 {
		t.Errorf("expected LoadBalanceLeastConnections to be 2, got %d", LoadBalanceLeastConnections)
	}
	if LoadBalanceRandom != 3 {
		t.Errorf("expected LoadBalanceRandom to be 3, got %d", LoadBalanceRandom)
	}
}
