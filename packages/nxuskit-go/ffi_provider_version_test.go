//go:build nxuskit

package nxuskit

import "testing"

func TestExpectedNxuskitVersionReportsV105(t *testing.T) {
	if ExpectedNxuskitVersion != "1.0.5" {
		t.Fatalf("ExpectedNxuskitVersion = %q, want 1.0.5", ExpectedNxuskitVersion)
	}
}
