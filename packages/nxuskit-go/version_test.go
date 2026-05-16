package nxuskit

import "testing"

func TestVersionMarkerReportsV094(t *testing.T) {
	if Version != "0.9.4" {
		t.Fatalf("Version = %q, want 0.9.4", Version)
	}
}
