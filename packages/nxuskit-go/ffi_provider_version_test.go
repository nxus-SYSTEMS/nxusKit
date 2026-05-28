//go:build nxuskit

package nxuskit

import "testing"

func TestExpectedNxuskitVersionReportsV100(t *testing.T) {
	if ExpectedNxuskitVersion != "1.0.0" {
		t.Fatalf("ExpectedNxuskitVersion = %q, want 1.0.0", ExpectedNxuskitVersion)
	}
}
