#!/usr/bin/env bash
# E2E Smoke Test for Rose (Rust implementation)
#
# Exercises every non-interactive CLI subcommand against a temporary library
# built from testdata/. Verifies exit codes and basic output.
#
# Usage:
#   bash rose-rs/tests/e2e_smoke.sh
#
# Environment variables:
#   ROSE_BIN         - Path to the rose binary (default: builds with cargo)
#   ROSE_TEST_FUSE   - Set to "1" to run VFS mount/unmount tests (requires FUSE)

set -euo pipefail

# --------------------------------------------------------------------------
# Colors / helpers
# --------------------------------------------------------------------------

GREEN='\033[0;32m'
RED='\033[0;31m'
BOLD='\033[1m'
NC='\033[0m'

run_cmd() {
    echo -e "${BOLD}>> $*${NC}"
    "$@"
    echo -e "${GREEN}   OK${NC}"
}

# --------------------------------------------------------------------------
# Setup
# --------------------------------------------------------------------------

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TESTDATA="$REPO_ROOT/testdata"

if [[ ! -d "$TESTDATA" ]]; then
    echo -e "${RED}ERROR: testdata/ not found at $TESTDATA${NC}" >&2
    exit 1
fi

# Build or locate the rose binary
if [[ -n "${ROSE_BIN:-}" ]]; then
    ROSE="$ROSE_BIN"
else
    echo "Building rose binary (release)..."
    cargo build --release -p rose-cli --manifest-path "$REPO_ROOT/rose-rs/Cargo.toml"
    ROSE="$REPO_ROOT/rose-rs/target/release/rose"
fi

if [[ ! -x "$ROSE" ]]; then
    echo -e "${RED}ERROR: rose binary not found at $ROSE${NC}" >&2
    exit 1
fi

# Create temp directory with a copy of testdata
TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT
cp -a "$TESTDATA/." "$TMPDIR/music_source/"
MUSIC_SOURCE="$TMPDIR/music_source"

# Write minimal config
CONFIG="$TMPDIR/config.toml"
cat > "$CONFIG" <<EOF
music_source_dir = "$MUSIC_SOURCE"

[vfs]
mount_dir = "$TMPDIR/mnt"
EOF

mkdir -p "$TMPDIR/mnt"

# We pass --config to every invocation
ROSE_CMD=("$ROSE" "--config" "$CONFIG")

echo ""
echo "========================================"
echo " Rose E2E Smoke Test"
echo "========================================"
echo " Binary:       $ROSE"
echo " Music source: $MUSIC_SOURCE"
echo " Config:       $CONFIG"
echo "========================================"
echo ""

# --------------------------------------------------------------------------
# 1. Version
# --------------------------------------------------------------------------

run_cmd "${ROSE_CMD[@]}" version

# --------------------------------------------------------------------------
# 2. Cache update (initial population)
# --------------------------------------------------------------------------

run_cmd "${ROSE_CMD[@]}" cache update

# --------------------------------------------------------------------------
# 3. Releases
# --------------------------------------------------------------------------

# Print all releases — capture output to extract a release ID
RELEASES_JSON=$("${ROSE_CMD[@]}" releases print-all)
echo -e "${BOLD}>> rose releases print-all${NC}"
echo "$RELEASES_JSON" | head -5
echo -e "${GREEN}   OK${NC}"

# Extract first release UUID from the JSON output.
# The JSON is an array of objects; grab the first "id" field.
RELEASE_ID=$(echo "$RELEASES_JSON" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)
if [[ -z "$RELEASE_ID" ]]; then
    echo -e "${RED}ERROR: Could not extract a release ID from releases print-all${NC}" >&2
    exit 1
fi
echo "  Using release ID: $RELEASE_ID"

run_cmd "${ROSE_CMD[@]}" releases print "$RELEASE_ID"
run_cmd "${ROSE_CMD[@]}" releases toggle-new "$RELEASE_ID"
run_cmd "${ROSE_CMD[@]}" releases toggle-favorite "$RELEASE_ID"
run_cmd "${ROSE_CMD[@]}" releases set-rating "$RELEASE_ID" 80

# --------------------------------------------------------------------------
# 4. Tracks
# --------------------------------------------------------------------------

