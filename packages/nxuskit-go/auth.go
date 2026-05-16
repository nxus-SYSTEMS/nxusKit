package nxuskit

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"strings"

	"github.com/zalando/go-keyring"
)

// ProviderAuthMetadata describes authentication requirements for a provider.
type ProviderAuthMetadata struct {
	ProviderID            string   `json:"provider_id"`
	DisplayName           string   `json:"display_name"`
	EnvVarName            string   `json:"env_var_name"`
	AuthRequired          bool     `json:"auth_required"`
	DashboardURL          string   `json:"dashboard_url,omitempty"`
	OAuthCapable          bool     `json:"oauth_capable"`
	AuthMethods           []string `json:"auth_methods"`
	CredentialServiceName string   `json:"credential_service_name"`
}

// AuthResolution is the result of resolving a credential.
type AuthResolution struct {
	ProviderID    string `json:"provider_id"`
	Source        string `json:"source"`
	HasCredential bool   `json:"has_credential"`
}

// AuthStatus describes the auth state of a provider.
type AuthStatus struct {
	ProviderID    string `json:"provider_id"`
	Status        string `json:"status"`
	MaskedPreview string `json:"masked_preview,omitempty"`
	Source        string `json:"source,omitempty"`
	DashboardURL  string `json:"dashboard_url,omitempty"`
}

// authProviderRegistry is the static list of known providers.
var authProviderRegistry = []ProviderAuthMetadata{
	{ProviderID: "openai", DisplayName: "OpenAI / GPT", EnvVarName: "OPENAI_API_KEY", AuthRequired: true, DashboardURL: "https://platform.openai.com/api-keys", AuthMethods: []string{"api_key"}, CredentialServiceName: "nxuskit-openai"},
	{ProviderID: "claude", DisplayName: "Anthropic / Claude", EnvVarName: "ANTHROPIC_API_KEY", AuthRequired: true, DashboardURL: "https://console.anthropic.com/settings/keys", AuthMethods: []string{"api_key"}, CredentialServiceName: "nxuskit-claude"},
	{ProviderID: "groq", DisplayName: "Groq", EnvVarName: "GROQ_API_KEY", AuthRequired: true, DashboardURL: "https://console.groq.com/keys", AuthMethods: []string{"api_key"}, CredentialServiceName: "nxuskit-groq"},
	{ProviderID: "xai", DisplayName: "xAI Grok", EnvVarName: "XAI_API_KEY", AuthRequired: true, DashboardURL: "https://console.x.ai/team/default/api-keys", AuthMethods: []string{"api_key"}, CredentialServiceName: "nxuskit-xai"},
	{ProviderID: "ollama", DisplayName: "Ollama", EnvVarName: "OLLAMA_HOST", AuthRequired: false, AuthMethods: []string{}, CredentialServiceName: "nxuskit-ollama"},
	{ProviderID: "lm-studio", DisplayName: "LM Studio", EnvVarName: "LM_STUDIO_HOST", AuthRequired: false, AuthMethods: []string{}, CredentialServiceName: "nxuskit-lm-studio"},
	{ProviderID: "mistral", DisplayName: "Mistral AI", EnvVarName: "MISTRAL_API_KEY", AuthRequired: true, DashboardURL: "https://console.mistral.ai/api-keys", AuthMethods: []string{"api_key"}, CredentialServiceName: "nxuskit-mistral"},
	{ProviderID: "fireworks", DisplayName: "Fireworks AI", EnvVarName: "FIREWORKS_API_KEY", AuthRequired: true, DashboardURL: "https://fireworks.ai/account/api-keys", AuthMethods: []string{"api_key"}, CredentialServiceName: "nxuskit-fireworks"},
	{ProviderID: "together", DisplayName: "Together AI", EnvVarName: "TOGETHER_API_KEY", AuthRequired: true, DashboardURL: "https://api.together.ai/settings/api-keys", AuthMethods: []string{"api_key"}, CredentialServiceName: "nxuskit-together"},
	{ProviderID: "openrouter", DisplayName: "OpenRouter", EnvVarName: "OPENROUTER_API_KEY", AuthRequired: true, DashboardURL: "https://openrouter.ai/settings/keys", AuthMethods: []string{"api_key"}, CredentialServiceName: "nxuskit-openrouter"},
	{ProviderID: "perplexity", DisplayName: "Perplexity", EnvVarName: "PERPLEXITY_API_KEY", AuthRequired: true, DashboardURL: "https://www.perplexity.ai/settings/api", AuthMethods: []string{"api_key"}, CredentialServiceName: "nxuskit-perplexity"},
}

// lookupProvider returns metadata for a known provider, or nil.
func lookupProvider(providerID string) *ProviderAuthMetadata {
	for i := range authProviderRegistry {
		if authProviderRegistry[i].ProviderID == providerID {
			return &authProviderRegistry[i]
		}
	}
	return nil
}

// AuthSetCredential stores a credential for a provider.
// Tries OS keyring first; falls back to file-based storage.
func AuthSetCredential(providerID, apiKey string) error {
	meta := lookupProvider(providerID)
	if meta == nil {
		return fmt.Errorf("unknown provider: %s", providerID)
	}
	if !meta.AuthRequired {
		return fmt.Errorf("provider '%s' does not require authentication", providerID)
	}

	err := keyring.Set(meta.CredentialServiceName, "default", apiKey)
	if err != nil {
		// Fallback to file
		return fileSet(meta.CredentialServiceName, apiKey)
	}
	return nil
}

