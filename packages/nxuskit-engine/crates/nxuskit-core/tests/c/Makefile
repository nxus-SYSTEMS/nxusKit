# Package-owned C ABI consumers for public-export runtime proofs.

CORE_DIR := $(abspath $(dir $(lastword $(MAKEFILE_LIST)))/../..)
ENGINE_WORKSPACE_DIR := $(abspath $(CORE_DIR)/../..)
INCLUDE_DIR := $(CORE_DIR)/include

ifeq ($(strip $(CARGO_TARGET_DIR)),)
$(error CARGO_TARGET_DIR is required for the proof-owned external target)
endif
ifeq ($(wildcard $(ENGINE_WORKSPACE_DIR)/Cargo.toml),)
$(error package engine workspace is absent)
endif
ifeq ($(wildcard $(INCLUDE_DIR)/nxuskit.h),)
$(error package C ABI header is absent)
endif

LIB_DIR := $(CARGO_TARGET_DIR)/debug
CC := cc
CFLAGS := -Wall -Wextra -Werror -I$(INCLUDE_DIR)
UNAME_S := $(shell uname -s)
ifeq ($(UNAME_S),Darwin)
CORE_LIBRARY := $(LIB_DIR)/libnxuskit_core.dylib
CORE_LINK_INPUT := -L$(LIB_DIR) -lnxuskit_core
RPATH_FLAG := -Wl,-rpath,$(LIB_DIR)
EXE_EXT :=
else ifeq ($(UNAME_S),Linux)
CORE_LIBRARY := $(LIB_DIR)/libnxuskit_core.so
CORE_LINK_INPUT := -L$(LIB_DIR) -lnxuskit_core
RPATH_FLAG := -Wl,-rpath,$(LIB_DIR)
EXE_EXT :=
else
CORE_LIBRARY := $(LIB_DIR)/nxuskit_core.dll
CORE_LINK_INPUT := $(CORE_LIBRARY)
RPATH_FLAG :=
EXE_EXT := .exe
WINDOWS_RUNTIME_PREREQ := stage-windows-runtime
endif

ACTION_GATE_PREVIEW_BINARY := $(CARGO_TARGET_DIR)/c-tests/action_gate_preview_test$(EXE_EXT)
DEPLOYMENT_TOKEN_V2_BINARY := $(CARGO_TARGET_DIR)/c-tests/deployment_token_v2_test$(EXE_EXT)

.PHONY: action-gate-preview deployment-token-v2 stage-windows-runtime

action-gate-preview: $(ACTION_GATE_PREVIEW_BINARY) $(WINDOWS_RUNTIME_PREREQ)
	$(ACTION_GATE_PREVIEW_BINARY)

$(ACTION_GATE_PREVIEW_BINARY): action_gate_preview_test.c $(INCLUDE_DIR)/nxuskit.h $(CORE_LIBRARY)
	@mkdir -p $(dir $(ACTION_GATE_PREVIEW_BINARY))
	$(CC) $(CFLAGS) -o $(ACTION_GATE_PREVIEW_BINARY) action_gate_preview_test.c $(CORE_LINK_INPUT) $(RPATH_FLAG)

deployment-token-v2: $(DEPLOYMENT_TOKEN_V2_BINARY) $(WINDOWS_RUNTIME_PREREQ)
	$(DEPLOYMENT_TOKEN_V2_BINARY)

$(DEPLOYMENT_TOKEN_V2_BINARY): deployment_token_v2_test.c $(INCLUDE_DIR)/nxuskit.h $(CORE_LIBRARY)
	@mkdir -p $(dir $(DEPLOYMENT_TOKEN_V2_BINARY))
	$(CC) $(CFLAGS) -o $(DEPLOYMENT_TOKEN_V2_BINARY) deployment_token_v2_test.c $(CORE_LINK_INPUT) $(RPATH_FLAG)

# The public import name in nxuskit.def is nxuskit.dll, while Cargo emits
# nxuskit_core.dll. Refresh beside the consumer even after a cache restore.
stage-windows-runtime: $(CORE_LIBRARY)
	@mkdir -p "$(CARGO_TARGET_DIR)/c-tests"
	cp "$(CORE_LIBRARY)" "$(CARGO_TARGET_DIR)/c-tests/nxuskit.dll"
