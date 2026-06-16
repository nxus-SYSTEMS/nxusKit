package nxuskit

import "testing"

func TestVersionMarkerReportsV104(t *testing.T) {
	if Version != "1.0.4" {
		t.Fatalf("Version = %q, want 1.0.4", Version)
	}
}
