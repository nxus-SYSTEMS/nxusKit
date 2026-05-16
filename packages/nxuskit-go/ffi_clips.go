//go:build nxuskit

// CLIPS Session API wrapper for the nxusKit C ABI.
//
// Provides an idiomatic Go ClipsSession type with Close() cleanup and
// runtime.SetFinalizer safety net.
//
// Thread safety: ClipsSession is NOT safe for concurrent access.
package nxuskit

/*
#cgo CFLAGS: -I${SRCDIR}/include
#cgo darwin,arm64 LDFLAGS: -L${SRCDIR}/lib/darwin_arm64 -lnxuskit -lpthread -ldl -lm -framework CoreFoundation -framework Security -Wl,-rpath,${SRCDIR}/lib/darwin_arm64
#cgo linux,amd64 LDFLAGS: -L${SRCDIR}/lib/linux_amd64 -lnxuskit -lpthread -ldl -lm -Wl,-rpath,${SRCDIR}/lib/linux_amd64
#cgo windows,amd64 LDFLAGS: -L${SRCDIR}/lib/windows_amd64 -lnxuskit -lws2_32 -luserenv -lbcrypt -lntdll

#include "nxuskit.h"
#include <stdlib.h>
*/
import "C"

import (
	"encoding/json"
	"fmt"
	"runtime"
	"unsafe"
)

// lastClipsError reads the thread-local error from the C ABI.
func lastClipsError(fallback string) error {
	ptr := C.nxuskit_last_error()
	if ptr == nil {
		return fmt.Errorf("nxuskit clips: %s", fallback)
	}
	msg := C.GoString(ptr)
	if msg == "" {
		return fmt.Errorf("nxuskit clips: %s", fallback)
	}
	return fmt.Errorf("nxuskit clips: %s", msg)
}

// clipsReadAndFreeString converts a C string to Go, frees the C memory, and returns the result.
func clipsReadAndFreeString(ptr *C.char, fallback string) (string, error) {
	if ptr == nil {
		return "", lastClipsError(fallback)
	}
	s := C.GoString(ptr)
	C.nxuskit_free_string(ptr)
	return s, nil
}

// ── ClipsSession ──────────────────────────────────────────────────

// ClipsSession is an opaque handle to a CLIPS inference session.
// Call Close() when done, or let the finalizer clean up.
type ClipsSession struct {
	handle C.uint64_t
}

// NewClipsSession creates a new isolated CLIPS session.
func NewClipsSession() (*ClipsSession, error) {
	h := C.nxuskit_clips_session_create()
	if h == 0 {
		return nil, lastClipsError("failed to create CLIPS session")
	}
	s := &ClipsSession{handle: h}
	runtime.SetFinalizer(s, func(cs *ClipsSession) { cs.Close() })
	return s, nil
}

// Close destroys the session and frees its resources.
// Safe to call multiple times.
func (s *ClipsSession) Close() {
	if s.handle != 0 {
		C.nxuskit_clips_session_destroy(s.handle)
		s.handle = 0
	}
}

// Reset retracts all facts and restores initial state, preserving rules.
func (s *ClipsSession) Reset() error {
	rc := C.nxuskit_clips_session_reset(s.handle)
	if rc != 0 {
		return lastClipsError("reset failed")
	}
	return nil
}

// Clear removes all constructs (rules, templates, facts, modules).
func (s *ClipsSession) Clear() error {
	rc := C.nxuskit_clips_session_clear(s.handle)
	if rc != 0 {
		return lastClipsError("clear failed")
	}
	return nil
}

// ClipsSessionInfo contains session metadata.
type ClipsSessionInfo struct {
	Name      string `json:"name"`
	CreatedAt string `json:"created_at"`
	FactCount int    `json:"fact_count"`
	RuleCount int    `json:"rule_count"`
}

// Info returns session metadata.
func (s *ClipsSession) Info() (*ClipsSessionInfo, error) {
	ptr := C.nxuskit_clips_session_info(s.handle)
	str, err := clipsReadAndFreeString(ptr, "info failed")
	if err != nil {
		return nil, err
	}
	var info ClipsSessionInfo
	if err := json.Unmarshal([]byte(str), &info); err != nil {
		return nil, fmt.Errorf("nxuskit clips: failed to parse session info: %w", err)
	}
	return &info, nil
}

// ── Construct Loading ─────────────────────────────────────────────

