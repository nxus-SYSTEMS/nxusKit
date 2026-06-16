package nxuskit

import "testing"

func TestVersionMarkerReportsV105(t *testing.T) {
	if Version != "1.0.5" {
		t.Fatalf("Version = %q, want 1.0.5", Version)
	}
}
