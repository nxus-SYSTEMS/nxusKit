# Makefile for nxuskit C smoke test
#
# Usage:
#   make            — build and run the smoke test (debug)
#   make release    — build and run against the release library
#   make clean      — remove build artifacts
#
# Prerequisites:
#   - nxuskit-core must be built first:
#     cargo build -p nxuskit-core           (debug)
#     cargo build --release -p nxuskit-core (release)

CRATE_DIR := ../..
INCLUDE_DIR := $(CRATE_DIR)/include

# Detect target directory (respect CARGO_TARGET_DIR)
CARGO_TARGET_DIR ?= $(shell cargo metadata --format-version 1 2>/dev/null | python3 -c "import sys,json; print(json.load(sys.stdin)['target_directory'])" 2>/dev/null || echo "$(CRATE_DIR)/../../../../target")

CC := cc
CFLAGS := -Wall -Wextra -Werror -I$(INCLUDE_DIR)
BINARY := bin/smoke_test

# Platform-specific library name and link flags
UNAME_S := $(shell uname -s)
ifeq ($(UNAME_S),Darwin)
    LIB_NAME := libnxuskit_core.dylib
    RPATH_FLAG = -Wl,-rpath,$(LIB_DIR)
else ifeq ($(UNAME_S),Linux)
    LIB_NAME := libnxuskit_core.so
    RPATH_FLAG = -Wl,-rpath,$(LIB_DIR)
else
    # Windows (MSYS/MinGW)
    LIB_NAME := nxuskit_core.dll
    RPATH_FLAG :=
endif

.PHONY: all release clean

all: LIB_DIR := $(CARGO_TARGET_DIR)/debug
all: build run

release: LIB_DIR := $(CARGO_TARGET_DIR)/release
release: build run

build:
	@mkdir -p bin
	$(CC) $(CFLAGS) -o $(BINARY) smoke_test.c -L$(LIB_DIR) -lnxuskit_core $(RPATH_FLAG)

run:
	@echo "--- Running C smoke test ---"
	$(BINARY)

clean:
	rm -rf bin
