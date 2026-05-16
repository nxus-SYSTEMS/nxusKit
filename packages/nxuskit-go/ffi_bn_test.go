//go:build nxuskit

package nxuskit

import (
	"encoding/json"
	"math"
	"os"
	"path/filepath"
	"testing"
)

// fixturePath resolves a BN fixture file path.
func fixturePath(t *testing.T, name string) string {
	t.Helper()
	// From packages/nxuskit, navigate to sibling nxuskit-engine crate
	path := filepath.Join("..", "nxuskit-engine", "crates", "nxuskit-engine", "tests", "fixtures", "bn", name)
	abs, err := filepath.Abs(path)
	if err != nil {
		t.Fatalf("failed to resolve fixture path: %v", err)
	}
	if _, err := os.Stat(abs); os.IsNotExist(err) {
		t.Skipf("fixture not found: %s", abs)
	}
	return abs
}

// ── Network Lifecycle Tests ─────────────────────────────────────────

func TestBnNetworkCreateEmpty(t *testing.T) {
	net, err := NewBnNetwork()
	if err != nil {
		t.Fatalf("NewBnNetwork: %v", err)
	}
	defer net.Close()
	if net.NumVariables() != 0 {
		t.Errorf("expected 0 variables, got %d", net.NumVariables())
	}
}

func TestBnNetworkLoadFile(t *testing.T) {
	path := fixturePath(t, "asia.bif")
	net, err := LoadBnNetwork(path)
	if err != nil {
		t.Fatalf("LoadBnNetwork: %v", err)
	}
	defer net.Close()
	if net.NumVariables() != 8 {
		t.Errorf("expected 8 variables, got %d", net.NumVariables())
	}
}

func TestBnNetworkLoadFileNonexistent(t *testing.T) {
	_, err := LoadBnNetwork("nonexistent.bif")
	if err == nil {
		t.Fatal("expected error for nonexistent file")
	}
}

func TestBnNetworkVariables(t *testing.T) {
	path := fixturePath(t, "asia.bif")
	net, err := LoadBnNetwork(path)
	if err != nil {
		t.Fatalf("LoadBnNetwork: %v", err)
	}
	defer net.Close()

	vars, err := net.Variables()
	if err != nil {
		t.Fatalf("Variables: %v", err)
	}
	if len(vars) != 8 {
		t.Errorf("expected 8 variables, got %d", len(vars))
	}
	found := false
	for _, v := range vars {
		if v == "Smoking" {
			found = true
			break
		}
	}
	if !found {
		t.Error("expected 'Smoking' in variable list")
	}
}

func TestBnNetworkVariableStates(t *testing.T) {
	path := fixturePath(t, "asia.bif")
	net, err := LoadBnNetwork(path)
	if err != nil {
		t.Fatalf("LoadBnNetwork: %v", err)
	}
	defer net.Close()

	states, err := net.VariableStates("Smoking")
	if err != nil {
		t.Fatalf("VariableStates: %v", err)
	}
	if len(states) != 2 {
		t.Errorf("expected 2 states, got %d", len(states))
	}
}

// ── BIF Export Tests ────────────────────────────────────────────────

func TestBnNetworkSaveFile(t *testing.T) {
	path := fixturePath(t, "asia.bif")
	net, err := LoadBnNetwork(path)
	if err != nil {
		t.Fatalf("LoadBnNetwork: %v", err)
	}
	defer net.Close()

	tmp := filepath.Join(t.TempDir(), "asia_export.bif")
	if err := net.SaveFile(tmp); err != nil {
		t.Fatalf("SaveFile: %v", err)
	}

	info, err := os.Stat(tmp)
	if err != nil {
		t.Fatalf("exported file missing: %v", err)
	}
	if info.Size() == 0 {
		t.Error("exported BIF file is empty")
	}

	// Round-trip
	reloaded, err := LoadBnNetwork(tmp)
	if err != nil {
		t.Fatalf("reload: %v", err)
	}
	defer reloaded.Close()
	if reloaded.NumVariables() != 8 {
		t.Errorf("expected 8 variables after round-trip, got %d", reloaded.NumVariables())
	}
}

