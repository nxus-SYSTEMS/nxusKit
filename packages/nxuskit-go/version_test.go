package nxuskit

import "testing"

func TestVersionMarkerReportsV102(t *testing.T) {
	if Version != "1.0.2" {
		t.Fatalf("Version = %q, want 1.0.2", Version)
	}
}
