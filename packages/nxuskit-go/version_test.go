package nxuskit

import "testing"

func TestVersionMarkerReportsV200(t *testing.T) {
	if Version != "2.0.0" {
		t.Fatalf("Version = %q, want 2.0.0", Version)
	}
}
