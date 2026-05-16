//go:build nxuskit

package ffi

// This file is intentionally separate from callbacks.go.
// cgo requires that //export functions and C.functionName references
// for the same symbol do NOT appear in the same file. The C trampoline
// functions in callbacks.go forward to the //export functions here.

/*
#include <stdint.h>
*/
import "C"

import (
	"encoding/json"
	"fmt"
	"runtime/cgo"
	"unsafe"
)

//export goStreamChunkCallback
func goStreamChunkCallback(chunkJSON *C.char, userData unsafe.Pointer) C.int32_t {
	h := *(*cgo.Handle)(userData)
	session := h.Value().(*StreamSession)

	if chunkJSON == nil {
		return 0
	}

	goStr := C.GoString(chunkJSON)
	var chunk StreamChunkData
	if err := json.Unmarshal([]byte(goStr), &chunk); err != nil {
		// Skip malformed chunks
		return 0
	}

	// Non-blocking send — if the buffer is full, we drop the chunk
	// rather than blocking the C runtime.
	select {
	case session.Chunks <- chunk:
	default:
	}

	return 0
}

//export goStreamDoneCallback
func goStreamDoneCallback(finalJSON *C.char, userData unsafe.Pointer) {
	h := *(*cgo.Handle)(userData)
	session := h.Value().(*StreamSession)

	close(session.Chunks)

	if finalJSON == nil {
		session.Done <- StreamDoneData{}
		close(session.Done)
		return
	}

	goStr := C.GoString(finalJSON)
	var raw map[string]any
	_ = json.Unmarshal([]byte(goStr), &raw)

	done := StreamDoneData{Raw: raw}
	if content, ok := raw["content"].(string); ok {
		done.Content = content
	}
	if errObj, ok := raw["error"]; ok {
		if errMap, ok := errObj.(map[string]any); ok {
			done.Error = &StreamError{
				ErrorType: fmt.Sprintf("%v", errMap["error_type"]),
				Message:   fmt.Sprintf("%v", errMap["message"]),
			}
		}
	}

	session.Done <- done
	close(session.Done)
}
