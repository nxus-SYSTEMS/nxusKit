package nxuskit

import (
	"os"
	"path/filepath"
	"runtime"
	"testing"
)

func TestMaskedPreviewNormal(t *testing.T) {
	got := MaskedPreview("sk-proj-abc123xyz789")
	want := "sk-...z789"
	if got != want {
		t.Errorf("MaskedPreview = %q, want %q", got, want)
	}
}

func TestMaskedPreviewShort(t *testing.T) {
	got := MaskedPreview("abc")
	want := "****"
	if got != want {
		t.Errorf("MaskedPreview = %q, want %q", got, want)
	}
}

func TestMaskedPreviewBoundary(t *testing.T) {
	got := MaskedPreview("1234567890")
	want := "123...7890"
	if got != want {
		t.Errorf("MaskedPreview = %q, want %q", got, want)
	}
}

func TestLookupKnownProvider(t *testing.T) {
	meta := lookupProvider("openai")
	if meta == nil {
		t.Fatal("expected openai metadata")
		return
	}
	if meta.EnvVarName != "OPENAI_API_KEY" {
		t.Errorf("env var = %q, want OPENAI_API_KEY", meta.EnvVarName)
	}
}

func TestLookupLocalProvider(t *testing.T) {
	meta := lookupProvider("ollama")
	if meta == nil {
		t.Fatal("expected ollama metadata")
		return
	}
	if meta.AuthRequired {
		t.Error("ollama should not require auth")
	}
}

func TestLookupUnknown(t *testing.T) {
	meta := lookupProvider("nonexistent")
	if meta != nil {
		t.Error("expected nil for unknown provider")
	}
}

func TestAuthResolveLocalProvider(t *testing.T) {
	res, err := AuthResolve("ollama", nil)
	if err != nil {
		t.Fatal(err)
	}
	if res.Source != "none" {
		t.Errorf("source = %q, want none", res.Source)
	}
	if res.HasCredential {
		t.Error("local provider should not have credential")
	}
}

func TestAuthResolveExplicitWins(t *testing.T) {
	key := "sk-test"
	res, err := AuthResolve("openai", &key)
	if err != nil {
		t.Fatal(err)
	}
	if res.Source != "explicit" {
		t.Errorf("source = %q, want explicit", res.Source)
	}
	if !res.HasCredential {
		t.Error("expected has_credential=true")
	}
}

func TestAuthResolveEnvWins(t *testing.T) {
	t.Setenv("GROQ_API_KEY", "gsk_test12345678")

	res, err := AuthResolve("groq", nil)
	if err != nil {
		t.Fatal(err)
	}
	if res.Source != "env" {
		t.Errorf("source = %q, want env", res.Source)
	}
}

func TestAuthResolveUnknownProvider(t *testing.T) {
	_, err := AuthResolve("nonexistent", nil)
	if err == nil {
		t.Error("expected error for unknown provider")
	}
}

func TestAuthStatusLocalProvider(t *testing.T) {
	s, err := GetAuthStatus("ollama")
	if err != nil {
		t.Fatal(err)
	}
	if s.Status != "not_required" {
		t.Errorf("status = %q, want not_required", s.Status)
	}
}

func TestAuthStatusEnv(t *testing.T) {
	t.Setenv("GROQ_API_KEY", "gsk_test12345678")

	s, err := GetAuthStatus("groq")
	if err != nil {
		t.Fatal(err)
	}
	if s.Status != "authenticated_env" {
		t.Errorf("status = %q, want authenticated_env", s.Status)
	}
	if s.MaskedPreview != "gsk...5678" {
		t.Errorf("masked = %q, want gsk...5678", s.MaskedPreview)
	}
}

func TestAuthSetCredentialLocalRejected(t *testing.T) {
	err := AuthSetCredential("ollama", "some-key")
	if err == nil {
		t.Error("expected error for local provider")
	}
}

func TestAuthSetCredentialUnknownRejected(t *testing.T) {
	err := AuthSetCredential("nonexistent", "key")
	if err == nil {
		t.Error("expected error for unknown provider")
	}
}

func TestFileFallbackRoundtrip(t *testing.T) {
	tmp := t.TempDir()
	t.Setenv("NXUSKIT_CREDENTIALS_DIR", tmp)

	service := "nxuskit-test-go-roundtrip"
	err := fileSet(service, "test-api-key-go")
	if err != nil {
		t.Fatal(err)
	}

	got := fileGet(service)
	if got != "test-api-key-go" {
		t.Errorf("got %q, want test-api-key-go", got)
	}

	// Verify file permissions (Unix only — Windows has no POSIX permission bits)
	path := filepath.Join(tmp, service+".key")
	info, err := os.Stat(path)
	if err != nil {
		t.Fatal(err)
	}
	if runtime.GOOS != "windows" {
		perm := info.Mode().Perm()
		if perm != 0600 {
			t.Errorf("file permissions = %o, want 0600", perm)
		}
	}

	err = fileDelete(service)
	if err != nil {
		t.Fatal(err)
	}

	got = fileGet(service)
	if got != "" {
		t.Errorf("expected empty after delete, got %q", got)
	}
}

func TestAuthProviders(t *testing.T) {
	providers := AuthProviders()
	if len(providers) < 5 {
		t.Errorf("expected at least 5 providers, got %d", len(providers))
	}
}

func TestAuthStatusAll(t *testing.T) {
	statuses, err := AuthStatusAll()
	if err != nil {
		t.Fatal(err)
	}
	if len(statuses) < 5 {
		t.Errorf("expected at least 5 statuses, got %d", len(statuses))
	}
}

func TestAuthProvidersJSON(t *testing.T) {
	js, err := AuthProvidersJSON()
	if err != nil {
		t.Fatal(err)
	}
	if js == "" || js[0] != '[' {
		t.Errorf("expected JSON array, got %q", js[:20])
	}
}

func TestAuthRemoveCredentialUnknownProvider(t *testing.T) {
	err := AuthRemoveCredential("nonexistent")
	if err == nil {
		t.Error("expected error for unknown provider")
	}
}

func TestAuthRemoveCredentialNoCredential(t *testing.T) {
	// Removing a credential that doesn't exist should return an error
	tmp := t.TempDir()
	t.Setenv("NXUSKIT_CREDENTIALS_DIR", tmp)

	err := AuthRemoveCredential("groq")
	if err == nil {
		t.Error("expected error when no credential stored")
	}
}

func TestAuthResolveNoCredential(t *testing.T) {
	tmp := t.TempDir()
	t.Setenv("NXUSKIT_CREDENTIALS_DIR", tmp)

	res, err := AuthResolve("groq", nil)
	if err != nil {
		t.Fatal(err)
	}
	if res.HasCredential {
		t.Error("expected HasCredential = false when no credential set")
	}
	if res.Source != "none" {
		t.Errorf("source = %q, want none", res.Source)
	}
}

func TestAuthSetAndRemoveWithFileFallback(t *testing.T) {
	tmp := t.TempDir()
	t.Setenv("NXUSKIT_CREDENTIALS_DIR", tmp)

	// Set credential (falls back to file on CI where keyring may not be available)
	err := AuthSetCredential("openai", "sk-test-file-fallback")
	if err != nil {
		t.Fatal(err)
	}

	// Remove should succeed (cleans up either keyring or file)
	err = AuthRemoveCredential("openai")
	if err != nil {
		t.Fatal(err)
	}
}

func TestGetAuthStatusUnknownProvider(t *testing.T) {
	_, err := GetAuthStatus("nonexistent")
	if err == nil {
		t.Error("expected error for unknown provider")
	}
}
