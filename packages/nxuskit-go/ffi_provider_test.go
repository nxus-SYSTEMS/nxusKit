//go:build nxuskit

package nxuskit

import (
	"encoding/json"
	"strings"
	"testing"
)

func TestFFIProviderConfigLicenseKey(t *testing.T) {
	cfg := ffiProviderConfig{
		ProviderType: "claude",
		LicenseKey:   "test-key-123",
	}
	data, err := json.Marshal(cfg)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(data), `"license_key":"test-key-123"`) {
		t.Errorf("expected license_key in JSON, got %s", string(data))
	}
}

func TestFFIProviderConfigLicenseKeyOmitEmpty(t *testing.T) {
	cfg := ffiProviderConfig{
		ProviderType: "openai",
	}
	data, err := json.Marshal(cfg)
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(data), "license_key") {
		t.Errorf("expected license_key to be omitted when empty, got %s", string(data))
	}
}
