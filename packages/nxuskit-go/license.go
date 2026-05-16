//go:build nxuskit

package nxuskit

import (
	"encoding/json"
	"fmt"

	"github.com/nxus-SYSTEMS/nxusKit/packages/nxuskit-go/internal/ffi"
)

// EditionLimits contains numerical limits for a product edition.
// Values are nil when unlimited (no enforcement).
type EditionLimits struct {
	MaxSessions          *uint64 `json:"max_sessions,omitempty"`
	MaxCachedRulebases   *uint64 `json:"max_cached_rulebases,omitempty"`
	MaxRulesPerSession   *uint64 `json:"max_rules_per_session,omitempty"`
	MaxFactsPerSession   *uint64 `json:"max_facts_per_session,omitempty"`
	MaxBayesianNodes     *uint64 `json:"max_bayesian_nodes,omitempty"`
	MaxSolverConstraints *uint64 `json:"max_solver_constraints,omitempty"`
	Seats                *uint64 `json:"seats,omitempty"`
}

// LicenseResolution is the result of resolving a license token from
// the precedence chain (env var → file → API param).
type LicenseResolution struct {
	// Source where the token was found: "env_var", "file", "api_param", or "none"
	Source string `json:"source"`
	// TokenType: "trial", "developer", "deployment", or "none"
	TokenType string `json:"token_type"`
	// Valid indicates whether the token passed validation
	Valid bool `json:"valid"`
	// Error message if validation failed (nil if valid)
	Error *string `json:"error"`
	// ProductID identifies which product the token is for (e.g., "nxuskit")
	ProductID string `json:"product_id,omitempty"`
	// EffectiveLimits contains the resolved limits (catalog defaults + token overrides)
	EffectiveLimits *EditionLimits `json:"effective_limits,omitempty"`
	// Features contains the effective feature list for the resolved edition
	Features []string `json:"features,omitempty"`
}

// TokenInfo is the result of validating a license token JWT.
type TokenInfo struct {
	// Valid indicates whether the token is valid
	Valid bool `json:"valid"`
	// TokenType: "trial", "developer", "deployment"
	TokenType string `json:"token_type"`
	// Edition granted by the token (e.g., "pro")
	Edition *string `json:"edition"`
	// DaysRemaining until token expiry (nil for deployment tokens)
	DaysRemaining *int64 `json:"days_remaining"`
	// Error message if validation failed
	Error *string `json:"error"`
	// Result is the entitlement result code
	Result string `json:"result"`
}

// LicenseResolve resolves the active license token from all available sources.
//
// Resolution order:
//  1. NXUSKIT_LICENSE_TOKEN environment variable
//  2. ~/.nxuskit/license.token file
//  3. explicitKey parameter (if non-empty)
func LicenseResolve(explicitKey string) (*LicenseResolution, error) {
	jsonStr, err := ffi.LicenseResolve(explicitKey)
	if err != nil {
		return nil, fmt.Errorf("nxuskit: license resolve failed: %w", err)
	}

	var result LicenseResolution
	if err := json.Unmarshal([]byte(jsonStr), &result); err != nil {
		return nil, fmt.Errorf("nxuskit: failed to parse license resolve JSON: %w", err)
	}

	return &result, nil
}

// LicenseValidate validates a license token JWT string.
//
// Performs RS384 signature verification, claim parsing, and type-specific
// validation (expiry, machine binding, version ceiling).
func LicenseValidate(token string) (*TokenInfo, error) {
	jsonStr, err := ffi.LicenseValidate(token)
	if err != nil {
		return nil, fmt.Errorf("nxuskit: license validate failed: %w", err)
	}

	var result TokenInfo
	if err := json.Unmarshal([]byte(jsonStr), &result); err != nil {
		return nil, fmt.Errorf("nxuskit: failed to parse license validate JSON: %w", err)
	}

	return &result, nil
}

// LicenseMachineID returns the machine fingerprint for this device.
//
// Returns a "sha256:<64-hex-chars>" string derived from the OS machine ID.
// Returns an error if the machine ID cannot be determined (e.g., in Docker
// containers or minimal environments).
func LicenseMachineID() (string, error) {
	return ffi.LicenseMachineID()
}

// ActivationResult is the result of activating or deactivating a Pro license.
type ActivationResult struct {
	// Success indicates whether the operation succeeded
	Success bool `json:"success"`
	// SeatsUsed is the number of machines currently using this license
	SeatsUsed uint32 `json:"seats_used"`
	// SeatsTotal is the maximum number of machines allowed
	SeatsTotal uint32 `json:"seats_total"`
	// Message is a human-readable status message
	Message string `json:"message"`
	// Error message if the operation failed (nil if successful)
	Error *string `json:"error"`
}

// TrialResult is the result of a trial issuance or activation.
type TrialResult struct {
	// Success indicates whether the operation succeeded
	Success bool `json:"success"`
	// DaysRemaining until trial expiry
	DaysRemaining uint32 `json:"days_remaining"`
	// Message is a human-readable status message
	Message string `json:"message"`
	// Error message if the operation failed (nil if successful)
	Error *string `json:"error"`
}

// LicenseActivate activates a Pro license on this machine.
//
// Calls the licensing microservice to validate the purchase ID, generate
// a machine-bound developer token, and store it locally.
func LicenseActivate(purchaseID string) (*ActivationResult, error) {
	jsonStr, err := ffi.LicenseActivate(purchaseID)
	if err != nil {
		return nil, fmt.Errorf("nxuskit: license activate failed: %w", err)
	}

	var result ActivationResult
	if err := json.Unmarshal([]byte(jsonStr), &result); err != nil {
		return nil, fmt.Errorf("nxuskit: failed to parse license activate JSON: %w", err)
	}

	return &result, nil
}

// LicenseDeactivate deactivates the Pro license on this machine.
//
// Releases this machine's seat and removes the stored token.
func LicenseDeactivate() (*ActivationResult, error) {
	jsonStr, err := ffi.LicenseDeactivate()
	if err != nil {
		return nil, fmt.Errorf("nxuskit: license deactivate failed: %w", err)
	}

	var result ActivationResult
	if err := json.Unmarshal([]byte(jsonStr), &result); err != nil {
		return nil, fmt.Errorf("nxuskit: failed to parse license deactivate JSON: %w", err)
	}

	return &result, nil
}

// LicenseTrialIssue issues a 30-day trial token for this machine.
func LicenseTrialIssue() (*TrialResult, error) {
	jsonStr, err := ffi.LicenseTrialIssue()
	if err != nil {
		return nil, fmt.Errorf("nxuskit: license trial issue failed: %w", err)
	}

	var result TrialResult
	if err := json.Unmarshal([]byte(jsonStr), &result); err != nil {
		return nil, fmt.Errorf("nxuskit: failed to parse license trial issue JSON: %w", err)
	}

	return &result, nil
}

// LicenseTrialActivate activates a trial token (completes email verification).
func LicenseTrialActivate(code string) (*TrialResult, error) {
	jsonStr, err := ffi.LicenseTrialActivate(code)
	if err != nil {
		return nil, fmt.Errorf("nxuskit: license trial activate failed: %w", err)
	}

	var result TrialResult
	if err := json.Unmarshal([]byte(jsonStr), &result); err != nil {
		return nil, fmt.Errorf("nxuskit: failed to parse license trial activate JSON: %w", err)
	}

	return &result, nil
}
