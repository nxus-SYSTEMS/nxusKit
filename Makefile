SHELL := /bin/bash

RUST_ENGINE_MANIFEST := packages/nxuskit-engine/Cargo.toml
RUST_WRAPPER_MANIFEST := packages/nxuskit/Cargo.toml
GO_DIR := packages/nxuskit-go
PYTHON_DIR := packages/nxuskit-py
PYTHON ?= $(shell command -v python3.13 2>/dev/null || command -v python3.12 2>/dev/null || command -v python3.11 2>/dev/null || command -v python3 2>/dev/null)
VERSION := $(shell sed -n 's/^version = "\(.*\)"/\1/p' $(RUST_ENGINE_MANIFEST) | head -1)
RUST_TARGET_DIR := packages/nxuskit-engine/target
RUST_DEBUG_DIR := $(RUST_TARGET_DIR)/debug
RUST_RELEASE_DIR := $(RUST_TARGET_DIR)/release
DIST_DIR ?= dist
UNAME_S := $(shell uname -s)
UNAME_M := $(shell uname -m)

ifeq ($(UNAME_S),Darwin)
  BUNDLE_OS := macos
  CORE_DYLIB := libnxuskit_core.dylib
  BUNDLE_DYLIB := libnxuskit.dylib
  ifeq ($(UNAME_M),arm64)
    BUNDLE_ARCH := arm64
    GO_LIB_PLATFORM := darwin_arm64
  else
    BUNDLE_ARCH := x86_64
    GO_LIB_PLATFORM := darwin_amd64
  endif
else ifeq ($(UNAME_S),Linux)
  BUNDLE_OS := linux
  CORE_DYLIB := libnxuskit_core.so
  BUNDLE_DYLIB := libnxuskit.so
  ifeq ($(UNAME_M),x86_64)
    BUNDLE_ARCH := x86_64
    GO_LIB_PLATFORM := linux_amd64
  else
    BUNDLE_ARCH := $(UNAME_M)
    GO_LIB_PLATFORM :=
  endif
else
  BUNDLE_OS := unknown
  BUNDLE_ARCH := $(UNAME_M)
  CORE_DYLIB :=
  BUNDLE_DYLIB :=
  GO_LIB_PLATFORM :=
endif

BUNDLE_PLATFORM := $(BUNDLE_OS)-$(BUNDLE_ARCH)
BUNDLE_DIR ?= $(DIST_DIR)/nxuskit-sdk-$(VERSION)-oss-$(BUNDLE_PLATFORM)
CE_RELEASE_ENV ?= NXUSKIT_ALLOW_DEV_KEY_IN_RELEASE=1 NXUSKIT_ALLOW_FALLBACK_CATALOG_IN_RELEASE=1

.PHONY: help build build-rust build-core build-cli build-go build-python build-release bundle cache-info cache-warm check check-rust public-ce-boundary qa clean

help:
	@echo "nxusKit Community Edition build targets"
	@echo ""
	@echo "  make build        Build CE native library, CLI, Rust wrapper check, Go SDK, and Python package"
	@echo "  make bundle       Build a local release-like CE SDK bundle under dist/"
	@echo "  make cache-info   Show local persistent build-cache paths"
	@echo "  make cache-warm   Warm local Rust, Go, and Python build caches"
	@echo "  make check        Run compile checks without producing release artifacts"
	@echo "  make public-ce-boundary"
	@echo "                    Verify public CE source has no Pro implementation/deps"
	@echo "  make qa           Run the CE source-build QA gate"
	@echo "  make clean        Remove local build outputs"

build: build-rust build-go build-python

build-rust: build-core build-cli
	cargo check --manifest-path $(RUST_WRAPPER_MANIFEST)

build-core:
	cargo build --manifest-path $(RUST_ENGINE_MANIFEST) -p nxuskit-core --no-default-features

build-cli:
	cargo build --manifest-path $(RUST_ENGINE_MANIFEST) -p nxuskit-cli --no-default-features

