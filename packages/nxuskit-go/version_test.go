package nxuskit

import "testing"

func TestVersionMarkerReportsV100(t *testing.T) {
	if Version != "1.0.0" {
		t.Fatalf("Version = %q, want 1.0.0", Version)
	}
}