// ── Gaussian Variable Tests ─────────────────────────────────────────

func TestBnAddGaussianVariable(t *testing.T) {
	net, err := NewBnNetwork()
	if err != nil {
		t.Fatalf("NewBnNetwork: %v", err)
	}
	defer net.Close()

	if err := net.AddGaussianVariable("X", 0.0, 1.0); err != nil {
		t.Fatalf("AddGaussianVariable: %v", err)
	}
}

func TestBnSetGaussianWeight(t *testing.T) {
	net, err := NewBnNetwork()
	if err != nil {
		t.Fatalf("NewBnNetwork: %v", err)
	}
	defer net.Close()

	if err := net.AddGaussianVariable("X", 0.0, 1.0); err != nil {
		t.Fatal(err)
	}
	if err := net.AddGaussianVariable("Y", 0.0, 1.0); err != nil {
		t.Fatal(err)
	}
	if err := net.SetGaussianWeight("Y", "X", 0.5); err != nil {
		t.Fatalf("SetGaussianWeight: %v", err)
	}
}

// ── Evidence Tests ──────────────────────────────────────────────────

func TestBnEvidenceSetDiscrete(t *testing.T) {
	path := fixturePath(t, "asia.bif")
	net, err := LoadBnNetwork(path)
	if err != nil {
		t.Fatalf("LoadBnNetwork: %v", err)
	}
	defer net.Close()

	ev, err := NewBnEvidence()
	if err != nil {
		t.Fatalf("NewBnEvidence: %v", err)
	}
	defer ev.Close()

	if err := ev.SetDiscrete(net, "Smoking", "yes"); err != nil {
		t.Fatalf("SetDiscrete: %v", err)
	}
}

func TestBnEvidenceSetContinuous(t *testing.T) {
	net, err := NewBnNetwork()
	if err != nil {
		t.Fatalf("NewBnNetwork: %v", err)
	}
	defer net.Close()

	if err := net.AddGaussianVariable("X", 0.0, 1.0); err != nil {
		t.Fatal(err)
	}

	ev, err := NewBnEvidence()
	if err != nil {
		t.Fatalf("NewBnEvidence: %v", err)
	}
	defer ev.Close()

	if err := ev.SetContinuous(net, "X", 2.5); err != nil {
		t.Fatalf("SetContinuous: %v", err)
	}
}

func TestBnEvidenceRetract(t *testing.T) {
	path := fixturePath(t, "asia.bif")
	net, err := LoadBnNetwork(path)
	if err != nil {
		t.Fatal(err)
	}
	defer net.Close()

	ev, err := NewBnEvidence()
	if err != nil {
		t.Fatal(err)
	}
	defer ev.Close()

	if err := ev.SetDiscrete(net, "Smoking", "yes"); err != nil {
		t.Fatal(err)
	}
	if err := ev.Retract("Smoking"); err != nil {
		t.Fatalf("Retract: %v", err)
	}
}

func TestBnEvidenceClear(t *testing.T) {
	path := fixturePath(t, "asia.bif")
	net, err := LoadBnNetwork(path)
	if err != nil {
		t.Fatal(err)
	}
	defer net.Close()

	ev, err := NewBnEvidence()
	if err != nil {
		t.Fatal(err)
	}
	defer ev.Close()

	if err := ev.SetDiscrete(net, "Smoking", "yes"); err != nil {
		t.Fatal(err)
	}
	if err := ev.Clear(); err != nil {
		t.Fatalf("Clear: %v", err)
	}
}

// ── Inference Tests ─────────────────────────────────────────────────

func TestBnInferVE(t *testing.T) {
	path := fixturePath(t, "asia.bif")
	net, err := LoadBnNetwork(path)
	if err != nil {
		t.Fatal(err)
	}
	defer net.Close()

	ev, err := NewBnEvidence()
	if err != nil {
		t.Fatal(err)
	}
	defer ev.Close()

	result, err := net.Infer(ev, "ve")
	if err != nil {
		t.Fatalf("Infer(ve): %v", err)
	}
	defer result.Close()

	if result.NumVariables() != 8 {
		t.Errorf("expected 8 variables, got %d", result.NumVariables())
	}
}

