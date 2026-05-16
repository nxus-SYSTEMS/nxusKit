#!/usr/bin/env bash
# nxusKit SDK Manager
#
# Manages nxusKit SDK installations under ~/.nxuskit/sdk/
#
# Usage:
#   install.sh install [VERSION]     Install an SDK version (default: latest)
#   install.sh uninstall VERSION     Remove a specific SDK version
#   install.sh list                  List installed versions
#   install.sh use VERSION           Switch active SDK version
#   install.sh status                Show current SDK status
#   install.sh cleanup [--keep N]    Remove old versions (keep N most recent, default 2)
#   install.sh --version             Show installer version
#
# Environment:
#   NXUSKIT_TOKEN  GitHub PAT for nxus-SYSTEMS/nxusKit releases (preferred)
#   GH_TOKEN       Fallback token if NXUSKIT_TOKEN is not set
#   NXUSKIT_REPO   Override repository (default: nxus-SYSTEMS/nxusKit)
#
# Layout:
#   ~/.nxuskit/
#     sdk/
#       nxuskit-sdk-0.7.9-macos-arm64/   (extracted SDK)
#       nxuskit-sdk-0.7.8-linux-x86_64/  (extracted SDK)
#       current -> nxuskit-sdk-0.7.9-macos-arm64  (symlink)

set -euo pipefail

INSTALLER_VERSION="1.0.0"
NXUSKIT_HOME="${HOME}/.nxuskit"
SDK_DIR="${NXUSKIT_HOME}/sdk"
REPO="${NXUSKIT_REPO:-nxus-SYSTEMS/nxusKit}"

# ─────────────────────────────────────────────────────────────────
# Output controls
# ─────────────────────────────────────────────────────────────────
QUIET=false

# Colors (disabled when not a terminal or in quiet mode)
if [[ -t 1 ]]; then
    RED='\033[0;31m'
    GREEN='\033[0;32m'
    YELLOW='\033[0;33m'
    CYAN='\033[0;36m'
    BOLD='\033[1m'
    NC='\033[0m'
else
    RED='' GREEN='' YELLOW='' CYAN='' BOLD='' NC=''
fi

info()  { $QUIET || echo -e "${CYAN}[nxuskit]${NC} $*"; }
ok()    { $QUIET || echo -e "${GREEN}[nxuskit]${NC} $*"; }
warn()  { $QUIET || echo -e "${YELLOW}[nxuskit]${NC} $*" >&2; }
err()   { echo -e "${RED}[nxuskit]${NC} $*" >&2; }
die()   { err "$@"; exit 1; }

# ─────────────────────────────────────────────────────────────────
# Platform detection
# ─────────────────────────────────────────────────────────────────
detect_platform() {
    local os arch
    os="$(uname -s)"
    arch="$(uname -m)"

    case "$os" in
        Darwin) os="macos" ;;
        Linux)  os="linux" ;;
        MINGW*|MSYS*|CYGWIN*) os="windows" ;;
        *) die "Unsupported OS: $os" ;;
    esac

    case "$arch" in
        arm64|aarch64) arch="arm64" ;;
        x86_64|amd64)  arch="x86_64" ;;
        *) die "Unsupported architecture: $arch" ;;
    esac

    echo "${os}-${arch}"
}

# ─────────────────────────────────────────────────────────────────
# GitHub authentication
# ─────────────────────────────────────────────────────────────────
setup_gh_auth() {
    if [[ -n "${NXUSKIT_TOKEN:-}" ]]; then
        export GH_TOKEN="$NXUSKIT_TOKEN"
    elif [[ -n "${GH_TOKEN:-}" ]]; then
        : # Already set
    elif command -v gh &>/dev/null && gh auth status &>/dev/null 2>&1; then
        export GH_TOKEN="$(gh auth token 2>/dev/null || true)"
    fi

    if [[ -z "${GH_TOKEN:-}" ]]; then
        die "No GitHub token found. Set NXUSKIT_TOKEN or GH_TOKEN, or run 'gh auth login'"
    fi
}

