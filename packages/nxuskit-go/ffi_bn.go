//go:build nxuskit

// Bayesian Network wrapper for the nxusKit C ABI.
//
// Provides idiomatic Go types (BnNetwork, BnEvidence, BnResult) with RAII-like
// Close() methods and runtime.SetFinalizer safety nets.
//
// Thread safety: Handle types are NOT safe for concurrent mutation.
// Concurrent reads on the same handle are safe.
package nxuskit

/*
#cgo CFLAGS: -I${SRCDIR}/include
#cgo darwin,arm64 LDFLAGS: -L${SRCDIR}/lib/darwin_arm64 -lnxuskit -lpthread -ldl -lm -framework CoreFoundation -framework Security -Wl,-rpath,${SRCDIR}/lib/darwin_arm64
#cgo linux,amd64 LDFLAGS: -L${SRCDIR}/lib/linux_amd64 -lnxuskit -lpthread -ldl -lm -Wl,-rpath,${SRCDIR}/lib/linux_amd64
#cgo windows,amd64 LDFLAGS: -L${SRCDIR}/lib/windows_amd64 -lnxuskit -lws2_32 -luserenv -lbcrypt -lntdll

#include "nxuskit.h"
#include <stdlib.h>
#include <math.h>

// Streaming callback trampoline — see ffi_bn_callbacks.go for export.
extern _Bool goBnStreamCallback(const char *chunk_json, uint32_t iteration, uint32_t total, _Bool is_final, void *user_data);
*/
import "C"

import (
	"encoding/json"
	"fmt"
	"math"
	"runtime"
	"runtime/cgo"
	"unsafe"
)

// lastBnError reads the thread-local error from the C ABI.
func lastBnError(fallback string) error {
	ptr := C.nxuskit_last_error()
	if ptr == nil {
		return fmt.Errorf("nxuskit bn: %s", fallback)
	}
	msg := C.GoString(ptr)
	if msg == "" {
		return fmt.Errorf("nxuskit bn: %s", fallback)
	}
	return fmt.Errorf("nxuskit bn: %s", msg)
}

// readAndFreeString converts a C string to Go, frees the C memory, and returns the result.
func readAndFreeString(ptr *C.char) (string, error) {
	if ptr == nil {
		return "", lastBnError("NULL string returned")
	}
	s := C.GoString(ptr)
	C.nxuskit_free_string(ptr)
	return s, nil
}

// ── BnNetwork ────────────────────────────────────────────────────

// BnNetwork is an opaque handle to a Bayesian Network.
// Call Close() when done, or let the finalizer clean up.
type BnNetwork struct {
	ptr *C.struct_NxuskitBnNet
}

// NewBnNetwork creates an empty Bayesian Network.
func NewBnNetwork() (*BnNetwork, error) {
	ptr := C.nxuskit_bn_net_create()
	if ptr == nil {
		return nil, lastBnError("failed to create BN")
	}
	net := &BnNetwork{ptr: ptr}
	runtime.SetFinalizer(net, func(n *BnNetwork) { n.Close() })
	return net, nil
}

// LoadBnNetwork loads a BIF file into a new network.
func LoadBnNetwork(path string) (*BnNetwork, error) {
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))

	ptr := C.nxuskit_bn_net_load_file(cPath)
	if ptr == nil {
		return nil, lastBnError("failed to load BIF file")
	}
	net := &BnNetwork{ptr: ptr}
	runtime.SetFinalizer(net, func(n *BnNetwork) { n.Close() })
	return net, nil
}

// Close destroys the underlying C handle. Safe to call multiple times.
func (n *BnNetwork) Close() {
	if n.ptr != nil {
		C.nxuskit_bn_net_destroy(n.ptr)
		n.ptr = nil
	}
}

// NumVariables returns the number of variables in the network.
func (n *BnNetwork) NumVariables() int {
	return int(C.nxuskit_bn_net_num_variables(n.ptr))
}

// Variables returns all variable names.
func (n *BnNetwork) Variables() ([]string, error) {
	ptr := C.nxuskit_bn_net_variables(n.ptr)
	jsonStr, err := readAndFreeString(ptr)
	if err != nil {
		return nil, err
	}
	var vars []string
	if err := json.Unmarshal([]byte(jsonStr), &vars); err != nil {
		return nil, fmt.Errorf("nxuskit bn: failed to parse variables JSON: %w", err)
	}
	return vars, nil
}