// LoadFile loads CLIPS constructs from a .clp file.
func (s *ClipsSession) LoadFile(path string) error {
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	rc := C.nxuskit_clips_session_load_file(s.handle, cPath)
	if rc != 0 {
		return lastClipsError("load_file failed")
	}
	return nil
}

// LoadString loads CLIPS constructs from a string.
func (s *ClipsSession) LoadString(constructs string) error {
	cStr := C.CString(constructs)
	defer C.free(unsafe.Pointer(cStr))
	rc := C.nxuskit_clips_session_load_string(s.handle, cStr)
	if rc != 0 {
		return lastClipsError("load_string failed")
	}
	return nil
}

// LoadBinary loads a CLIPS binary image.
func (s *ClipsSession) LoadBinary(path string) error {
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	rc := C.nxuskit_clips_session_load_binary(s.handle, cPath)
	if rc != 0 {
		return lastClipsError("load_binary failed")
	}
	return nil
}

// SaveBinary saves the current session as a CLIPS binary image.
func (s *ClipsSession) SaveBinary(path string) error {
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	rc := C.nxuskit_clips_session_save_binary(s.handle, cPath)
	if rc != 0 {
		return lastClipsError("save_binary failed")
	}
	return nil
}

// Build loads a single CLIPS construct string (e.g., a defrule or deftemplate).
func (s *ClipsSession) Build(construct string) error {
	cStr := C.CString(construct)
	defer C.free(unsafe.Pointer(cStr))
	rc := C.nxuskit_clips_session_build(s.handle, cStr)
	if rc != 0 {
		return lastClipsError("build failed")
	}
	return nil
}

// LoadJSON loads modules, templates, rules, and/or facts from a JSON definition.
func (s *ClipsSession) LoadJSON(jsonStr string) error {
	cJSON := C.CString(jsonStr)
	defer C.free(unsafe.Pointer(cJSON))
	rc := C.nxuskit_clips_session_load_json(s.handle, cJSON)
	if rc != 0 {
		return lastClipsError("load_json failed")
	}
	return nil
}

// Batch executes a CLIPS batch file.
func (s *ClipsSession) Batch(path string) error {
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	rc := C.nxuskit_clips_session_batch(s.handle, cPath)
	if rc != 0 {
		return lastClipsError("batch failed")
	}
	return nil
}

// ── Fact Operations ───────────────────────────────────────────────

// FactAssertString asserts a fact from a CLIPS string representation.
// Returns the fact index.
func (s *ClipsSession) FactAssertString(factString string) (int64, error) {
	cStr := C.CString(factString)
	defer C.free(unsafe.Pointer(cStr))
	idx := int64(C.nxuskit_clips_fact_assert_string(s.handle, cStr))
	if idx < 0 {
		return -1, lastClipsError("fact_assert_string failed")
	}
	return idx, nil
}

// FactAssertStructured asserts a structured fact with template name and slot values as JSON.
// Returns the fact index.
func (s *ClipsSession) FactAssertStructured(templateName string, slotsJSON string) (int64, error) {
	cTmpl := C.CString(templateName)
	defer C.free(unsafe.Pointer(cTmpl))
	cSlots := C.CString(slotsJSON)
	defer C.free(unsafe.Pointer(cSlots))
	idx := int64(C.nxuskit_clips_fact_assert_structured(s.handle, cTmpl, cSlots))
	if idx < 0 {
		return -1, lastClipsError("fact_assert_structured failed")
	}
	return idx, nil
}

// FactRetract retracts a fact by its index.
func (s *ClipsSession) FactRetract(factIndex int64) error {
	rc := C.nxuskit_clips_fact_retract(s.handle, C.int64_t(factIndex))
	if rc != 0 {
		return lastClipsError("fact_retract failed")
	}
	return nil
}

// FactRetractByTemplate retracts all facts of a given template.
func (s *ClipsSession) FactRetractByTemplate(templateName string) error {
	cName := C.CString(templateName)
	defer C.free(unsafe.Pointer(cName))
	rc := C.nxuskit_clips_fact_retract_by_template(s.handle, cName)
	if rc != 0 {
		return lastClipsError("fact_retract_by_template failed")
	}
	return nil
}

// FactExists checks if a fact with the given index exists.
func (s *ClipsSession) FactExists(factIndex int64) bool {
	return bool(C.nxuskit_clips_fact_exists(s.handle, C.int64_t(factIndex)))
}