# ─────────────────────────────────────────────────────────────────
# Resolve version tag
# ─────────────────────────────────────────────────────────────────
resolve_version() {
    local requested="${1:-latest}"

    if [[ "$requested" == "latest" ]]; then
        # Get the latest sdk-v* release tag (filter to only sdk-v* tags)
        setup_gh_auth
        local tag
        tag="$(gh release list --repo "$REPO" --limit 50 --json tagName \
            --jq '[.[] | select(.tagName | startswith("sdk-v"))][0].tagName' 2>/dev/null)" || \
            die "Failed to query releases from $REPO"
        [[ -z "$tag" || "$tag" == "null" ]] && die "No sdk-v* releases found in $REPO"
        echo "$tag"
    elif [[ "$requested" == sdk-v* ]]; then
        echo "$requested"
    elif [[ "$requested" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
        echo "sdk-v${requested}"
    else
        die "Invalid version format: $requested (expected: 0.7.8, sdk-v0.7.8, or 'latest')"
    fi
}

# Resolve version from locally installed SDKs only (no network)
resolve_version_local() {
    local requested="${1:-}"
    local platform
    platform="$(detect_platform)"

    if [[ -z "$requested" ]]; then
        die "Version required. Run: $(basename "$0") list"
    fi

    if [[ "$requested" == "latest" ]]; then
        # Find the newest installed version by directory modification time
        local newest
        newest="$(ls -dt "${SDK_DIR}"/nxuskit-sdk-*-"${platform}" 2>/dev/null | head -1)"
        if [[ -z "$newest" ]]; then
            die "No SDK versions installed for ${platform}. Run: $(basename "$0") install"
        fi
        local dirname
        dirname="$(basename "$newest")"
        # Extract version: nxuskit-sdk-0.7.9-macos-arm64 -> 0.7.9
        local version="${dirname#nxuskit-sdk-}"
        version="${version%-"${platform}"}"
        echo "sdk-v${version}"
    elif [[ "$requested" == sdk-v* ]]; then
        echo "$requested"
    elif [[ "$requested" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
        echo "sdk-v${requested}"
    else
        die "Invalid version format: $requested (expected: 0.7.8, sdk-v0.7.8, or 'latest')"
    fi
}

# Extract version number from tag (sdk-v0.7.8 -> 0.7.8)
version_from_tag() {
    echo "${1#sdk-v}"
}

# ─────────────────────────────────────────────────────────────────
# Temp directory cleanup helper
# ─────────────────────────────────────────────────────────────────
_cleanup_tmpdir=""

cleanup_tmpdir() {
    if [[ -n "$_cleanup_tmpdir" && -d "$_cleanup_tmpdir" ]]; then
        rm -rf "$_cleanup_tmpdir"
    fi
}

# ─────────────────────────────────────────────────────────────────
# install [VERSION]
# ─────────────────────────────────────────────────────────────────
cmd_install() {
    setup_gh_auth

    local platform tag version dirname archive_pattern ext
    platform="$(detect_platform)"
    tag="$(resolve_version "${1:-latest}")"
    version="$(version_from_tag "$tag")"
    dirname="nxuskit-sdk-${version}-${platform}"

    # Check if already installed
    if [[ -d "${SDK_DIR}/${dirname}" ]]; then
        ok "SDK ${version} (${platform}) is already installed"
        cmd_use "$tag"
        return 0
    fi

    # Determine archive pattern
    case "$platform" in
        windows-*) ext="zip"; archive_pattern="*${platform}.zip" ;;
        *)         ext="tar.gz"; archive_pattern="*${platform}.tar.gz" ;;
    esac

    info "Installing nxusKit SDK ${version} for ${platform}..."

    # Download to temp directory with reliable cleanup
    _cleanup_tmpdir="$(mktemp -d)"
    trap cleanup_tmpdir EXIT

    local tmpdir="$_cleanup_tmpdir"

    info "Downloading from ${REPO} tag ${tag}..."
    gh release download "$tag" \
        --repo "$REPO" \
        --pattern "$archive_pattern" \
        --dir "$tmpdir" || die "Failed to download SDK (check token permissions and that release ${tag} exists)"

    # Verify SHA256 if checksum file is available
    if gh release download "$tag" --repo "$REPO" --pattern "${archive_pattern}.sha256" --dir "$tmpdir" 2>/dev/null; then
        local sha_file
        sha_file="$(find "$tmpdir" -name "*.sha256" -print -quit 2>/dev/null)"
        if [[ -n "$sha_file" ]]; then
            info "Verifying checksum..."
            local archive_file expected_sha actual_sha
            archive_file="$(find "$tmpdir" -name "*.${ext}" -not -name "*.sha256" -print -quit 2>/dev/null)"
            expected_sha="$(awk '{print $1}' "$sha_file")"
            if command -v sha256sum &>/dev/null; then
                actual_sha="$(sha256sum "$archive_file" | awk '{print $1}')"
            elif command -v shasum &>/dev/null; then
                actual_sha="$(shasum -a 256 "$archive_file" | awk '{print $1}')"
            else
                warn "No sha256sum or shasum found — skipping checksum verification"
            fi
            if [[ -n "${actual_sha:-}" && "$actual_sha" != "$expected_sha" ]]; then
                die "Checksum mismatch! Expected: ${expected_sha}, Got: ${actual_sha}"
            fi
            ok "Checksum verified"
        fi
    fi

    # Extract
    mkdir -p "${SDK_DIR}/${dirname}"
    info "Extracting to ${SDK_DIR}/${dirname}/..."

    case "$ext" in
        tar.gz)
            tar -xzf "$tmpdir"/*."${ext}" -C "${SDK_DIR}/${dirname}" --strip-components=1
            ;;
        zip)
            local extract_tmp="${tmpdir}/extract"
            mkdir -p "$extract_tmp"
            unzip -q "$tmpdir"/*."${ext}" -d "$extract_tmp"
            # Move contents up (zip archives may have a top-level dir)
            local top_dir
            top_dir="$(ls "$extract_tmp")"
            if [[ -d "${extract_tmp}/${top_dir}/lib" ]]; then
                mv "${extract_tmp}/${top_dir}"/* "${SDK_DIR}/${dirname}/"
            else
                mv "${extract_tmp}"/* "${SDK_DIR}/${dirname}/"
            fi
            ;;
    esac

    ok "SDK ${version} installed to ${SDK_DIR}/${dirname}"

    # Set as current
    cmd_use "$tag"

    # Cleanup
    trap - EXIT
    cleanup_tmpdir
    _cleanup_tmpdir=""
}

# ─────────────────────────────────────────────────────────────────
# uninstall VERSION
# ─────────────────────────────────────────────────────────────────
cmd_uninstall() {
    local tag version platform dirname
    tag="$(resolve_version_local "${1:-}")"
    version="$(version_from_tag "$tag")"
    platform="$(detect_platform)"
    dirname="nxuskit-sdk-${version}-${platform}"

    if [[ ! -d "${SDK_DIR}/${dirname}" ]]; then
        die "SDK ${version} (${platform}) is not installed."
    fi

    # Check if this is the active version
    local current_target=""
    if [[ -L "${SDK_DIR}/current" ]]; then
        current_target="$(readlink "${SDK_DIR}/current")"
    fi

    if [[ "$dirname" == "$current_target" ]]; then
        warn "Removing active SDK version — unsetting 'current' symlink"
        rm -f "${SDK_DIR}/current"
    fi

    info "Removing ${dirname}..."
    rm -rf "${SDK_DIR}/${dirname}"
    ok "SDK ${version} (${platform}) uninstalled."
}

# ─────────────────────────────────────────────────────────────────
# use VERSION
# ─────────────────────────────────────────────────────────────────
cmd_use() {
    local tag version platform dirname
    # Use local resolution — 'use' should not require network
    tag="$(resolve_version_local "${1:-}")"
    version="$(version_from_tag "$tag")"
    platform="$(detect_platform)"
    dirname="nxuskit-sdk-${version}-${platform}"

    if [[ ! -d "${SDK_DIR}/${dirname}" ]]; then
        die "SDK ${version} (${platform}) not installed. Run: $(basename "$0") install ${version}"
    fi

    # Update symlink
    rm -f "${SDK_DIR}/current"
    ln -sf "${dirname}" "${SDK_DIR}/current"

    ok "Active SDK: ${version} (${platform})"
    info "  ${SDK_DIR}/current -> ${dirname}"
    info ""
    info "In your repo, create a .sdk symlink:"
    info "  ln -sf ~/.nxuskit/sdk/current /path/to/repo/.sdk"
}

# ─────────────────────────────────────────────────────────────────
# list
# ─────────────────────────────────────────────────────────────────
cmd_list() {
    if [[ ! -d "$SDK_DIR" ]]; then
        info "No SDK versions installed."
        return 0
    fi

    local current_target=""
    if [[ -L "${SDK_DIR}/current" ]]; then
        current_target="$(readlink "${SDK_DIR}/current")"
    fi

    info "Installed SDK versions:"
    echo ""

    local found=false
    for dir in "${SDK_DIR}"/nxuskit-sdk-*; do
        [[ -d "$dir" ]] || continue
        found=true
        local name
        name="$(basename "$dir")"
        if [[ "$name" == "$current_target" ]]; then
            echo -e "  ${GREEN}* ${name}${NC} (active)"
        else
            echo "    ${name}"
        fi
    done

    if ! $found; then
        info "  (none)"
    fi

    echo ""

    # Also list available remote versions (best-effort, no hard failure)
    if [[ -n "${GH_TOKEN:-}" ]] || [[ -n "${NXUSKIT_TOKEN:-}" ]] || \
       (command -v gh &>/dev/null && gh auth status &>/dev/null 2>&1); then
        # Set up auth for remote query
        if [[ -n "${NXUSKIT_TOKEN:-}" ]]; then
            local _saved_gh_token="${GH_TOKEN:-}"
            export GH_TOKEN="$NXUSKIT_TOKEN"
        fi

        info "Available remote versions:"
        gh release list --repo "$REPO" --limit 10 --json tagName,publishedAt \
            --jq '.[] | select(.tagName | startswith("sdk-v")) | "  \(.tagName)  (\(.publishedAt | split("T")[0]))"' 2>/dev/null || \
            warn "  (could not query remote releases)"

        # Restore GH_TOKEN if we overrode it
        if [[ -n "${NXUSKIT_TOKEN:-}" ]]; then
            export GH_TOKEN="${_saved_gh_token}"
        fi
    fi
}

# ─────────────────────────────────────────────────────────────────
# status
# ─────────────────────────────────────────────────────────────────
cmd_status() {
    echo -e "${BOLD}nxusKit SDK Status${NC}"
    echo ""

    # Current version
    if [[ -L "${SDK_DIR}/current" ]]; then
        local target
        target="$(readlink "${SDK_DIR}/current")"
        echo -e "  Active version: ${GREEN}${target}${NC}"
        echo "  Path: ${SDK_DIR}/current -> ${target}"
    else
        echo -e "  Active version: ${YELLOW}(none)${NC}"
    fi
    echo ""

    # Installed count
    local count=0
    for dir in "${SDK_DIR}"/nxuskit-sdk-*; do
        [[ -d "$dir" ]] && count=$((count + 1))
    done
    echo "  Installed versions: ${count}"
    echo ""

    # Disk usage
    if [[ $count -gt 0 ]]; then
        local disk_usage
        disk_usage="$(du -sh "${SDK_DIR}" 2>/dev/null | awk '{print $1}')"
        echo "  Disk usage: ${disk_usage}"
    fi
    echo ""

    # Check lib contents
    if [[ -L "${SDK_DIR}/current" && -d "${SDK_DIR}/current/lib" ]]; then
        echo "  Libraries:"
        ls -1 "${SDK_DIR}/current/lib/" 2>/dev/null | while read -r f; do
            echo "    ${f}"
        done
    fi

    echo ""
    echo "  Installer version: ${INSTALLER_VERSION}"
    echo "  Repository: ${REPO}"
}

# ─────────────────────────────────────────────────────────────────
# cleanup [--keep N]
# ─────────────────────────────────────────────────────────────────
cmd_cleanup() {
    local keep=2

    while [[ $# -gt 0 ]]; do
        case "$1" in
            --keep) keep="${2:?'--keep requires a number'}"; shift 2 ;;
            --keep=*) keep="${1#--keep=}"; shift ;;
            *) die "Unknown option: $1" ;;
        esac
    done

    if [[ ! -d "$SDK_DIR" ]]; then
        info "Nothing to clean up."
        return 0
    fi

    local current_target
    current_target=""
    if [[ -L "${SDK_DIR}/current" ]]; then
        current_target="$(readlink "${SDK_DIR}/current")"
    fi

    # Collect installed versions sorted by modification time (newest first)
    local dirs=()
    while IFS= read -r d; do
        [[ -d "$d" ]] && dirs+=("$(basename "$d")")
    done < <(ls -dt "${SDK_DIR}"/nxuskit-sdk-* 2>/dev/null)

    if [[ ${#dirs[@]} -le $keep ]]; then
        ok "Only ${#dirs[@]} version(s) installed (keep=$keep). Nothing to remove."
        return 0
    fi

    local removed=0
    for ((i = keep; i < ${#dirs[@]}; i++)); do
        local name="${dirs[$i]}"
        if [[ "$name" == "$current_target" ]]; then
            warn "Skipping active version: ${name}"
            continue
        fi
        info "Removing ${name}..."
        rm -rf "${SDK_DIR}/${name}"
        removed=$((removed + 1))
    done

    ok "Removed ${removed} version(s), kept ${keep}."
}

# ─────────────────────────────────────────────────────────────────
# Main
# ─────────────────────────────────────────────────────────────────
main() {
    # Parse global flags
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --version|-V)
                echo "nxusKit SDK Manager v${INSTALLER_VERSION}"
                exit 0
                ;;
            --quiet|-q)
                QUIET=true
                shift
                ;;
            -*)
                # Check if it's a command flag (like --keep) — if so, break
                break
                ;;
            *)
                break
                ;;
        esac
    done

    mkdir -p "$SDK_DIR"

    local cmd="${1:-help}"
    shift || true

    case "$cmd" in
        install)    cmd_install "$@" ;;
        uninstall)  cmd_uninstall "$@" ;;
        use)        cmd_use "$@" ;;
        list)       cmd_list "$@" ;;
        status)     cmd_status "$@" ;;
        cleanup)    cmd_cleanup "$@" ;;
        help|-h|--help)
            echo "nxusKit SDK Manager v${INSTALLER_VERSION}"
            echo ""
            echo "Usage: $(basename "$0") [--quiet] <command> [args]"
            echo ""
            echo "Commands:"
            echo "  install [VERSION]     Install an SDK version (default: latest)"
            echo "  uninstall VERSION     Remove a specific SDK version"
            echo "  list                  List installed and available versions"
            echo "  use VERSION           Switch active SDK version"
            echo "  status                Show current SDK status"
            echo "  cleanup [--keep N]    Remove old versions (keep N most recent, default 2)"
            echo ""
            echo "Options:"
            echo "  --version, -V         Show installer version"
            echo "  --quiet, -q           Suppress informational messages"
            echo ""
            echo "Examples:"
            echo "  $(basename "$0") install              # Install latest"
            echo "  $(basename "$0") install 0.7.9        # Install specific version"
            echo "  $(basename "$0") install sdk-v0.7.9   # Install by tag name"
            echo "  $(basename "$0") use 0.7.9            # Switch to version"
            echo "  $(basename "$0") uninstall 0.7.8      # Remove a version"
            echo "  $(basename "$0") cleanup --keep 3     # Keep 3 most recent versions"
            echo ""
            echo "After installing, create a .sdk symlink in your repo:"
            echo "  ln -sf ~/.nxuskit/sdk/current /path/to/repo/.sdk"
            echo ""
            echo "Environment:"
            echo "  NXUSKIT_TOKEN   GitHub PAT for private repo access (preferred)"
            echo "  GH_TOKEN        Fallback token if NXUSKIT_TOKEN is not set"
            echo "  NXUSKIT_REPO    Override repository (default: nxus-SYSTEMS/nxusKit)"
            ;;
        *)
            die "Unknown command: $cmd (try 'help')"
            ;;
    esac
}

main "$@"
