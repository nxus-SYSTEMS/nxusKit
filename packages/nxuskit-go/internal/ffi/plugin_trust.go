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

// PluginSetTrustMode calls nxuskit_plugin_set_trust_mode.
func PluginSetTrustMode(mode int32) int32 {
	return int32(C.nxuskit_plugin_set_trust_mode(C.int32_t(mode)))
}

// PluginGetTrustMode calls nxuskit_plugin_get_trust_mode.
func PluginGetTrustMode() int32 {
	return int32(C.nxuskit_plugin_get_trust_mode())
}

// PluginLoadDirTrusted calls nxuskit_plugin_load_dir_trusted.
func PluginLoadDirTrusted(dir string) (int32, error) {
	cDir := C.CString(dir)
	defer C.free(unsafe.Pointer(cDir))

	result := int32(C.nxuskit_plugin_load_dir_trusted(cDir))
	if result < 0 {
		return 0, fmt.Errorf("nxuskit_plugin_load_dir_trusted returned %d: %s", result, LastError())
	}
	return result, nil
}

// PluginList returns all loaded plugin names as a JSON array string.
func PluginList() (string, error) {
	ptr := C.nxuskit_plugin_list()
	if ptr == nil {
		return "", fmt.Errorf("plugin_list: %s", LastError())
	}
	s := C.GoString(ptr)
	C.nxuskit_free_string(ptr)
	return s, nil
}

// PluginInfo returns metadata for a loaded plugin as a JSON string.
func PluginInfo(name string) (string, error) {
	cName := C.CString(name)
	defer C.free(unsafe.Pointer(cName))

	ptr := C.nxuskit_plugin_info(cName)
	if ptr == nil {
		return "", fmt.Errorf("plugin_info(%s): %s", name, LastError())
	}
	s := C.GoString(ptr)
	C.nxuskit_free_string(ptr)
	return s, nil
}

// PluginCount returns the number of loaded plugins.
func PluginCount() int32 {
	return int32(C.nxuskit_plugin_count())
}

// PluginLoaded returns true if a plugin with the given name is loaded.
func PluginLoaded(name string) bool {
	cName := C.CString(name)
	defer C.free(unsafe.Pointer(cName))
	return int32(C.nxuskit_plugin_loaded(cName)) != 0
}

// PluginUnloadAll unloads all loaded plugins.
func PluginUnloadAll() {
	C.nxuskit_plugin_unload_all()
}