// VariableStates returns the states for a specific variable.
func (n *BnNetwork) VariableStates(variable string) ([]string, error) {
	cVar := C.CString(variable)
	defer C.free(unsafe.Pointer(cVar))

	ptr := C.nxuskit_bn_net_variable_states(n.ptr, cVar)
	jsonStr, err := readAndFreeString(ptr)
	if err != nil {
		return nil, err
	}
	var states []string
	if err := json.Unmarshal([]byte(jsonStr), &states); err != nil {
		return nil, fmt.Errorf("nxuskit bn: failed to parse states JSON: %w", err)
	}
	return states, nil
}

// SaveFile saves the network to a BIF file.
func (n *BnNetwork) SaveFile(path string) error {
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))

	ok := C.nxuskit_bn_net_save_file(n.ptr, cPath)
	if !ok {
		return lastBnError("failed to save BIF file")
	}
	return nil
}

// AddGaussianVariable adds a continuous Gaussian variable to the network.
func (n *BnNetwork) AddGaussianVariable(name string, meanBase, variance float64) error {
	cName := C.CString(name)
	defer C.free(unsafe.Pointer(cName))

	ok := C.nxuskit_bn_net_add_gaussian_variable(n.ptr, cName, C.double(meanBase), C.double(variance))
	if !ok {
		return lastBnError("failed to add Gaussian variable")
	}
	return nil
}

// SetGaussianWeight sets the weight from parent to a Gaussian child variable.
func (n *BnNetwork) SetGaussianWeight(variable, parent string, weight float64) error {
	cVar := C.CString(variable)
	defer C.free(unsafe.Pointer(cVar))
	cParent := C.CString(parent)
	defer C.free(unsafe.Pointer(cParent))

	ok := C.nxuskit_bn_net_set_gaussian_weight(n.ptr, cVar, cParent, C.double(weight))
	if !ok {
		return lastBnError("failed to set Gaussian weight")
	}
	return nil
}

// Infer runs inference with the given algorithm ("ve", "jt", "gibbs", "lbp").
func (n *BnNetwork) Infer(evidence *BnEvidence, algorithm string) (*BnResult, error) {
	return n.InferWithOptions(evidence, algorithm, 0, 0, 0)
}

// InferWithOptions runs inference with full Gibbs sampling options.
func (n *BnNetwork) InferWithOptions(evidence *BnEvidence, algorithm string, numSamples, burnIn uint32, seed uint64) (*BnResult, error) {
	cAlgo := C.CString(algorithm)
	defer C.free(unsafe.Pointer(cAlgo))

	ptr := C.nxuskit_bn_infer(n.ptr, evidence.ptr, cAlgo, C.uint32_t(numSamples), C.uint32_t(burnIn), C.uint64_t(seed))
	if ptr == nil {
		return nil, lastBnError("inference failed")
	}
	result := &BnResult{ptr: ptr}
	runtime.SetFinalizer(result, func(r *BnResult) { r.Close() })
	return result, nil
}

// InferWithConfig runs inference with algorithm-specific JSON configuration.
func (n *BnNetwork) InferWithConfig(evidence *BnEvidence, algorithm, configJSON string) (*BnResult, error) {
	cAlgo := C.CString(algorithm)
	defer C.free(unsafe.Pointer(cAlgo))
	cConfig := C.CString(configJSON)
	defer C.free(unsafe.Pointer(cConfig))

	ptr := C.nxuskit_bn_infer_with_config(n.ptr, evidence.ptr, cAlgo, cConfig)
	if ptr == nil {
		return nil, lastBnError("inference with config failed")
	}
	result := &BnResult{ptr: ptr}
	runtime.SetFinalizer(result, func(r *BnResult) { r.Close() })
	return result, nil
}

// BnStreamChunk is a progressive inference result delivered via streaming.
type BnStreamChunk struct {
	ChunkJSON string // Raw JSON of the partial result
	Iteration uint32 // Current iteration/sample count
	Total     uint32 // Target iterations
	IsFinal   bool   // True for the last chunk
}

