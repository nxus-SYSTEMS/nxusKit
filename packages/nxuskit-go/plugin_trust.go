//go:build nxuskit

package nxuskit

import (
	"fmt"

	"github.com/nxus-SYSTEMS/nxusKit/packages/nxuskit-go/internal/ffi"
)

// TrustMode controls whether unsigned plugins are allowed to load.
type TrustMode int32

const (
	// TrustModeSignedOnly only allows cryptographically signed plugins (default).
	TrustModeSignedOnly TrustMode = 0
	// TrustModeAllowUnsigned allows unsigned plugins but emits audit events.
	TrustModeAllowUnsigned TrustMode = 1
)

// String returns a human-readable representation of the trust mode.
func (m TrustMode) String() string {
	switch m {
	case TrustModeSignedOnly:
		return "signed-only"
	case TrustModeAllowUnsigned:
		return "allow-unsigned"
	default:
		return "unknown"
	}
}

// SetPluginTrustMode sets the global plugin trust mode.
func SetPluginTrustMode(mode TrustMode) error {
	result := ffi.PluginSetTrustMode(int32(mode))
	if result < 0 {
		return fmt.Errorf("nxuskit: invalid trust mode: %d", mode)
	}
	return nil
}

// GetPluginTrustMode returns the current plugin trust mode.
func GetPluginTrustMode() TrustMode {
	return TrustMode(ffi.PluginGetTrustMode())
}

// LoadPluginsTrusted loads plugins from a directory using the current trust mode.
func LoadPluginsTrusted(dir string) (int, error) {
	count, err := ffi.PluginLoadDirTrusted(dir)
	if err != nil {
		return 0, fmt.Errorf("nxuskit: plugin load failed: %w", err)
	}
	return int(count), nil
}