// AuthRemoveCredential removes a stored credential for a provider.
func AuthRemoveCredential(providerID string) error {
	meta := lookupProvider(providerID)
	if meta == nil {
		return fmt.Errorf("unknown provider: %s", providerID)
	}

	krErr := keyring.Delete(meta.CredentialServiceName, "default")
	fileErr := fileDelete(meta.CredentialServiceName)

	if krErr != nil && fileErr != nil {
		return fmt.Errorf("no credential found for '%s'", providerID)
	}
	return nil
}

// AuthResolve resolves a credential using deterministic precedence.
func AuthResolve(providerID string, explicitKey *string) (*AuthResolution, error) {
	meta := lookupProvider(providerID)
	if meta == nil {
		return nil, fmt.Errorf("unknown provider: %s", providerID)
	}

	if !meta.AuthRequired {
		return &AuthResolution{ProviderID: providerID, Source: "none", HasCredential: false}, nil
	}

	// 1. Explicit
	if explicitKey != nil {
		return &AuthResolution{ProviderID: providerID, Source: "explicit", HasCredential: true}, nil
	}

	// 2. Env
	if _, ok := os.LookupEnv(meta.EnvVarName); ok {
		return &AuthResolution{ProviderID: providerID, Source: "env", HasCredential: true}, nil
	}

	// 3. Store
	if _, err := keyring.Get(meta.CredentialServiceName, "default"); err == nil {
		return &AuthResolution{ProviderID: providerID, Source: "store", HasCredential: true}, nil
	}
	if val := fileGet(meta.CredentialServiceName); val != "" {
		return &AuthResolution{ProviderID: providerID, Source: "store", HasCredential: true}, nil
	}

	return &AuthResolution{ProviderID: providerID, Source: "none", HasCredential: false}, nil
}

// GetAuthStatus returns auth status for a single provider.
func GetAuthStatus(providerID string) (*AuthStatus, error) {
	meta := lookupProvider(providerID)
	if meta == nil {
		return nil, fmt.Errorf("unknown provider: %s", providerID)
	}

	if !meta.AuthRequired {
		return &AuthStatus{
			ProviderID:   providerID,
			Status:       "not_required",
			DashboardURL: meta.DashboardURL,
		}, nil
	}

	// Env
	if val, ok := os.LookupEnv(meta.EnvVarName); ok {
		return &AuthStatus{
			ProviderID:    providerID,
			Status:        "authenticated_env",
			MaskedPreview: MaskedPreview(val),
			Source:        "env",
			DashboardURL:  meta.DashboardURL,
		}, nil
	}

	// Store
	if val, err := keyring.Get(meta.CredentialServiceName, "default"); err == nil {
		return &AuthStatus{
			ProviderID:    providerID,
			Status:        "authenticated_store",
			MaskedPreview: MaskedPreview(val),
			Source:        "store",
			DashboardURL:  meta.DashboardURL,
		}, nil
	}
	if val := fileGet(meta.CredentialServiceName); val != "" {
		return &AuthStatus{
			ProviderID:    providerID,
			Status:        "authenticated_store",
			MaskedPreview: MaskedPreview(val),
			Source:        "store",
			DashboardURL:  meta.DashboardURL,
		}, nil
	}

	return &AuthStatus{
		ProviderID:   providerID,
		Status:       "not_authenticated",
		DashboardURL: meta.DashboardURL,
	}, nil
}

// AuthStatusAll returns auth status for all known providers.
func AuthStatusAll() ([]AuthStatus, error) {
	var result []AuthStatus
	for _, meta := range authProviderRegistry {
		s, err := GetAuthStatus(meta.ProviderID)
		if err != nil {
			continue
		}
		result = append(result, *s)
	}
	return result, nil
}

// AuthProviders returns metadata for all known providers.
func AuthProviders() []ProviderAuthMetadata {
	cp := make([]ProviderAuthMetadata, len(authProviderRegistry))
	copy(cp, authProviderRegistry)
	return cp
}

// AuthProvidersJSON returns metadata for all known providers as JSON.
func AuthProvidersJSON() (string, error) {
	b, err := json.Marshal(authProviderRegistry)
	if err != nil {
		return "", err
	}
	return string(b), nil
}

// MaskedPreview returns a masked credential preview.
// Shows first 3 + "..." + last 4 chars. Short keys show "****".
func MaskedPreview(key string) string {
	if len(key) < 10 {
		return "****"
	}
	return key[:3] + "..." + key[len(key)-4:]
}

// ── File-based fallback ──────────────────────────────────────────────

func credentialDir() string {
	if dir := os.Getenv("NXUSKIT_CREDENTIALS_DIR"); dir != "" {
		return dir
	}
	home := os.Getenv("HOME")
	if runtime.GOOS == "windows" {
		home = os.Getenv("USERPROFILE")
	}
	if home != "" {
		return filepath.Join(home, ".nxuskit", "credentials")
	}
	return filepath.Join(os.TempDir(), ".nxuskit-credentials")
}

func fileSet(service, value string) error {
	dir := credentialDir()
	if err := os.MkdirAll(dir, 0700); err != nil {
		return fmt.Errorf("create credential dir: %w", err)
	}
	path := filepath.Join(dir, service+".key")
	return os.WriteFile(path, []byte(value), 0600)
}

func fileGet(service string) string {
	path := filepath.Join(credentialDir(), service+".key")
	b, err := os.ReadFile(path)
	if err != nil {
		return ""
	}
	return strings.TrimSpace(string(b))
}

func fileDelete(service string) error {
	path := filepath.Join(credentialDir(), service+".key")
	if _, err := os.Stat(path); os.IsNotExist(err) {
		return fmt.Errorf("no file credential")
	}
	return os.Remove(path)
}