func TestBnInferVEWithEvidence(t *testing.T) {
	path := fixturePath(t, "asia.bif")
	net, err := LoadBnNetwork(path)
	if err != nil {
		t.Fatal(err)
	}
	defer net.Close()

	ev, err := NewBnEvidence()
	if err != nil {
		t.Fatal(err)
	}
	defer ev.Close()
	if err := ev.SetDiscrete(net, "Smoking", "yes"); err != nil {
		t.Fatal(err)
	}

	result, err := net.Infer(ev, "ve")
	if err != nil {
		t.Fatalf("Infer: %v", err)
	}
	defer result.Close()

	dist, err := result.Marginal("Bronchitis")
	if err != nil {
		t.Fatalf("Marginal: %v", err)
	}
	pPresent, ok := dist["present"]
	if !ok {
		t.Fatal("expected 'present' state in Bronchitis marginal")
	}
	if pPresent <= 0.5 {
		t.Errorf("P(Bronchitis=present|Smoking=yes) should be > 0.5, got %f", pPresent)
	}
}

func TestBnInferJT(t *testing.T) {
	path := fixturePath(t, "asia.bif")
	net, err := LoadBnNetwork(path)
	if err != nil {
		t.Fatal(err)
	}
	defer net.Close()

	ev, err := NewBnEvidence()
	if err != nil {
		t.Fatal(err)
	}
	defer ev.Close()

	result, err := net.Infer(ev, "jt")
	if err != nil {
		t.Fatalf("Infer(jt): %v", err)
	}
	defer result.Close()

	if result.NumVariables() != 8 {
		t.Errorf("expected 8 variables, got %d", result.NumVariables())
	}
}

func TestBnInferGibbs(t *testing.T) {
	path := fixturePath(t, "asia.bif")
	net, err := LoadBnNetwork(path)
	if err != nil {
		t.Fatal(err)
	}
	defer net.Close()

	ev, err := NewBnEvidence()
	if err != nil {
		t.Fatal(err)
	}
	defer ev.Close()

	result, err := net.InferWithOptions(ev, "gibbs", 5000, 500, 42)
	if err != nil {
		t.Fatalf("InferWithOptions(gibbs): %v", err)
	}
	defer result.Close()

	if result.NumVariables() != 8 {
		t.Errorf("expected 8 variables, got %d", result.NumVariables())
	}
}

func TestBnInferLBP(t *testing.T) {
	path := fixturePath(t, "asia.bif")
	net, err := LoadBnNetwork(path)
	if err != nil {
		t.Fatal(err)
	}
	defer net.Close()

	ev, err := NewBnEvidence()
	if err != nil {
		t.Fatal(err)
	}
	defer ev.Close()

	result, err := net.Infer(ev, "lbp")
	if err != nil {
		t.Fatalf("Infer(lbp): %v", err)
	}
	defer result.Close()

	if result.NumVariables() != 8 {
		t.Errorf("expected 8 variables, got %d", result.NumVariables())
	}
}

// ── InferWithConfig Tests ───────────────────────────────────────────

func TestBnInferLBPWithConfig(t *testing.T) {
	path := fixturePath(t, "asia.bif")
	net, err := LoadBnNetwork(path)
	if err != nil {
		t.Fatal(err)
	}
	defer net.Close()

	ev, err := NewBnEvidence()
	if err != nil {
		t.Fatal(err)
	}
	defer ev.Close()

	config := `{"max_iterations": 200, "damping": 0.3, "convergence_threshold": 1e-8}`
	result, err := net.InferWithConfig(ev, "lbp", config)
	if err != nil {
		t.Fatalf("InferWithConfig(lbp): %v", err)
	}
	defer result.Close()

	if result.NumVariables() != 8 {
		t.Errorf("expected 8 variables, got %d", result.NumVariables())
	}
}