// InferStream runs streaming Gibbs inference, delivering chunks via a channel.
// The returned channel is closed when streaming completes or is cancelled.
// Set chunkSize to 0 for the default (1000 samples between callbacks).
func (n *BnNetwork) InferStream(evidence *BnEvidence, numSamples, burnIn uint32, seed uint64, chunkSize uint32) (<-chan BnStreamChunk, error) {
	ch := make(chan BnStreamChunk, 16)

	session := &bnStreamSession{chunks: ch}
	handle := cgo.NewHandle(session)

	go func() {
		defer close(ch)
		defer handle.Delete()

		ok := C.nxuskit_bn_infer_stream(
			n.ptr, evidence.ptr,
			C.uint32_t(numSamples), C.uint32_t(burnIn), C.uint64_t(seed),
			C.uint32_t(chunkSize),
			C.NxuskitBnStreamCallback(C.goBnStreamCallback),
			unsafe.Pointer(&handle),
		)
		if !ok {
			// Error — channel closes naturally via defer
			_ = lastBnError("streaming inference failed")
		}
	}()

	return ch, nil
}

// bnStreamSession holds the channel for streaming callbacks.
type bnStreamSession struct {
	chunks chan<- BnStreamChunk
}

// ── BnEvidence ───────────────────────────────────────────────────

// BnEvidence holds observations for Bayesian Network inference.
type BnEvidence struct {
	ptr *C.struct_NxuskitBnEvidence
}

// NewBnEvidence creates an empty evidence set.
func NewBnEvidence() (*BnEvidence, error) {
	ptr := C.nxuskit_bn_ev_create()
	if ptr == nil {
		return nil, lastBnError("failed to create evidence")
	}
	ev := &BnEvidence{ptr: ptr}
	runtime.SetFinalizer(ev, func(e *BnEvidence) { e.Close() })
	return ev, nil
}

// Close destroys the underlying C handle. Safe to call multiple times.
func (e *BnEvidence) Close() {
	if e.ptr != nil {
		C.nxuskit_bn_ev_destroy(e.ptr)
		e.ptr = nil
	}
}

// SetDiscrete sets a discrete observation: variable = state.
func (e *BnEvidence) SetDiscrete(network *BnNetwork, variable, state string) error {
	cVar := C.CString(variable)
	defer C.free(unsafe.Pointer(cVar))
	cState := C.CString(state)
	defer C.free(unsafe.Pointer(cState))

	ok := C.nxuskit_bn_ev_set_discrete(e.ptr, network.ptr, cVar, cState)
	if !ok {
		return lastBnError("failed to set discrete evidence")
	}
	return nil
}

// SetContinuous sets a continuous observation for a Gaussian variable.
func (e *BnEvidence) SetContinuous(network *BnNetwork, variable string, value float64) error {
	cVar := C.CString(variable)
	defer C.free(unsafe.Pointer(cVar))

	ok := C.nxuskit_bn_ev_set_continuous(e.ptr, network.ptr, cVar, C.double(value))
	if !ok {
		return lastBnError("failed to set continuous evidence")
	}
	return nil
}

// Retract removes evidence for a variable.
func (e *BnEvidence) Retract(variable string) error {
	cVar := C.CString(variable)
	defer C.free(unsafe.Pointer(cVar))

	ok := C.nxuskit_bn_ev_retract(e.ptr, cVar)
	if !ok {
		return lastBnError("failed to retract evidence")
	}
	return nil
}

// Clear removes all evidence.
func (e *BnEvidence) Clear() error {
	ok := C.nxuskit_bn_ev_clear(e.ptr)
	if !ok {
		return lastBnError("failed to clear evidence")
	}
	return nil
}

// ── BnResult ─────────────────────────────────────────────────────

// BnResult holds inference results with posterior distributions.
type BnResult struct {
	ptr *C.struct_NxuskitBnResult
}

// Close destroys the underlying C handle. Safe to call multiple times.
func (r *BnResult) Close() {
	if r.ptr != nil {
		C.nxuskit_bn_result_destroy(r.ptr)
		r.ptr = nil
	}
}

