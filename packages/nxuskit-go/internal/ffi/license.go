//go:build nxuskit

package ffi

/*
#include "nxuskit.h"
#include <stdlib.h>
*/
import "C"

import (
	"fmt"
	"unsafe"
)

// LicenseResolve calls nxuskit_license_resolve and returns the JSON result.
func LicenseResolve(explicitKey string) (string, error) {
	var ptr *C.char
	if explicitKey == "" {
		ptr = C.nxuskit_license_resolve(nil)
	} else {
		cKey := C.CString(explicitKey)
		defer C.free(unsafe.Pointer(cKey))
		ptr = C.nxuskit_license_resolve(cKey)
	}

	if ptr == nil {
		return "", fmt.Errorf("nxuskit_license_resolve returned NULL: %s", LastError())
	}
	defer C.nxuskit_free_string(ptr)
	return C.GoString(ptr), nil
}

// LicenseValidate calls nxuskit_license_validate and returns the JSON result.
func LicenseValidate(tokenJWT string) (string, error) {
	cToken := C.CString(tokenJWT)
	defer C.free(unsafe.Pointer(cToken))

	ptr := C.nxuskit_license_validate(cToken)
	if ptr == nil {
		return "", fmt.Errorf("nxuskit_license_validate returned NULL: %s", LastError())
	}
	defer C.nxuskit_free_string(ptr)
	return C.GoString(ptr), nil
}

// LicenseMachineID calls nxuskit_license_machine_id and returns the fingerprint.
func LicenseMachineID() (string, error) {
	ptr := C.nxuskit_license_machine_id()
	if ptr == nil {
		return "", fmt.Errorf("machine ID unavailable: %s", LastError())
	}
	defer C.nxuskit_free_string(ptr)
	return C.GoString(ptr), nil
}

// LicenseActivate calls nxuskit_license_activate and returns the JSON result.
func LicenseActivate(purchaseID string) (string, error) {
	cPID := C.CString(purchaseID)
	defer C.free(unsafe.Pointer(cPID))

	ptr := C.nxuskit_license_activate(cPID)
	if ptr == nil {
		return "", fmt.Errorf("nxuskit_license_activate returned NULL: %s", LastError())
	}
	defer C.nxuskit_free_string(ptr)
	return C.GoString(ptr), nil
}

// LicenseDeactivate calls nxuskit_license_deactivate and returns the JSON result.
func LicenseDeactivate() (string, error) {
	ptr := C.nxuskit_license_deactivate()
	if ptr == nil {
		return "", fmt.Errorf("nxuskit_license_deactivate returned NULL: %s", LastError())
	}
	defer C.nxuskit_free_string(ptr)
	return C.GoString(ptr), nil
}

// LicenseTrialIssue calls nxuskit_license_trial_issue and returns the JSON result.
func LicenseTrialIssue() (string, error) {
	ptr := C.nxuskit_license_trial_issue()
	if ptr == nil {
		return "", fmt.Errorf("nxuskit_license_trial_issue returned NULL: %s", LastError())
	}
	defer C.nxuskit_free_string(ptr)
	return C.GoString(ptr), nil
}

// LicenseTrialActivate calls nxuskit_license_trial_activate and returns the JSON result.
func LicenseTrialActivate(code string) (string, error) {
	cCode := C.CString(code)
	defer C.free(unsafe.Pointer(cCode))

	ptr := C.nxuskit_license_trial_activate(cCode)
	if ptr == nil {
		return "", fmt.Errorf("nxuskit_license_trial_activate returned NULL: %s", LastError())
	}
	defer C.nxuskit_free_string(ptr)
	return C.GoString(ptr), nil
}