func TestBnInferGibbsWithConfig(t *testing.T) {
	path := fixturePath(t, "asia.bif")
	net, err := LoadBnNetwork(path)
	if err != nil {
		t.Fatal(err)
	}
	defer net.Close()

	ev, err := NewBnEvidence()
	if err != nil {
		t.Fatal(err)
	}
	defer ev.Close()

	config := `{"num_samples": 5000, "burn_in": 500, "seed": 42}`
	result, err := net.InferWithConfig(ev, "gibbs", config)
	if err != nil {
		t.Fatalf("InferWithConfig(gibbs): %v", err)
	}
	defer result.Close()

	if result.NumVariables() != 8 {
		t.Errorf("expected 8 variables, got %d", result.NumVariables())
	}
}

func TestBnInferNUTS(t *testing.T) {
	net, err := NewBnNetwork()
	if err != nil {
		t.Fatal(err)
	}
	defer net.Close()

	if err := net.AddGaussianVariable("X", 0.0, 1.0); err != nil {
		t.Fatal(err)
	}
	if err := net.AddGaussianVariable("Y", 0.0, 1.0); err != nil {
		t.Fatal(err)
	}
	if err := net.SetGaussianWeight("Y", "X", 0.8); err != nil {
		t.Fatal(err)
	}

	ev, err := NewBnEvidence()
	if err != nil {
		t.Fatal(err)
	}
	defer ev.Close()

	config := `{"num_samples": 500, "num_tune": 200, "seed": 42}`
	result, err := net.InferWithConfig(ev, "nuts", config)
	if err != nil {
		t.Fatalf("InferWithConfig(nuts): %v", err)
	}
	defer result.Close()

	// NUTS result should have continuous marginals
	jsonStr, err := result.JSON()
	if err != nil {
		t.Fatalf("JSON: %v", err)
	}
	var parsed map[string]interface{}
	if err := json.Unmarshal([]byte(jsonStr), &parsed); err != nil {
		t.Fatalf("JSON parse: %v", err)
	}
	if _, ok := parsed["continuous_marginals"]; !ok {
		t.Error("expected continuous_marginals in NUTS result JSON")
	}
}

// ── Result Access Tests ─────────────────────────────────────────────

func TestBnResultJSON(t *testing.T) {
	path := fixturePath(t, "asia.bif")
	net, err := LoadBnNetwork(path)
	if err != nil {
		t.Fatal(err)
	}
	defer net.Close()

	ev, err := NewBnEvidence()
	if err != nil {
		t.Fatal(err)
	}
	defer ev.Close()

	result, err := net.Infer(ev, "ve")
	if err != nil {
		t.Fatal(err)
	}
	defer result.Close()

	jsonStr, err := result.JSON()
	if err != nil {
		t.Fatalf("JSON: %v", err)
	}
	var parsed map[string]interface{}
	if err := json.Unmarshal([]byte(jsonStr), &parsed); err != nil {
		t.Fatalf("JSON parse: %v", err)
	}
	if _, ok := parsed["marginals"]; !ok {
		t.Error("expected 'marginals' in result JSON")
	}
	if algo, ok := parsed["algorithm"].(string); !ok || algo != "ve" {
		t.Errorf("expected algorithm 've', got %v", parsed["algorithm"])
	}
}

func TestBnResultIteration(t *testing.T) {
	path := fixturePath(t, "asia.bif")
	net, err := LoadBnNetwork(path)
	if err != nil {
		t.Fatal(err)
	}
	defer net.Close()

	ev, err := NewBnEvidence()
	if err != nil {
		t.Fatal(err)
	}
	defer ev.Close()

	result, err := net.Infer(ev, "ve")
	if err != nil {
		t.Fatal(err)
	}
	defer result.Close()

	names := result.VariableNames()
	if len(names) != 8 {
		t.Errorf("expected 8 variable names, got %d", len(names))
	}

	// Reset and iterate again
	names2 := result.VariableNames()
	if len(names) != len(names2) {
		t.Error("Reset should produce same iteration count")
	}
}

