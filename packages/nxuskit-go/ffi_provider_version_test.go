//go:build nxuskit

package nxuskit

import "testing"

func TestExpectedNxuskitVersionReportsV094(t *testing.T) {
	if ExpectedNxuskitVersion != "0.9.4" {
		t.Fatalf("ExpectedNxuskitVersion = %q, want 0.9.4", ExpectedNxuskitVersion)
	}
}