// JSON returns the full result as a JSON string.
func (r *BnResult) JSON() (string, error) {
	ptr := C.nxuskit_bn_result_json(r.ptr)
	return readAndFreeString(ptr)
}

// Marginal returns the posterior distribution for a discrete variable
// as a map of state -> probability.
func (r *BnResult) Marginal(variable string) (map[string]float64, error) {
	cVar := C.CString(variable)
	defer C.free(unsafe.Pointer(cVar))

	ptr := C.nxuskit_bn_result_query(r.ptr, cVar)
	jsonStr, err := readAndFreeString(ptr)
	if err != nil {
		return nil, err
	}
	var dist map[string]float64
	if err := json.Unmarshal([]byte(jsonStr), &dist); err != nil {
		return nil, fmt.Errorf("nxuskit bn: failed to parse distribution JSON: %w", err)
	}
	return dist, nil
}

// NumVariables returns the number of variables in the result.
func (r *BnResult) NumVariables() int {
	return int(C.nxuskit_bn_result_num_variables(r.ptr))
}

// Next returns the next variable name in the iteration, or "" when exhausted.
func (r *BnResult) Next() string {
	ptr := C.nxuskit_bn_result_next(r.ptr)
	if ptr == nil {
		return ""
	}
	s := C.GoString(ptr)
	C.nxuskit_free_string(ptr)
	return s
}

// Reset resets the iteration cursor.
func (r *BnResult) Reset() {
	C.nxuskit_bn_result_reset(r.ptr)
}

// VariableNames collects all variable names.
func (r *BnResult) VariableNames() []string {
	r.Reset()
	var names []string
	for {
		name := r.Next()
		if name == "" {
			break
		}
		names = append(names, name)
	}
	return names
}

// ContinuousMarginal holds the posterior summary for a continuous variable.
type ContinuousMarginal struct {
	Mean     float64 `json:"mean"`
	Variance float64 `json:"variance"`
	CILower  float64 `json:"ci_lower"`
	CIUpper  float64 `json:"ci_upper"`
}

// Mean returns the posterior mean for a continuous variable.
// Returns NaN if the variable is not found or is discrete-only.
func (r *BnResult) Mean(variable string) (float64, error) {
	cVar := C.CString(variable)
	defer C.free(unsafe.Pointer(cVar))

	val := float64(C.nxuskit_bn_result_mean(r.ptr, cVar))
	if math.IsNaN(val) {
		return val, lastBnError("mean not available for variable")
	}
	return val, nil
}

// Variance returns the posterior variance for a continuous variable.
// Returns NaN if the variable is not found or is discrete-only.
func (r *BnResult) Variance(variable string) (float64, error) {
	cVar := C.CString(variable)
	defer C.free(unsafe.Pointer(cVar))

	val := float64(C.nxuskit_bn_result_variance(r.ptr, cVar))
	if math.IsNaN(val) {
		return val, lastBnError("variance not available for variable")
	}
	return val, nil
}

// ContinuousMarginalResult returns the full continuous marginal for a variable.
func (r *BnResult) ContinuousMarginalResult(variable string) (*ContinuousMarginal, error) {
	cVar := C.CString(variable)
	defer C.free(unsafe.Pointer(cVar))

	ptr := C.nxuskit_bn_result_continuous_marginal(r.ptr, cVar)
	jsonStr, err := readAndFreeString(ptr)
	if err != nil {
		return nil, err
	}
	var m ContinuousMarginal
	if err := json.Unmarshal([]byte(jsonStr), &m); err != nil {
		return nil, fmt.Errorf("nxuskit bn: failed to parse continuous marginal JSON: %w", err)
	}
	return &m, nil
}

// ── Structure & Parameter Learning ────────────────────────────────

// BnSearchStructureConfig holds options for structure learning.
type BnSearchStructureConfig struct {
	// Algorithm: "hill_climb" or "k2".
	Algorithm string
	// Scoring: "bic" or "bdeu".
	Scoring string
	// MaxParents per node (0 = default: 5 for hill_climb, 3 for k2).
	MaxParents uint32
	// MaxSteps for hill_climb search (0 = default 1000). Ignored for k2.
	MaxSteps uint32
	// ESS (equivalent sample size) for BDeu scoring (0 = default 10.0). Ignored for BIC.
	ESS float64
	// Ordering is a variable name ordering for K2 (nil for hill_climb).
	Ordering []string
}