// ── VE vs JT Cross-Validation ───────────────────────────────────────

func TestBnVEJTAgreement(t *testing.T) {
	path := fixturePath(t, "asia.bif")
	net, err := LoadBnNetwork(path)
	if err != nil {
		t.Fatal(err)
	}
	defer net.Close()

	ev, err := NewBnEvidence()
	if err != nil {
		t.Fatal(err)
	}
	defer ev.Close()
	if err := ev.SetDiscrete(net, "Smoking", "yes"); err != nil {
		t.Fatal(err)
	}

	resultVE, err := net.Infer(ev, "ve")
	if err != nil {
		t.Fatal(err)
	}
	defer resultVE.Close()

	resultJT, err := net.Infer(ev, "jt")
	if err != nil {
		t.Fatal(err)
	}
	defer resultJT.Close()

	veD, err := resultVE.Marginal("Bronchitis")
	if err != nil {
		t.Fatal(err)
	}
	jtD, err := resultJT.Marginal("Bronchitis")
	if err != nil {
		t.Fatal(err)
	}

	for state, pVE := range veD {
		pJT, ok := jtD[state]
		if !ok {
			t.Errorf("state %q in VE but not JT", state)
			continue
		}
		if math.Abs(pVE-pJT) > 1e-6 {
			t.Errorf("VE vs JT mismatch for Bronchitis[%s]: %f vs %f", state, pVE, pJT)
		}
	}
}

// ── LBP vs VE Approximate Agreement ────────────────────────────────

func TestBnLBPVEAgreement(t *testing.T) {
	path := fixturePath(t, "asia.bif")
	net, err := LoadBnNetwork(path)
	if err != nil {
		t.Fatal(err)
	}
	defer net.Close()

	ev, err := NewBnEvidence()
	if err != nil {
		t.Fatal(err)
	}
	defer ev.Close()
	if err := ev.SetDiscrete(net, "Smoking", "yes"); err != nil {
		t.Fatal(err)
	}

	resultVE, err := net.Infer(ev, "ve")
	if err != nil {
		t.Fatal(err)
	}
	defer resultVE.Close()

	resultLBP, err := net.Infer(ev, "lbp")
	if err != nil {
		t.Fatal(err)
	}
	defer resultLBP.Close()

	veD, err := resultVE.Marginal("Bronchitis")
	if err != nil {
		t.Fatal(err)
	}
	lbpD, err := resultLBP.Marginal("Bronchitis")
	if err != nil {
		t.Fatal(err)
	}

	for state, pVE := range veD {
		pLBP, ok := lbpD[state]
		if !ok {
			t.Errorf("state %q in VE but not LBP", state)
			continue
		}
		// LBP is approximate — wider tolerance
		if math.Abs(pVE-pLBP) > 0.05 {
			t.Errorf("VE vs LBP mismatch for Bronchitis[%s]: %f vs %f", state, pVE, pLBP)
		}
	}
}

// ── Continuous Marginal Tests ───────────────────────────────────────

func TestBnResultMeanVariance(t *testing.T) {
	net, err := NewBnNetwork()
	if err != nil {
		t.Fatal(err)
	}
	defer net.Close()

	if err := net.AddGaussianVariable("X", 5.0, 2.0); err != nil {
		t.Fatal(err)
	}

	ev, err := NewBnEvidence()
	if err != nil {
		t.Fatal(err)
	}
	defer ev.Close()

	config := `{"num_samples": 1000, "num_tune": 500, "seed": 42}`
	result, err := net.InferWithConfig(ev, "nuts", config)
	if err != nil {
		t.Fatalf("InferWithConfig: %v", err)
	}
	defer result.Close()

	mean, err := result.Mean("X")
	if err != nil {
		t.Fatalf("Mean: %v", err)
	}
	if math.Abs(mean-5.0) > 2.0 {
		t.Errorf("posterior mean should be near 5.0, got %f", mean)
	}

	variance, err := result.Variance("X")
	if err != nil {
		t.Fatalf("Variance: %v", err)
	}
	if variance <= 0 {
		t.Errorf("variance should be positive, got %f", variance)
	}
}