// FactGetSlot returns a single slot value as a typed JSON string.
func (s *ClipsSession) FactGetSlot(factIndex int64, slotName string) (string, error) {
	cSlot := C.CString(slotName)
	defer C.free(unsafe.Pointer(cSlot))
	ptr := C.nxuskit_clips_fact_get_slot(s.handle, C.int64_t(factIndex), cSlot)
	return clipsReadAndFreeString(ptr, "fact_get_slot failed")
}

// FactSlotValues returns all slot values for a fact as a JSON object string.
func (s *ClipsSession) FactSlotValues(factIndex int64) (string, error) {
	ptr := C.nxuskit_clips_fact_slot_values(s.handle, C.int64_t(factIndex))
	return clipsReadAndFreeString(ptr, "fact_slot_values failed")
}

// FactPPForm returns the pretty-print form of a fact.
func (s *ClipsSession) FactPPForm(factIndex int64) (string, error) {
	ptr := C.nxuskit_clips_fact_pp_form(s.handle, C.int64_t(factIndex))
	return clipsReadAndFreeString(ptr, "fact_pp_form failed")
}

// FactIndex returns the index of a fact (useful after iteration).
func (s *ClipsSession) FactIndex(factIndex int64) (int64, error) {
	idx := int64(C.nxuskit_clips_fact_index(s.handle, C.int64_t(factIndex)))
	if idx < 0 {
		return -1, lastClipsError("fact_index failed")
	}
	return idx, nil
}

// FactsList returns all fact indices as a slice.
func (s *ClipsSession) FactsList() ([]int64, error) {
	ptr := C.nxuskit_clips_facts_list(s.handle)
	str, err := clipsReadAndFreeString(ptr, "facts_list failed")
	if err != nil {
		return nil, err
	}
	var facts []int64
	if err := json.Unmarshal([]byte(str), &facts); err != nil {
		return nil, fmt.Errorf("nxuskit clips: failed to parse facts list: %w", err)
	}
	return facts, nil
}

// FactsByTemplate returns fact indices for a specific template.
func (s *ClipsSession) FactsByTemplate(templateName string) ([]int64, error) {
	cName := C.CString(templateName)
	defer C.free(unsafe.Pointer(cName))
	ptr := C.nxuskit_clips_facts_by_template(s.handle, cName)
	str, err := clipsReadAndFreeString(ptr, "facts_by_template failed")
	if err != nil {
		return nil, err
	}
	var facts []int64
	if err := json.Unmarshal([]byte(str), &facts); err != nil {
		return nil, fmt.Errorf("nxuskit clips: failed to parse facts by template: %w", err)
	}
	return facts, nil
}

// ── Template Operations ───────────────────────────────────────────

// TemplateExists checks if a template with the given name exists.
func (s *ClipsSession) TemplateExists(name string) bool {
	cName := C.CString(name)
	defer C.free(unsafe.Pointer(cName))
	return bool(C.nxuskit_clips_template_exists(s.handle, cName))
}

// TemplateList returns all template names.
func (s *ClipsSession) TemplateList() ([]string, error) {
	ptr := C.nxuskit_clips_template_list(s.handle)
	str, err := clipsReadAndFreeString(ptr, "template_list failed")
	if err != nil {
		return nil, err
	}
	var names []string
	if err := json.Unmarshal([]byte(str), &names); err != nil {
		return nil, fmt.Errorf("nxuskit clips: failed to parse template list: %w", err)
	}
	return names, nil
}

// TemplateSlotNames returns slot names for a template.
func (s *ClipsSession) TemplateSlotNames(templateName string) ([]string, error) {
	cName := C.CString(templateName)
	defer C.free(unsafe.Pointer(cName))
	ptr := C.nxuskit_clips_template_slot_names(s.handle, cName)
	str, err := clipsReadAndFreeString(ptr, "template_slot_names failed")
	if err != nil {
		return nil, err
	}
	var names []string
	if err := json.Unmarshal([]byte(str), &names); err != nil {
		return nil, fmt.Errorf("nxuskit clips: failed to parse slot names: %w", err)
	}
	return names, nil
}