build-release:
	$(CE_RELEASE_ENV) cargo build --release --manifest-path $(RUST_ENGINE_MANIFEST) -p nxuskit-core --no-default-features
	$(CE_RELEASE_ENV) cargo build --release --manifest-path $(RUST_ENGINE_MANIFEST) -p nxuskit-cli --no-default-features
	cargo check --manifest-path $(RUST_WRAPPER_MANIFEST)

build-go:
	cd $(GO_DIR) && go build ./...

build-python:
	@test -n "$(PYTHON)" || (echo "Python 3.11+ is required; set PYTHON=/path/to/python3.11" && exit 1)
	@$(PYTHON) -c 'import sys; raise SystemExit(0 if sys.version_info >= (3, 11) else "Python 3.11+ is required; set PYTHON=/path/to/python3.11")'
	cd $(PYTHON_DIR) && $(PYTHON) -m compileall -q src

bundle: build-release
	@test "$(BUNDLE_OS)" != "unknown" || (echo "Unsupported local bundle platform: $(UNAME_S)/$(UNAME_M)" && exit 1)
	@test -f "$(RUST_RELEASE_DIR)/$(CORE_DYLIB)" || (echo "Missing dynamic library: $(RUST_RELEASE_DIR)/$(CORE_DYLIB)" && exit 1)
	@test -f "$(RUST_RELEASE_DIR)/libnxuskit_core.a" || (echo "Missing static library: $(RUST_RELEASE_DIR)/libnxuskit_core.a" && exit 1)
	@test -f "$(RUST_RELEASE_DIR)/nxuskit-cli" || (echo "Missing CLI binary: $(RUST_RELEASE_DIR)/nxuskit-cli" && exit 1)
	rm -rf "$(BUNDLE_DIR)"
	mkdir -p "$(BUNDLE_DIR)/bin" "$(BUNDLE_DIR)/lib" "$(BUNDLE_DIR)/include"
	cp "$(RUST_RELEASE_DIR)/$(CORE_DYLIB)" "$(BUNDLE_DIR)/lib/$(BUNDLE_DYLIB)"
	cp "$(RUST_RELEASE_DIR)/libnxuskit_core.a" "$(BUNDLE_DIR)/lib/libnxuskit.a"
	cp "$(RUST_RELEASE_DIR)/nxuskit-cli" "$(BUNDLE_DIR)/bin/nxuskit-cli"
	if [ "$(BUNDLE_OS)" = "macos" ] && command -v install_name_tool >/dev/null 2>&1; then \
		install_name_tool -id "@rpath/$(BUNDLE_DYLIB)" "$(BUNDLE_DIR)/lib/$(BUNDLE_DYLIB)"; \
	fi
	if [ "$(BUNDLE_OS)" = "macos" ] && command -v codesign >/dev/null 2>&1; then \
		codesign --sign - --force "$(BUNDLE_DIR)/lib/$(BUNDLE_DYLIB)"; \
		codesign --sign - --force "$(BUNDLE_DIR)/lib/libnxuskit.a"; \
		codesign --sign - --force "$(BUNDLE_DIR)/bin/nxuskit-cli"; \
	fi
	if [ "$(BUNDLE_OS)" = "linux" ] && command -v strip >/dev/null 2>&1; then \
		strip "$(BUNDLE_DIR)/bin/nxuskit-cli"; \
	fi
	cp packages/nxuskit-engine/crates/nxuskit-core/include/nxuskit.h "$(BUNDLE_DIR)/include/"
	cp LICENSE NOTICE "$(BUNDLE_DIR)/"
	if [ -d sdk-packaging/docs ]; then \
		cp -R sdk-packaging/docs "$(BUNDLE_DIR)/docs"; \
	elif [ -d docs/user ]; then \
		mkdir -p "$(BUNDLE_DIR)/docs"; \
		cp -R docs/user/. "$(BUNDLE_DIR)/docs/"; \
	else \
		cp -R docs "$(BUNDLE_DIR)/docs"; \
	fi
	if [ -d sdk-packaging/examples ]; then \
		cp -R sdk-packaging/examples "$(BUNDLE_DIR)/examples"; \
	else \
		mkdir -p "$(BUNDLE_DIR)/examples"; \
		printf '%s\n' '# nxusKit examples' '' 'Install the public nxusKit-examples repository for the full examples set.' > "$(BUNDLE_DIR)/examples/README.md"; \
	fi
	if [ -d sdk-packaging/scripts ]; then \
		cp -R sdk-packaging/scripts "$(BUNDLE_DIR)/scripts"; \
	else \
		mkdir -p "$(BUNDLE_DIR)/scripts"; \
	fi
	mkdir -p "$(BUNDLE_DIR)/rust"
	cp -R packages/nxuskit/src "$(BUNDLE_DIR)/rust/src"
	cp packages/nxuskit/Cargo.toml packages/nxuskit/build.rs packages/nxuskit/README.md packages/nxuskit/CHANGELOG.md "$(BUNDLE_DIR)/rust/"
	cp -R "$(GO_DIR)" "$(BUNDLE_DIR)/go"
	rm -rf "$(BUNDLE_DIR)/go/bin" "$(BUNDLE_DIR)/go/.git"
	mkdir -p "$(BUNDLE_DIR)/go/include" "$(BUNDLE_DIR)/go/lib"
	cp "$(BUNDLE_DIR)/include/nxuskit.h" "$(BUNDLE_DIR)/go/include/nxuskit.h"
	if [ -n "$(GO_LIB_PLATFORM)" ]; then ln -sf ../../lib "$(BUNDLE_DIR)/go/lib/$(GO_LIB_PLATFORM)"; fi
	mkdir -p "$(BUNDLE_DIR)/python"
	cp -R "$(PYTHON_DIR)/src" "$(BUNDLE_DIR)/python/src"
	cp "$(PYTHON_DIR)/pyproject.toml" "$(PYTHON_DIR)/README.md" "$(BUNDLE_DIR)/python/"
	mkdir -p "$(BUNDLE_DIR)/conformance"
	if [ -d sdk-packaging/conformance ]; then \
		cp sdk-packaging/conformance/*.json "$(BUNDLE_DIR)/conformance/"; \
	else \
		cp -R conformance/. "$(BUNDLE_DIR)/conformance/"; \
	fi
	if [ -x scripts/generate-capability-manifest.sh ]; then \
		scripts/generate-capability-manifest.sh "$(BUNDLE_DIR)/lib" --edition oss --output "$(BUNDLE_DIR)/capability-manifest.json"; \
	else \
		printf '%s\n' '{"edition":"oss","sdk_version":"$(VERSION)","platform":"$(BUNDLE_OS)","arch":"$(BUNDLE_ARCH)"}' > "$(BUNDLE_DIR)/capability-manifest.json"; \
	fi
	@echo "CE SDK bundle ready: $(BUNDLE_DIR)"

cache-info:
	scripts/local-cache-env.sh

cache-warm:
	scripts/local-cache-warm.sh

check: check-rust build-go build-python

check-rust:
	cargo check --manifest-path $(RUST_ENGINE_MANIFEST) -p nxuskit-core --no-default-features
	cargo check --manifest-path $(RUST_ENGINE_MANIFEST) -p nxuskit-engine --no-default-features
	cargo check --manifest-path $(RUST_ENGINE_MANIFEST) -p nxuskit-cli --no-default-features
	cargo check --manifest-path $(RUST_WRAPPER_MANIFEST)

public-ce-boundary:
	bash scripts/assert-public-ce-clean.sh

qa: check public-ce-boundary
	cd $(GO_DIR) && go test ./...

clean:
	cargo clean --manifest-path $(RUST_ENGINE_MANIFEST)
	rm -rf "$(DIST_DIR)"
	cd $(GO_DIR) && go clean -cache
