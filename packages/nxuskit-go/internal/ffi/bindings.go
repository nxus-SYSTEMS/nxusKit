//go:build nxuskit

// Package ffi provides low-level cgo bindings to the nxuskit C ABI.
//
// This package is internal — consumers should use the public nxuskit API.
// All nxuskit C functions are wrapped here with proper Go error handling,
// JSON marshaling, and memory management.
package ffi

/*
#cgo CFLAGS: -I${SRCDIR}/../../include

#cgo darwin,arm64 LDFLAGS: -L${SRCDIR}/../../lib/darwin_arm64 -lnxuskit -lpthread -ldl -lm -framework CoreFoundation -framework Security -Wl,-rpath,${SRCDIR}/../../lib/darwin_arm64
#cgo linux,amd64 LDFLAGS: -L${SRCDIR}/../../lib/linux_amd64 -lnxuskit -lpthread -ldl -lm -Wl,-rpath,${SRCDIR}/../../lib/linux_amd64
#cgo windows,amd64 LDFLAGS: -L${SRCDIR}/../../lib/windows_amd64 -lnxuskit -lws2_32 -luserenv -lbcrypt -lntdll

#include "nxuskit.h"
#include <stdlib.h>
*/
import "C"

import (
	"encoding/json"
	"fmt"
	"runtime"
	"sync"
	"unsafe"
)

// initOnce ensures we check library availability exactly once.
var initOnce sync.Once
var initErr error

// Init verifies that the nxuskit library is linked and functional.
// It is safe to call multiple times — only the first call performs work.
func Init() error {
	initOnce.Do(func() {
		ver := C.nxuskit_version()
		if ver == nil {
			initErr = fmt.Errorf("nxuskit: library not loaded (nxuskit_version returned NULL)")
			return
		}
		_ = C.GoString(ver) // verify readable
	})
	return initErr
}

// Version returns the nxuskit library version string.
func Version() string {
	ptr := C.nxuskit_version()
	if ptr == nil {
		return ""
	}
	return C.GoString(ptr)
}

// LastError returns the last error message from the nxuskit library
// on the calling thread. Returns "" if no error is set.
func LastError() string {
	ptr := C.nxuskit_last_error()
	if ptr == nil {
		return ""
	}
	return C.GoString(ptr)
}

// ProviderHandle is an opaque reference to a nxuskit provider.
// It must be freed with FreeProvider when no longer needed.
type ProviderHandle struct {
	ptr *C.struct_NxuskitProvider
}

// CreateProvider creates a provider from a JSON configuration string.
// The config must include at minimum {"provider_type": "..."}.
func CreateProvider(configJSON string) (*ProviderHandle, error) {
	cConfig := C.CString(configJSON)
	defer C.free(unsafe.Pointer(cConfig))

	ptr := C.nxuskit_create_provider(cConfig)
	if ptr == nil {
		return nil, fmt.Errorf("nxuskit: create_provider failed: %s", LastError())
	}

	h := &ProviderHandle{ptr: ptr}
	runtime.SetFinalizer(h, func(h *ProviderHandle) {
		h.Free()
	})
	return h, nil
}

// Free releases the provider. Safe to call multiple times.
func (h *ProviderHandle) Free() {
	if h.ptr != nil {
		C.nxuskit_free_provider(h.ptr)
		h.ptr = nil
	}
}

// Chat performs a synchronous chat request and returns the response as
// a parsed JSON map. The request is serialized to JSON before calling
// the C API.
func (h *ProviderHandle) Chat(requestJSON string) (map[string]any, error) {
	if h.ptr == nil {
		return nil, fmt.Errorf("nxuskit: provider handle is nil")
	}

	cReq := C.CString(requestJSON)
	defer C.free(unsafe.Pointer(cReq))

	resp := C.nxuskit_chat(h.ptr, cReq)
	if resp == nil {
		return nil, fmt.Errorf("nxuskit: chat failed: %s", LastError())
	}
	defer C.nxuskit_free_response(resp)

	jsonPtr := C.nxuskit_response_json(resp)
	if jsonPtr == nil {
		return nil, fmt.Errorf("nxuskit: response_json returned NULL")
	}

	jsonStr := C.GoString(jsonPtr)
	var result map[string]any
	if err := json.Unmarshal([]byte(jsonStr), &result); err != nil {
		return nil, fmt.Errorf("nxuskit: failed to parse response JSON: %w", err)
	}

	// Check for error in response
	if errObj, ok := result["error"]; ok {
		if errMap, ok := errObj.(map[string]any); ok {
			return nil, parseNxuskitError(errMap)
		}
	}

	return result, nil
}

// NxuskitError is a structured error returned by the nxuskit C ABI.
// The ffi_provider layer maps this to the public LLMError types.
type NxuskitError struct {
	// ErrorType is the error category string from the C ABI (e.g. "license_required").
	ErrorType string
	// Message is the human-readable error message.
	Message string
	// Feature is the feature name (for entitlement errors).
	Feature string
	// RequiredEdition is the required edition (for "edition_insufficient" errors).
	RequiredEdition string
}

// Error implements the error interface.
func (e *NxuskitError) Error() string {
	if e.Message == "" {
		return fmt.Sprintf("nxuskit: %s", e.ErrorType)
	}
	return fmt.Sprintf("nxuskit: %s: %s", e.ErrorType, e.Message)
}

// parseNxuskitError converts an error map from the nxuskit C ABI response
// into a structured [NxuskitError]. The caller (ffi_provider.go) maps this
// to the appropriate public [LLMError] types.
func parseNxuskitError(errMap map[string]any) error {
	msg, _ := errMap["message"].(string)
	errType, _ := errMap["error_type"].(string)
	feature, _ := errMap["feature"].(string)
	edition, _ := errMap["required_edition"].(string)

	return &NxuskitError{
		ErrorType:       errType,
		Message:         msg,
		Feature:         feature,
		RequiredEdition: edition,
	}
}

// ListModels returns available models as a parsed JSON array.
func (h *ProviderHandle) ListModels() ([]map[string]any, error) {
	if h.ptr == nil {
		return nil, fmt.Errorf("nxuskit: provider handle is nil")
	}

	cResult := C.nxuskit_list_models(h.ptr)
	if cResult == nil {
		return nil, fmt.Errorf("nxuskit: list_models failed: %s", LastError())
	}
	defer C.nxuskit_free_string(cResult)

	jsonStr := C.GoString(cResult)
	var models []map[string]any
	if err := json.Unmarshal([]byte(jsonStr), &models); err != nil {
		return nil, fmt.Errorf("nxuskit: failed to parse models JSON: %w", err)
	}

	return models, nil
}