// TemplateSlotInfo returns detailed slot information for a template.
func (s *ClipsSession) TemplateSlotInfo(templateName string) ([]ClipsSlotInfo, error) {
	cName := C.CString(templateName)
	defer C.free(unsafe.Pointer(cName))
	ptr := C.nxuskit_clips_template_slot_info(s.handle, cName)
	str, err := clipsReadAndFreeString(ptr, "template_slot_info failed")
	if err != nil {
		return nil, err
	}
	var slots []ClipsSlotInfo
	if err := json.Unmarshal([]byte(str), &slots); err != nil {
		return nil, fmt.Errorf("nxuskit clips: failed to parse slot info: %w", err)
	}
	return slots, nil
}

// TemplateFacts returns fact indices for a template.
func (s *ClipsSession) TemplateFacts(templateName string) ([]int64, error) {
	cName := C.CString(templateName)
	defer C.free(unsafe.Pointer(cName))
	ptr := C.nxuskit_clips_template_facts(s.handle, cName)
	str, err := clipsReadAndFreeString(ptr, "template_facts failed")
	if err != nil {
		return nil, err
	}
	var facts []int64
	if err := json.Unmarshal([]byte(str), &facts); err != nil {
		return nil, fmt.Errorf("nxuskit clips: failed to parse template facts: %w", err)
	}
	return facts, nil
}

// TemplatePPForm returns the pretty-print form of a template.
func (s *ClipsSession) TemplatePPForm(templateName string) (string, error) {
	cName := C.CString(templateName)
	defer C.free(unsafe.Pointer(cName))
	ptr := C.nxuskit_clips_template_pp_form(s.handle, cName)
	return clipsReadAndFreeString(ptr, "template_pp_form failed")
}

// ── Rule Operations ───────────────────────────────────────────────

// RuleExists checks if a rule with the given name exists.
func (s *ClipsSession) RuleExists(name string) bool {
	cName := C.CString(name)
	defer C.free(unsafe.Pointer(cName))
	return bool(C.nxuskit_clips_rule_exists(s.handle, cName))
}

// RuleList returns all rule names.
func (s *ClipsSession) RuleList() ([]string, error) {
	ptr := C.nxuskit_clips_rule_list(s.handle)
	str, err := clipsReadAndFreeString(ptr, "rule_list failed")
	if err != nil {
		return nil, err
	}
	var names []string
	if err := json.Unmarshal([]byte(str), &names); err != nil {
		return nil, fmt.Errorf("nxuskit clips: failed to parse rule list: %w", err)
	}
	return names, nil
}

// RuleTimesFired returns the number of times a rule has fired.
func (s *ClipsSession) RuleTimesFired(ruleName string) (int64, error) {
	cName := C.CString(ruleName)
	defer C.free(unsafe.Pointer(cName))
	n := int64(C.nxuskit_clips_rule_times_fired(s.handle, cName))
	if n < 0 {
		return 0, lastClipsError("rule_times_fired failed")
	}
	return n, nil
}

// RuleBreakpointSet sets a breakpoint on a rule.
func (s *ClipsSession) RuleBreakpointSet(ruleName string) error {
	cName := C.CString(ruleName)
	defer C.free(unsafe.Pointer(cName))
	rc := C.nxuskit_clips_rule_breakpoint_set(s.handle, cName)
	if rc != 0 {
		return lastClipsError("rule_breakpoint_set failed")
	}
	return nil
}

// RuleBreakpointRemove removes a breakpoint from a rule.
func (s *ClipsSession) RuleBreakpointRemove(ruleName string) error {
	cName := C.CString(ruleName)
	defer C.free(unsafe.Pointer(cName))
	rc := C.nxuskit_clips_rule_breakpoint_remove(s.handle, cName)
	if rc != 0 {
		return lastClipsError("rule_breakpoint_remove failed")
	}
	return nil
}

// RuleHasBreakpoint checks if a rule has a breakpoint set.
func (s *ClipsSession) RuleHasBreakpoint(ruleName string) bool {
	cName := C.CString(ruleName)
	defer C.free(unsafe.Pointer(cName))
	return bool(C.nxuskit_clips_rule_has_breakpoint(s.handle, cName))
}

// RuleRefresh refreshes a rule's activations.
func (s *ClipsSession) RuleRefresh(ruleName string) error {
	cName := C.CString(ruleName)
	defer C.free(unsafe.Pointer(cName))
	rc := C.nxuskit_clips_rule_refresh(s.handle, cName)
	if rc != 0 {
		return lastClipsError("rule_refresh failed")
	}
	return nil
}