TRACKS_JSON=$("${ROSE_CMD[@]}" tracks print-all)
echo -e "${BOLD}>> rose tracks print-all${NC}"
echo "$TRACKS_JSON" | head -5
echo -e "${GREEN}   OK${NC}"

TRACK_ID=$(echo "$TRACKS_JSON" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)
if [[ -z "$TRACK_ID" ]]; then
    echo -e "${RED}ERROR: Could not extract a track ID from tracks print-all${NC}" >&2
    exit 1
fi
echo "  Using track ID: $TRACK_ID"

run_cmd "${ROSE_CMD[@]}" tracks print "$TRACK_ID"

# --------------------------------------------------------------------------
# 5. Artists / Genres / Labels / Descriptors
# --------------------------------------------------------------------------

run_cmd "${ROSE_CMD[@]}" artists print-all
run_cmd "${ROSE_CMD[@]}" genres print-all
run_cmd "${ROSE_CMD[@]}" labels print-all
run_cmd "${ROSE_CMD[@]}" descriptors print-all

# --------------------------------------------------------------------------
# 6. Collages
# --------------------------------------------------------------------------

run_cmd "${ROSE_CMD[@]}" collages print-all
run_cmd "${ROSE_CMD[@]}" collages create "Smoke Test Collage"
run_cmd "${ROSE_CMD[@]}" collages add-release "Smoke Test Collage" "$RELEASE_ID"
run_cmd "${ROSE_CMD[@]}" collages print "Smoke Test Collage"
run_cmd "${ROSE_CMD[@]}" collages rename "Smoke Test Collage" "Renamed Collage"
run_cmd "${ROSE_CMD[@]}" collages remove-release "Renamed Collage" "$RELEASE_ID"
run_cmd "${ROSE_CMD[@]}" collages delete "Renamed Collage"

# --------------------------------------------------------------------------
# 7. Playlists
# --------------------------------------------------------------------------

run_cmd "${ROSE_CMD[@]}" playlists print-all
run_cmd "${ROSE_CMD[@]}" playlists create "Smoke Test Playlist"
run_cmd "${ROSE_CMD[@]}" playlists add-track "Smoke Test Playlist" "$TRACK_ID"
run_cmd "${ROSE_CMD[@]}" playlists print "Smoke Test Playlist"
run_cmd "${ROSE_CMD[@]}" playlists rename "Smoke Test Playlist" "Renamed Playlist"
run_cmd "${ROSE_CMD[@]}" playlists remove-track "Renamed Playlist" "$TRACK_ID"
run_cmd "${ROSE_CMD[@]}" playlists delete "Renamed Playlist"

# --------------------------------------------------------------------------
# 8. Rules (dry-run only)
# --------------------------------------------------------------------------

# Use a simple matcher + action with --dry-run --yes to avoid interactive prompt.
# This tests that the rules engine parses and executes without error.
run_cmd "${ROSE_CMD[@]}" rules run-stored --dry-run --yes

# --------------------------------------------------------------------------
# 9. Cache update --force (re-read everything)
# --------------------------------------------------------------------------

run_cmd "${ROSE_CMD[@]}" cache update --force

# --------------------------------------------------------------------------
# 10. Config preview-templates
# --------------------------------------------------------------------------

run_cmd "${ROSE_CMD[@]}" config preview-templates

# --------------------------------------------------------------------------
# 11. VFS tests (optional, requires FUSE)
# --------------------------------------------------------------------------

if [[ "${ROSE_TEST_FUSE:-0}" == "1" ]]; then
    echo ""
    echo "-- VFS tests (ROSE_TEST_FUSE=1) --"
    run_cmd "${ROSE_CMD[@]}" fs mount
    sleep 2
    ls -la "$TMPDIR/mnt/" || true
    run_cmd "${ROSE_CMD[@]}" fs unmount
else
    echo ""
    echo "-- Skipping VFS tests (set ROSE_TEST_FUSE=1 to enable) --"
fi

# --------------------------------------------------------------------------
# Done
# --------------------------------------------------------------------------

echo ""
echo "========================================"
echo -e "${GREEN}${BOLD} SMOKE TEST PASSED${NC}"
echo "========================================"
