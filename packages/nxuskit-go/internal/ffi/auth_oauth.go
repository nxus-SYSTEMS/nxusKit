//go:build nxuskit

package ffi

/*
#include "../../include/nxuskit.h"
#include <stdlib.h>
*/
import "C"
import (
	"fmt"
	"unsafe"
)

// OAuthStart calls the C ABI nxuskit_oauth_start function.
func OAuthStart(providerID string, timeoutSecs uint32) (string, error) {
	cProvider := C.CString(providerID)
	defer C.free(unsafe.Pointer(cProvider))

	result := C.nxuskit_oauth_start(cProvider, C.uint32_t(timeoutSecs))
	if result == nil {
		return "", fmt.Errorf("nxuskit_oauth_start returned NULL")
	}
	defer C.nxuskit_free_string(result)

	return C.GoString(result), nil
}

// OAuthStatus calls the C ABI nxuskit_oauth_status function.
func OAuthStatus(providerID string) (string, error) {
	cProvider := C.CString(providerID)
	defer C.free(unsafe.Pointer(cProvider))

	result := C.nxuskit_oauth_status(cProvider)
	if result == nil {
		return "", fmt.Errorf("nxuskit_oauth_status returned NULL")
	}
	defer C.nxuskit_free_string(result)

	return C.GoString(result), nil
}

// OAuthRevoke calls the C ABI nxuskit_oauth_revoke function.
func OAuthRevoke(providerID string) error {
	cProvider := C.CString(providerID)
	defer C.free(unsafe.Pointer(cProvider))

	result := C.nxuskit_oauth_revoke(cProvider)
	if result != 0 {
		return fmt.Errorf("nxuskit_oauth_revoke failed (code %d)", result)
	}
	return nil
}
