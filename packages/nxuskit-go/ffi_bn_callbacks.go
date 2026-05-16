//go:build nxuskit

package nxuskit

/*
#include <stdbool.h>
#include <stdint.h>
*/
import "C"

import (
	"runtime/cgo"
	"unsafe"
)

//export goBnStreamCallback
func goBnStreamCallback(chunkJSON *C.char, iteration C.uint32_t, total C.uint32_t, isFinal C._Bool, userData unsafe.Pointer) C._Bool {
	h := *(*cgo.Handle)(userData)
	session := h.Value().(*bnStreamSession)

	chunk := BnStreamChunk{
		ChunkJSON: C.GoString(chunkJSON),
		Iteration: uint32(iteration),
		Total:     uint32(total),
		IsFinal:   bool(isFinal),
	}

	// Non-blocking send — drop chunk if buffer is full to avoid blocking C runtime.
	select {
	case session.chunks <- chunk:
	default:
	}

	return C._Bool(true) // continue streaming
}
