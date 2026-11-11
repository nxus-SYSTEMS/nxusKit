//go:build nxuskit

package nxuskit

import "testing"

func TestExpectedNxuskitVersionReportsV110(t *testing.T) {
	if ExpectedNxuskitVersion != "2.0.0" {
		t.Fatalf("ExpectedNxuskitVersion = %q, want 2.0.0", ExpectedNxuskitVersion)
	}
}
