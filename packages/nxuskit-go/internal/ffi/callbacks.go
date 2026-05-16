//go:build nxuskit

package ffi

/*
#include "nxuskit.h"
#include <stdlib.h>

// C trampoline functions that forward to the Go //export functions.
// These avoid the cgo "conflicting types" issue that occurs when
// //export and C.functionName reference the same function in one file.
extern int32_t goStreamChunkCallback(const char *chunk_json, void *user_data);
extern void goStreamDoneCallback(const char *final_json, void *user_data);

int32_t cgoChunkTrampoline(const char *chunk_json, void *user_data) {
    return goStreamChunkCallback(chunk_json, user_data);
}

void cgoDoneTrampoline(const char *final_json, void *user_data) {
    goStreamDoneCallback(final_json, user_data);
}
*/
import "C"

import (
	"fmt"
	"runtime/cgo"
	"unsafe"
)

// StreamSession holds the state for an in-flight streaming request.
// The Chunks channel receives incremental chunk JSON objects.
// The Done channel receives the final aggregated response (or error) exactly once.
type StreamSession struct {
	Chunks chan StreamChunkData
	Done   chan StreamDoneData
	handle cgo.Handle
	stream *C.struct_NxuskitStream
}

// StreamChunkData is a single streaming chunk.
type StreamChunkData struct {
	Content string `json:"delta"`
	Index   int    `json:"index"`
}

// StreamDoneData is the final aggregated response from streaming.
type StreamDoneData struct {
	Content string         `json:"delta"`
	Error   *StreamError   `json:"error,omitempty"`
	Raw     map[string]any // Full parsed JSON
}

// StreamError captures error info from the streaming response.
type StreamError struct {
	ErrorType string `json:"error_type"`
	Message   string `json:"message"`
}

// ChatStream starts a streaming chat request. Returns a StreamSession
// that the caller reads from. The caller must call session.Close() when done.
func (h *ProviderHandle) ChatStream(requestJSON string) (*StreamSession, error) {
	if h.ptr == nil {
		return nil, fmt.Errorf("nxuskit: provider handle is nil")
	}

	session := &StreamSession{
		Chunks: make(chan StreamChunkData, 64),
		Done:   make(chan StreamDoneData, 1),
	}

	// Create a cgo.Handle so the C callbacks can find this session.
	session.handle = cgo.NewHandle(session)

	cReq := C.CString(requestJSON)
	defer C.free(unsafe.Pointer(cReq))

	stream := C.nxuskit_chat_stream(
		h.ptr,
		cReq,
		C.NxuskitStreamCallback(C.cgoChunkTrampoline),
		C.NxuskitStreamDoneCallback(C.cgoDoneTrampoline),
		unsafe.Pointer(&session.handle),
	)
	if stream == nil {
		session.handle.Delete()
		return nil, fmt.Errorf("nxuskit: chat_stream failed: %s", LastError())
	}

	session.stream = stream
	return session, nil
}

// Cancel cancels the stream. Blocks until all callbacks have completed.
func (s *StreamSession) Cancel() {
	if s.stream != nil {
		C.nxuskit_cancel_stream(s.stream)
	}
}

// Close frees the stream handle and the cgo.Handle.
// Must be called after the stream completes or is cancelled.
func (s *StreamSession) Close() {
	if s.stream != nil {
		C.nxuskit_free_stream(s.stream)
		s.stream = nil
	}
	if s.handle != 0 {
		s.handle.Delete()
		s.handle = 0
	}
}