// RulePPForm returns the pretty-print form of a rule.
func (s *ClipsSession) RulePPForm(ruleName string) (string, error) {
	cName := C.CString(ruleName)
	defer C.free(unsafe.Pointer(cName))
	ptr := C.nxuskit_clips_rule_pp_form(s.handle, cName)
	return clipsReadAndFreeString(ptr, "rule_pp_form failed")
}

// RuleDelete removes a rule.
func (s *ClipsSession) RuleDelete(ruleName string) error {
	cName := C.CString(ruleName)
	defer C.free(unsafe.Pointer(cName))
	rc := C.nxuskit_clips_rule_delete(s.handle, cName)
	if rc != 0 {
		return lastClipsError("rule_delete failed")
	}
	return nil
}

// ── Execution & Agenda ────────────────────────────────────────────

// Run executes the CLIPS inference engine. Pass -1 for unlimited rule firings.
// Returns the number of rules fired.
func (s *ClipsSession) Run(limit int64) (int64, error) {
	fired := int64(C.nxuskit_clips_session_run(s.handle, C.int64_t(limit)))
	if fired < 0 {
		return 0, lastClipsError("run failed")
	}
	return fired, nil
}

// Halt signals the inference engine to stop.
func (s *ClipsSession) Halt() error {
	rc := C.nxuskit_clips_session_halt(s.handle)
	if rc != 0 {
		return lastClipsError("halt failed")
	}
	return nil
}

// AgendaSize returns the number of activations on the agenda.
func (s *ClipsSession) AgendaSize() (int64, error) {
	n := int64(C.nxuskit_clips_agenda_size(s.handle))
	if n < 0 {
		return 0, lastClipsError("agenda_size failed")
	}
	return n, nil
}

// AgendaClear clears all activations from the agenda.
func (s *ClipsSession) AgendaClear() error {
	rc := C.nxuskit_clips_agenda_clear(s.handle)
	if rc != 0 {
		return lastClipsError("agenda_clear failed")
	}
	return nil
}

// AgendaReorder reorders agenda activations.
func (s *ClipsSession) AgendaReorder() error {
	rc := C.nxuskit_clips_agenda_reorder(s.handle)
	if rc != 0 {
		return lastClipsError("agenda_reorder failed")
	}
	return nil
}

// StrategyGet returns the current conflict resolution strategy.
func (s *ClipsSession) StrategyGet() (string, error) {
	ptr := C.nxuskit_clips_strategy_get(s.handle)
	return clipsReadAndFreeString(ptr, "strategy_get failed")
}

// StrategySet sets the conflict resolution strategy.
func (s *ClipsSession) StrategySet(strategy string) error {
	cStr := C.CString(strategy)
	defer C.free(unsafe.Pointer(cStr))
	rc := C.nxuskit_clips_strategy_set(s.handle, cStr)
	if rc != 0 {
		return lastClipsError("strategy_set failed")
	}
	return nil
}

// SalienceModeGet returns the current salience evaluation mode.
func (s *ClipsSession) SalienceModeGet() (string, error) {
	ptr := C.nxuskit_clips_salience_mode_get(s.handle)
	return clipsReadAndFreeString(ptr, "salience_mode_get failed")
}

// SalienceModeSet sets the salience evaluation mode.
func (s *ClipsSession) SalienceModeSet(mode string) error {
	cStr := C.CString(mode)
	defer C.free(unsafe.Pointer(cStr))
	rc := C.nxuskit_clips_salience_mode_set(s.handle, cStr)
	if rc != 0 {
		return lastClipsError("salience_mode_set failed")
	}
	return nil
}

// ── Module & Focus Stack ──────────────────────────────────────────

// ModuleExists checks if a module exists.
func (s *ClipsSession) ModuleExists(name string) bool {
	cName := C.CString(name)
	defer C.free(unsafe.Pointer(cName))
	return bool(C.nxuskit_clips_module_exists(s.handle, cName))
}

// ModuleList returns all module names.
func (s *ClipsSession) ModuleList() ([]string, error) {
	ptr := C.nxuskit_clips_module_list(s.handle)
	str, err := clipsReadAndFreeString(ptr, "module_list failed")
	if err != nil {
		return nil, err
	}
	var names []string
	if err := json.Unmarshal([]byte(str), &names); err != nil {
		return nil, fmt.Errorf("nxuskit clips: failed to parse module list: %w", err)
	}
	return names, nil
}