func TestBnResultContinuousMarginal(t *testing.T) {
	net, err := NewBnNetwork()
	if err != nil {
		t.Fatal(err)
	}
	defer net.Close()

	if err := net.AddGaussianVariable("X", 0.0, 1.0); err != nil {
		t.Fatal(err)
	}

	ev, err := NewBnEvidence()
	if err != nil {
		t.Fatal(err)
	}
	defer ev.Close()

	config := `{"num_samples": 500, "num_tune": 200, "seed": 42}`
	result, err := net.InferWithConfig(ev, "nuts", config)
	if err != nil {
		t.Fatalf("InferWithConfig: %v", err)
	}
	defer result.Close()

	marginal, err := result.ContinuousMarginalResult("X")
	if err != nil {
		t.Fatalf("ContinuousMarginalResult: %v", err)
	}

	if marginal.Variance <= 0 {
		t.Errorf("variance should be positive, got %f", marginal.Variance)
	}
	if marginal.CILower >= marginal.Mean {
		t.Error("CI lower should be below mean")
	}
	if marginal.CIUpper <= marginal.Mean {
		t.Error("CI upper should be above mean")
	}
}

// ── Alarm Network (37 nodes) ────────────────────────────────────────

func TestBnAlarmNetwork(t *testing.T) {
	path := fixturePath(t, "alarm.bif")
	net, err := LoadBnNetwork(path)
	if err != nil {
		t.Fatal(err)
	}
	defer net.Close()

	if net.NumVariables() != 37 {
		t.Errorf("expected 37 variables, got %d", net.NumVariables())
	}

	ev, err := NewBnEvidence()
	if err != nil {
		t.Fatal(err)
	}
	defer ev.Close()

	result, err := net.Infer(ev, "ve")
	if err != nil {
		t.Fatalf("Infer: %v", err)
	}
	defer result.Close()

	if result.NumVariables() != 37 {
		t.Errorf("expected 37 variables in result, got %d", result.NumVariables())
	}
}

// ── Streaming Tests ─────────────────────────────────────────────────

func TestBnInferStreamGibbs(t *testing.T) {
	path := fixturePath(t, "asia.bif")
	net, err := LoadBnNetwork(path)
	if err != nil {
		t.Fatal(err)
	}
	defer net.Close()

	ev, err := NewBnEvidence()
	if err != nil {
		t.Fatal(err)
	}
	defer ev.Close()

	ch, err := net.InferStream(ev, 5000, 500, 42, 1000)
	if err != nil {
		t.Fatalf("InferStream: %v", err)
	}

	chunks := 0
	var lastChunk BnStreamChunk
	for chunk := range ch {
		chunks++
		lastChunk = chunk
		if chunk.ChunkJSON == "" {
			t.Error("chunk JSON should not be empty")
		}
	}

	if chunks == 0 {
		t.Error("expected at least one streaming chunk")
	}
	if !lastChunk.IsFinal {
		t.Error("last chunk should have IsFinal=true")
	}
}

// ── RAII Drop Safety ────────────────────────────────────────────────

func TestBnRAIIDropMultipleResources(t *testing.T) {
	path := fixturePath(t, "asia.bif")
	net, err := LoadBnNetwork(path)
	if err != nil {
		t.Fatal(err)
	}
	defer net.Close()

	ev, err := NewBnEvidence()
	if err != nil {
		t.Fatal(err)
	}
	defer ev.Close()

	if err := ev.SetDiscrete(net, "Smoking", "yes"); err != nil {
		t.Fatal(err)
	}

	result, err := net.Infer(ev, "ve")
	if err != nil {
		t.Fatal(err)
	}
	defer result.Close()

	// All Close() calls should not panic, even if called multiple times.
	result.Close()
	ev.Close()
	net.Close()
}
