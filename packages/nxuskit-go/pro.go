// Package nxuskit provides a unified interface for multiple LLM providers.
//
// This file contains Pro feature stubs that return ErrLicenseRequired errors
// in the open-source version.

package nxuskit

import (
	"time"
)

// =============================================================================
// Semantic Router
// =============================================================================

// SemanticRouterConfig configures the semantic router.
type SemanticRouterConfig struct {
	// Routes to configure.
	Routes []SemanticRoute
}

// SemanticRoute defines a semantic routing rule.
type SemanticRoute struct {
	// Name is the route identifier.
	Name string
	// Provider is the target provider for this route.
	Provider string
	// Patterns are keywords or patterns that trigger this route.
	Patterns []string
}

// SemanticRouter provides intelligent request routing based on content analysis.
//
// Pro Feature: Requires a nxusKit Pro license.
type SemanticRouter struct{}

// NewSemanticRouter creates a new semantic router with the given configuration.
//
// Returns ErrLicenseRequired — upgrade to nxusKit Pro for this feature.
func NewSemanticRouter(_ SemanticRouterConfig) (*SemanticRouter, error) {
	return nil, NewLicenseRequiredError("Semantic Router")
}

// Route routes a request to the appropriate provider based on content.
//
// Returns ErrLicenseRequired — upgrade to nxusKit Pro for this feature.
func (r *SemanticRouter) Route(_ string) (string, error) {
	return "", NewLicenseRequiredError("Semantic Router")
}

// =============================================================================
// Cost Tracker
// =============================================================================

// CostTrackerConfig configures the cost tracker.
type CostTrackerConfig struct {
	// BudgetLimit is the budget limit in USD (0 for unlimited).
	BudgetLimit float64
	// AlertThreshold is the alert threshold as a percentage of budget (0.0-1.0).
	AlertThreshold float64
}

// CostTracker provides real-time cost tracking for LLM usage.
//
// Pro Feature: Requires a nxusKit Pro license.
type CostTracker struct{}

// NewCostTracker creates a new cost tracker with the given configuration.
//
// Returns ErrLicenseRequired — upgrade to nxusKit Pro for this feature.
func NewCostTracker(_ CostTrackerConfig) (*CostTracker, error) {
	return nil, NewLicenseRequiredError("Cost Tracker")
}

// TotalCost returns the current total cost.
//
// Returns ErrLicenseRequired — upgrade to nxusKit Pro for this feature.
func (t *CostTracker) TotalCost() (float64, error) {
	return 0, NewLicenseRequiredError("Cost Tracker")
}

// CostByProvider returns the cost breakdown by provider.
//
// Returns ErrLicenseRequired — upgrade to nxusKit Pro for this feature.
func (t *CostTracker) CostByProvider() (map[string]float64, error) {
	return nil, NewLicenseRequiredError("Cost Tracker")
}

// =============================================================================
// Retry Policy
// =============================================================================

// RetryPolicyConfig configures the automatic retry policy.
type RetryPolicyConfig struct {
	// MaxRetries is the maximum number of retries.
	MaxRetries int
	// InitialDelay is the initial delay between retries.
	InitialDelay time.Duration
	// MaxDelay is the maximum delay between retries.
	MaxDelay time.Duration
	// Multiplier is the exponential backoff multiplier.
	Multiplier float64
}

// DefaultRetryPolicyConfig returns the default retry policy configuration.
func DefaultRetryPolicyConfig() RetryPolicyConfig {
	return RetryPolicyConfig{
		MaxRetries:   3,
		InitialDelay: 100 * time.Millisecond,
		MaxDelay:     30 * time.Second,
		Multiplier:   2.0,
	}
}

// RetryPolicy provides automatic retry with exponential backoff.
//
// Pro Feature: Requires a nxusKit Pro license.
type RetryPolicy struct{}

// NewRetryPolicy creates a new retry policy with the given configuration.
//
// Returns ErrLicenseRequired — upgrade to nxusKit Pro for this feature.
func NewRetryPolicy(_ RetryPolicyConfig) (*RetryPolicy, error) {
	return nil, NewLicenseRequiredError("Automatic Retries")
}

// =============================================================================
// Request Batcher
// =============================================================================

// BatcherConfig configures the request batcher.
type BatcherConfig struct {
	// MaxBatchSize is the maximum batch size.
	MaxBatchSize int
	// MaxWait is the maximum wait time before flushing a partial batch.
	MaxWait time.Duration
}

// DefaultBatcherConfig returns the default batcher configuration.
func DefaultBatcherConfig() BatcherConfig {
	return BatcherConfig{
		MaxBatchSize: 10,
		MaxWait:      100 * time.Millisecond,
	}
}

// RequestBatcher provides efficient batching of multiple requests.
//
// Pro Feature: Requires a nxusKit Pro license.
type RequestBatcher struct{}

// NewRequestBatcher creates a new request batcher with the given configuration.
//
// Returns ErrLicenseRequired — upgrade to nxusKit Pro for this feature.
func NewRequestBatcher(_ BatcherConfig) (*RequestBatcher, error) {
	return nil, NewLicenseRequiredError("Request Batching")
}

// =============================================================================
// Response Cache
// =============================================================================

// CacheConfig configures the response cache.
type CacheConfig struct {
	// MaxEntries is the maximum number of cached entries.
	MaxEntries int
	// DefaultTTL is the default TTL for cached entries.
	DefaultTTL time.Duration
}

// DefaultCacheConfig returns the default cache configuration.
func DefaultCacheConfig() CacheConfig {
	return CacheConfig{
		MaxEntries: 1000,
		DefaultTTL: time.Hour,
	}
}

// ResponseCache provides intelligent response caching with TTL and invalidation.
//
// Pro Feature: Requires a nxusKit Pro license.
type ResponseCache struct{}

// NewResponseCache creates a new response cache with the given configuration.
//
// Returns ErrLicenseRequired — upgrade to nxusKit Pro for this feature.
func NewResponseCache(_ CacheConfig) (*ResponseCache, error) {
	return nil, NewLicenseRequiredError("Response Caching")
}

// =============================================================================
// Load Balancer
// =============================================================================

// LoadBalanceStrategy defines the load balancing strategy.
type LoadBalanceStrategy int

const (
	// LoadBalanceRoundRobin distributes requests round-robin.
	LoadBalanceRoundRobin LoadBalanceStrategy = iota
	// LoadBalanceWeighted distributes requests based on provider weights.
	LoadBalanceWeighted
	// LoadBalanceLeastConnections distributes to providers with fewest connections.
	LoadBalanceLeastConnections
	// LoadBalanceRandom distributes requests randomly.
	LoadBalanceRandom
)

// LoadBalancerConfig configures the load balancer.
type LoadBalancerConfig struct {
	// Providers to balance across.
	Providers []string
	// Strategy is the load balancing strategy.
	Strategy LoadBalanceStrategy
	// Weights for weighted strategy (one per provider).
	Weights []int
}

// LoadBalancer provides multi-provider load balancing and failover.
//
// Pro Feature: Requires a nxusKit Pro license.
type LoadBalancer struct{}

// NewLoadBalancer creates a new load balancer with the given configuration.
//
// Returns ErrLicenseRequired — upgrade to nxusKit Pro for this feature.
func NewLoadBalancer(_ LoadBalancerConfig) (*LoadBalancer, error) {
	return nil, NewLicenseRequiredError("Load Balancing")
}
