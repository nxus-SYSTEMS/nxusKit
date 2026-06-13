//go:build nxuskit

package nxuskit

import "testing"

func TestExpectedNxuskitVersionReportsV102(t *testing.T) {
	if ExpectedNxuskitVersion != "1.0.2" {
		t.Fatalf("ExpectedNxuskitVersion = %q, want 1.0.2", ExpectedNxuskitVersion)
	}
}
