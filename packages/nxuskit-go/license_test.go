//go:build nxuskit

package nxuskit

import (
	"crypto/ecdsa"
	"crypto/elliptic"
	"crypto/rand"
	"crypto/sha256"
	"crypto/x509"
	"encoding/base64"
	"encoding/json"
	"encoding/pem"
	"math/big"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

var v2FixtureSeed = []byte{
	0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08,
	0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10,
	0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18,
	0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20,
}

func v2FixtureToken(t *testing.T, kid string, mutate func(map[string]any)) (string, string) {
	t.Helper()
	claimsBytes, err := os.ReadFile(filepath.Join("..", "..", "tests", "fixtures", "licensing", "deployment-claims-v2.json"))
	if err != nil {
		t.Fatalf("read frozen Licensing fixture: %v", err)
	}
	claims := map[string]any{}
	if err := json.Unmarshal(claimsBytes, &claims); err != nil {
		t.Fatalf("decode frozen Licensing fixture: %v", err)
	}
	if mutate != nil {
		mutate(claims)
	}

	curve := elliptic.P256()
	d := new(big.Int).SetBytes(v2FixtureSeed)
	x, y := curve.ScalarBaseMult(d.Bytes())
	privateKey := &ecdsa.PrivateKey{PublicKey: ecdsa.PublicKey{Curve: curve, X: x, Y: y}, D: d}
	publicDER, err := x509.MarshalPKIXPublicKey(&privateKey.PublicKey)
	if err != nil {
		t.Fatalf("encode deterministic public key: %v", err)
	}
	publicPEM := string(pem.EncodeToMemory(&pem.Block{Type: "PUBLIC KEY", Bytes: publicDER}))

	headerJSON, err := json.Marshal(map[string]string{"alg": "ES256", "typ": "JWT", "kid": kid})
	if err != nil {
		t.Fatalf("encode header: %v", err)
	}
	payloadJSON, err := json.Marshal(claims)
	if err != nil {
		t.Fatalf("encode claims: %v", err)
	}
	unsigned := base64.RawURLEncoding.EncodeToString(headerJSON) + "." + base64.RawURLEncoding.EncodeToString(payloadJSON)
	digest := sha256.Sum256([]byte(unsigned))
	r, s, err := ecdsa.Sign(rand.Reader, privateKey, digest[:])
	if err != nil {
		t.Fatalf("sign fixture: %v", err)
	}
	signature := make([]byte, 64)
	r.FillBytes(signature[:32])
	s.FillBytes(signature[32:])
	return unsigned + "." + base64.RawURLEncoding.EncodeToString(signature), publicPEM
}

// ── T047: Deployment token via environment ───────────────────────────

func TestDeploymentTokenViaEnvVar(t *testing.T) {
	t.Skip("requires an ES256-signed deployment token fixture")
	// TODO: When ES256 test fixtures are available for Go:
	// 1. Set NXUSKIT_LICENSE_TOKEN env var with ES256 deployment token
	// 2. Call LicenseResolve("")
	// 3. Verify result.Valid == true
	// 4. Verify result.ProductID == "nxuskit"
	// 5. Verify result.TokenType == "deployment"
}

// ── T050: License resolve precedence ──────────────────────────────────

func TestLicenseResolvePrecedence(t *testing.T) {
	t.Skip("requires ES256-signed tokens and full SDK build for FFI")
	// TODO: Test env var > file > API param resolution with ES256 tokens
}

// ── T040: New fields in LicenseResolution ─────────────────────────────

func TestLicenseResolveNewFields(t *testing.T) {
	t.Skip("requires full SDK build for FFI")
	// TODO: Verify LicenseResolution includes ProductID, EffectiveLimits, Features
}

// W540: the C ABI adds only PII-free v2 deployment inspection facts.
func TestV2DeploymentTokenInfoSafeMetadataIsAdditive(t *testing.T) {
	const rawTokenMarker = "header.payload.signature"
	token, publicPEM := v2FixtureToken(t, "es256-v1", nil)
	keyPath := filepath.Join(t.TempDir(), "deployment-v2-public.pem")
	if err := os.WriteFile(keyPath, []byte(publicPEM), 0o600); err != nil {
		t.Fatalf("write deterministic public key: %v", err)
	}
	t.Setenv("NXUS_SIGNING_KEY_PATH", keyPath)

	info, err := LicenseValidate(token)
	if err != nil {
		t.Fatalf("validate frozen v2 fixture through Go boundary: %v", err)
	}
	if info.SDKReleaseLine == nil || *info.SDKReleaseLine != 2 {
		t.Fatalf("expected release line 2, got %#v", info.SDKReleaseLine)
	}
	if info.GrantRevision == nil || *info.GrantRevision != 3 {
		t.Fatalf("expected grant revision 3, got %#v", info.GrantRevision)
	}
	if info.AuthorizationScope == nil || *info.AuthorizationScope != "client_entitlement" {
		t.Fatalf("unexpected authorization scope: %#v", info.AuthorizationScope)
	}
	if info.TokenFingerprint == nil || strings.Contains(strings.TrimSpace(*info.TokenFingerprint), rawTokenMarker) || strings.Contains(strings.TrimSpace(*info.TokenFingerprint), token) {
		t.Fatal("safe token information must never expose a compact token")
	}
}

func TestV2DeploymentTokenRejectsCrossLineFixtureWithStableClass(t *testing.T) {
	token, publicPEM := v2FixtureToken(t, "es256-v1", func(claims map[string]any) {
		claims["sdk_release_line"] = 3
	})
	keyPath := filepath.Join(t.TempDir(), "deployment-v2-public.pem")
	if err := os.WriteFile(keyPath, []byte(publicPEM), 0o600); err != nil {
		t.Fatalf("write deterministic public key: %v", err)
	}
	t.Setenv("NXUS_SIGNING_KEY_PATH", keyPath)

	info, err := LicenseValidate(token)
	if err != nil {
		t.Fatalf("C ABI returns a structured v2 rejection, not a transport error: %v", err)
	}
	if info.Valid || info.Result != "incompatible_release_line" {
		t.Fatalf("expected incompatible release line, got %+v", info)
	}
	if info.Error != nil && strings.Contains(*info.Error, token) {
		t.Fatal("rejected Go token result must not echo the compact token")
	}
}