// ModuleCurrentGet returns the current module name.
func (s *ClipsSession) ModuleCurrentGet() (string, error) {
	ptr := C.nxuskit_clips_module_current_get(s.handle)
	return clipsReadAndFreeString(ptr, "module_current_get failed")
}

// ModuleCurrentSet sets the current module.
func (s *ClipsSession) ModuleCurrentSet(name string) error {
	cName := C.CString(name)
	defer C.free(unsafe.Pointer(cName))
	rc := C.nxuskit_clips_module_current_set(s.handle, cName)
	if rc != 0 {
		return lastClipsError("module_current_set failed")
	}
	return nil
}

// FocusPush pushes a module onto the focus stack.
func (s *ClipsSession) FocusPush(moduleName string) error {
	cName := C.CString(moduleName)
	defer C.free(unsafe.Pointer(cName))
	rc := C.nxuskit_clips_focus_push(s.handle, cName)
	if rc != 0 {
		return lastClipsError("focus_push failed")
	}
	return nil
}

// FocusGet returns the module at the top of the focus stack, or "" if empty.
func (s *ClipsSession) FocusGet() string {
	ptr := C.nxuskit_clips_focus_get(s.handle)
	if ptr == nil {
		return ""
	}
	s2 := C.GoString(ptr)
	C.nxuskit_free_string(ptr)
	return s2
}

// FocusPop pops the top module from the focus stack.
func (s *ClipsSession) FocusPop() error {
	rc := C.nxuskit_clips_focus_pop(s.handle)
	if rc != 0 {
		return lastClipsError("focus_pop failed")
	}
	return nil
}

// FocusClear clears the focus stack.
func (s *ClipsSession) FocusClear() error {
	rc := C.nxuskit_clips_focus_clear(s.handle)
	if rc != 0 {
		return lastClipsError("focus_clear failed")
	}
	return nil
}

// ── Global Variables ──────────────────────────────────────────────

// GlobalExists checks if a global variable exists.
func (s *ClipsSession) GlobalExists(name string) bool {
	cName := C.CString(name)
	defer C.free(unsafe.Pointer(cName))
	return bool(C.nxuskit_clips_global_exists(s.handle, cName))
}

// GlobalList returns all global variable names.
func (s *ClipsSession) GlobalList() ([]string, error) {
	ptr := C.nxuskit_clips_global_list(s.handle)
	str, err := clipsReadAndFreeString(ptr, "global_list failed")
	if err != nil {
		return nil, err
	}
	var names []string
	if err := json.Unmarshal([]byte(str), &names); err != nil {
		return nil, fmt.Errorf("nxuskit clips: failed to parse global list: %w", err)
	}
	return names, nil
}

// GlobalGetValue returns the value of a global variable as JSON.
func (s *ClipsSession) GlobalGetValue(name string) (string, error) {
	cName := C.CString(name)
	defer C.free(unsafe.Pointer(cName))
	ptr := C.nxuskit_clips_global_get_value(s.handle, cName)
	return clipsReadAndFreeString(ptr, "global_get_value failed")
}

// GlobalSetValue sets the value of a global variable from a JSON value.
func (s *ClipsSession) GlobalSetValue(name string, valueJSON string) error {
	cName := C.CString(name)
	defer C.free(unsafe.Pointer(cName))
	cVal := C.CString(valueJSON)
	defer C.free(unsafe.Pointer(cVal))
	rc := C.nxuskit_clips_global_set_value(s.handle, cName, cVal)
	if rc != 0 {
		return lastClipsError("global_set_value failed")
	}
	return nil
}

// ── Expression Evaluation ─────────────────────────────────────────

// Eval evaluates a CLIPS expression and returns the result as JSON.
func (s *ClipsSession) Eval(expression string) (string, error) {
	cExpr := C.CString(expression)
	defer C.free(unsafe.Pointer(cExpr))
	ptr := C.nxuskit_clips_eval(s.handle, cExpr)
	return clipsReadAndFreeString(ptr, "eval failed")
}