// BnStructureResult holds the result of structure learning.
type BnStructureResult struct {
	Edges      []BnEdge `json:"edges"`
	Score      float64  `json:"score"`
	Iterations int      `json:"iterations"`
}

// BnEdge represents a directed edge in the learned BN structure.
// The C ABI returns edges as [from, to] arrays; this type handles both
// array and object JSON formats via custom unmarshalling.
type BnEdge struct {
	From string `json:"from"`
	To   string `json:"to"`
}

// UnmarshalJSON handles both ["from","to"] arrays and {"from":"...","to":"..."} objects.
func (e *BnEdge) UnmarshalJSON(data []byte) error {
	// Try array format first: ["parent", "child"]
	var arr []string
	if err := json.Unmarshal(data, &arr); err == nil {
		if len(arr) >= 2 {
			e.From = arr[0]
			e.To = arr[1]
			return nil
		}
		return fmt.Errorf("BnEdge array must have 2 elements, got %d", len(arr))
	}
	// Fall back to object format: {"from": "...", "to": "..."}
	type alias BnEdge
	var obj alias
	if err := json.Unmarshal(data, &obj); err != nil {
		return fmt.Errorf("BnEdge: expected [from,to] array or {from,to} object: %w", err)
	}
	*e = BnEdge(obj)
	return nil
}

// SearchStructure runs structure learning on the network given CSV data.
// The network is used to provide variable metadata; the learned structure
// is returned as a BnStructureResult.
func (n *BnNetwork) SearchStructure(csvPath string, cfg BnSearchStructureConfig) (*BnStructureResult, error) {
	cCSV := C.CString(csvPath)
	defer C.free(unsafe.Pointer(cCSV))
	cAlgo := C.CString(cfg.Algorithm)
	defer C.free(unsafe.Pointer(cAlgo))
	cScoring := C.CString(cfg.Scoring)
	defer C.free(unsafe.Pointer(cScoring))

	var cOrdering *C.char
	if len(cfg.Ordering) > 0 {
		orderJSON, err := json.Marshal(cfg.Ordering)
		if err != nil {
			return nil, fmt.Errorf("nxuskit bn: failed to marshal ordering: %w", err)
		}
		cOrdering = C.CString(string(orderJSON))
		defer C.free(unsafe.Pointer(cOrdering))
	}

	ptr := C.nxuskit_bn_search_structure(
		n.ptr,
		cCSV,
		cAlgo,
		cScoring,
		C.uint32_t(cfg.MaxParents),
		C.uint32_t(cfg.MaxSteps),
		C.double(cfg.ESS),
		cOrdering,
	)
	jsonStr, err := readAndFreeString(ptr)
	if err != nil {
		return nil, err
	}
	var result BnStructureResult
	if err := json.Unmarshal([]byte(jsonStr), &result); err != nil {
		return nil, fmt.Errorf("nxuskit bn: failed to parse structure result JSON: %w", err)
	}
	return &result, nil
}

// LearnMLE learns CPT parameters from CSV data using Maximum Likelihood Estimation.
// Pseudocount controls Laplace smoothing (0.0 = none, 1.0 = standard).
// The network is modified in-place with learned parameters.
func (n *BnNetwork) LearnMLE(csvPath string, pseudocount float64) error {
	cCSV := C.CString(csvPath)
	defer C.free(unsafe.Pointer(cCSV))

	ok := C.nxuskit_bn_learn_mle(n.ptr, cCSV, C.double(pseudocount))
	if !ok {
		return lastBnError("MLE learning failed")
	}
	return nil
}

// LogLikelihood computes the log-likelihood of the CSV data given the
// current network CPTs. Returns NEG_INFINITY on error.
func (n *BnNetwork) LogLikelihood(csvPath string) (float64, error) {
	cCSV := C.CString(csvPath)
	defer C.free(unsafe.Pointer(cCSV))

	ll := float64(C.nxuskit_bn_log_likelihood(n.ptr, cCSV))
	if math.IsInf(ll, -1) {
		return ll, lastBnError("log-likelihood computation failed")
	}
	return ll, nil
}
