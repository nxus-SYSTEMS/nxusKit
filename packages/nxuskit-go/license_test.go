//go:build nxuskit

package nxuskit

import (
	"testing"
)

// ── T047: Deployment token via env var (055-licensing-client-integration) ──

func TestDeploymentTokenViaEnvVar(t *testing.T) {
	t.Skip("requires ES256-signed deployment token from external licensing client test utilities")
	// TODO: When ES256 test fixtures are available for Go:
	// 1. Set NXUSKIT_LICENSE_TOKEN env var with ES256 deployment token
	// 2. Call LicenseResolve("")
	// 3. Verify result.Valid == true
	// 4. Verify result.ProductID == "nxuskit"
	// 5. Verify result.TokenType == "deployment"
}

// ── T050: License resolve precedence (055-licensing-client-integration) ──

func TestLicenseResolvePrecedence(t *testing.T) {
	t.Skip("requires ES256-signed tokens and full SDK build for FFI")
	// TODO: Test env var > file > API param resolution with ES256 tokens
}

// ── T040: New fields in LicenseResolution (055-licensing-client-integration) ──

func TestLicenseResolveNewFields(t *testing.T) {
	t.Skip("requires full SDK build for FFI")
	// TODO: Verify LicenseResolution includes ProductID, EffectiveLimits, Features
}