// FunctionCall calls a CLIPS function with JSON arguments and returns the result as JSON.
func (s *ClipsSession) FunctionCall(functionName, argsJSON string) (string, error) {
	cName := C.CString(functionName)
	defer C.free(unsafe.Pointer(cName))
	cArgs := C.CString(argsJSON)
	defer C.free(unsafe.Pointer(cArgs))
	ptr := C.nxuskit_clips_function_call(s.handle, cName, cArgs)
	return clipsReadAndFreeString(ptr, "function_call failed")
}

// ── Watch & Diagnostics ───────────────────────────────────────────

// Watch enables watching for the specified item (e.g., "facts", "rules", "activations").
func (s *ClipsSession) Watch(item string) error {
	cItem := C.CString(item)
	defer C.free(unsafe.Pointer(cItem))
	rc := C.nxuskit_clips_watch(s.handle, cItem)
	if rc != 0 {
		return lastClipsError("watch failed")
	}
	return nil
}

// Unwatch disables watching for the specified item.
func (s *ClipsSession) Unwatch(item string) error {
	cItem := C.CString(item)
	defer C.free(unsafe.Pointer(cItem))
	rc := C.nxuskit_clips_unwatch(s.handle, cItem)
	if rc != 0 {
		return lastClipsError("unwatch failed")
	}
	return nil
}

// DribbleOn starts recording all CLIPS output to a file.
func (s *ClipsSession) DribbleOn(path string) error {
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	rc := C.nxuskit_clips_dribble_on(s.handle, cPath)
	if rc != 0 {
		return lastClipsError("dribble_on failed")
	}
	return nil
}

// DribbleOff stops recording CLIPS output.
func (s *ClipsSession) DribbleOff() error {
	rc := C.nxuskit_clips_dribble_off(s.handle)
	if rc != 0 {
		return lastClipsError("dribble_off failed")
	}
	return nil
}

// ── Settings ──────────────────────────────────────────────────────

// FactDuplicationGet returns whether fact duplication is allowed.
func (s *ClipsSession) FactDuplicationGet() bool {
	return bool(C.nxuskit_clips_fact_duplication_get(s.handle))
}

// FactDuplicationSet sets whether fact duplication is allowed.
func (s *ClipsSession) FactDuplicationSet(allow bool) error {
	rc := C.nxuskit_clips_fact_duplication_set(s.handle, C.bool(allow))
	if rc != 0 {
		return lastClipsError("fact_duplication_set failed")
	}
	return nil
}

// ResetGlobalsGet returns whether globals are reset on session reset.
func (s *ClipsSession) ResetGlobalsGet() bool {
	return bool(C.nxuskit_clips_reset_globals_get(s.handle))
}

// ResetGlobalsSet sets whether globals are reset on session reset.
func (s *ClipsSession) ResetGlobalsSet(reset bool) error {
	rc := C.nxuskit_clips_reset_globals_set(s.handle, C.bool(reset))
	if rc != 0 {
		return lastClipsError("reset_globals_set failed")
	}
	return nil
}

// ── Session Cache ─────────────────────────────────────────────────

// ClipsSessionPreload preloads a named session with rules configuration JSON.
// The session can later be cloned with ClipsSessionGetCached.
func ClipsSessionPreload(name, rulesJSON string) error {
	cName := C.CString(name)
	defer C.free(unsafe.Pointer(cName))
	cJSON := C.CString(rulesJSON)
	defer C.free(unsafe.Pointer(cJSON))
	rc := C.nxuskit_clips_session_preload(cName, cJSON)
	if rc != 0 {
		return lastClipsError("session_preload failed")
	}
	return nil
}

// ClipsSessionGetCached retrieves an independent clone of a cached session.
// The returned session is fully independent — modifications do not affect the cache.
func ClipsSessionGetCached(name string) (*ClipsSession, error) {
	cName := C.CString(name)
	defer C.free(unsafe.Pointer(cName))
	h := C.nxuskit_clips_session_get_cached(cName)
	if h == 0 {
		return nil, lastClipsError("session_get_cached failed")
	}
	s := &ClipsSession{handle: h}
	runtime.SetFinalizer(s, func(cs *ClipsSession) { cs.Close() })
	return s, nil
}

// ClipsSessionCacheRemove removes a cached session by name.
func ClipsSessionCacheRemove(name string) error {
	cName := C.CString(name)
	defer C.free(unsafe.Pointer(cName))
	rc := C.nxuskit_clips_session_cache_remove(cName)
	if rc != 0 {
		return lastClipsError("session_cache_remove failed")
	}
	return nil
}
