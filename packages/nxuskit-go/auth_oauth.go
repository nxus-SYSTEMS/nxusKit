//go:build nxuskit

package nxuskit

import (
	"encoding/json"
	"fmt"

	"github.com/nxus-SYSTEMS/nxusKit/packages/nxuskit-go/internal/ffi"
)

// OAuthResult is the result of an OAuth authentication flow.
type OAuthResult struct {
	// Success indicates whether the OAuth flow completed successfully
	Success bool `json:"success"`
	// ProviderID identifies the provider that was authenticated
	ProviderID string `json:"provider_id"`
	// Message is a human-readable status message
	Message string `json:"message"`
	// Error message if the flow failed (nil if successful)
	Error *string `json:"error"`
}

// OAuthStatus is the OAuth authentication status for a provider.
type OAuthStatus struct {
	// Authenticated indicates whether an OAuth credential is stored
	Authenticated bool `json:"authenticated"`
	// ProviderID identifies the provider
	ProviderID string `json:"provider_id"`
	// ExpiresAt is the Unix timestamp when the token expires (nil if unknown)
	ExpiresAt *int64 `json:"expires_at"`
	// Scopes granted by the OAuth token (nil if unknown)
	Scopes []string `json:"scopes"`
}

// OAuthStart initiates an OAuth authentication flow for a provider.
//
// This is a blocking call — it launches a browser, starts a localhost
// callback server, and waits for the authorization code.
//
// Set timeoutSecs to 0 for the default timeout (120 seconds).
func OAuthStart(providerID string, timeoutSecs uint32) (*OAuthResult, error) {
	jsonStr, err := ffi.OAuthStart(providerID, timeoutSecs)
	if err != nil {
		return nil, fmt.Errorf("nxuskit: oauth start failed: %w", err)
	}

	var result OAuthResult
	if err := json.Unmarshal([]byte(jsonStr), &result); err != nil {
		return nil, fmt.Errorf("nxuskit: failed to parse oauth start JSON: %w", err)
	}

	return &result, nil
}

// OAuthStatus checks the OAuth authentication status for a provider.
func OAuthGetStatus(providerID string) (*OAuthStatus, error) {
	jsonStr, err := ffi.OAuthStatus(providerID)
	if err != nil {
		return nil, fmt.Errorf("nxuskit: oauth status failed: %w", err)
	}

	var result OAuthStatus
	if err := json.Unmarshal([]byte(jsonStr), &result); err != nil {
		return nil, fmt.Errorf("nxuskit: failed to parse oauth status JSON: %w", err)
	}

	return &result, nil
}

// OAuthRevoke removes the stored OAuth token for a provider.
//
// Returns nil even if no token was stored.
func OAuthRevoke(providerID string) error {
	return ffi.OAuthRevoke(providerID)
}
