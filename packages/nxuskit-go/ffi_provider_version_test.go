//go:build nxuskit

package nxuskit

import "testing"

func TestExpectedNxuskitVersionReportsV104(t *testing.T) {
	if ExpectedNxuskitVersion != "1.0.4" {
		t.Fatalf("ExpectedNxuskitVersion = %q, want 1.0.4", ExpectedNxuskitVersion)
	}
}
