#!/usr/bin/env bash
set -euo pipefail

fail() {
  echo "AR-11 Release Set asset materialization failed: $*" >&2
  exit 1
}

usage() {
  cat >&2 <<'EOF'
usage:
  release-set-assets-ar11.sh materialize <current-v3|known-good-v2-v3> <release-set-id> <asset-root> <release-root>
  release-set-assets-ar11.sh --self-test
EOF
  exit 2
}

require_regular_file() {
  local path="$1"
  test -f "$path" || fail "missing regular asset: $path"
  test ! -L "$path" || fail "symlink asset forbidden: $path"
}

materialize() {
  local mode="$1"
  local release_id="$2"
  local asset_root="$3"
  local release_root="$4"

  test -d "$asset_root" || fail "asset root unavailable: $asset_root"
  if find "$asset_root" -mindepth 1 -type l -print -quit | grep -q .; then
    fail "asset root contains symlink"
  fi
  require_regular_file "$asset_root/release-set.json"

  local document_id schema expected_count
  document_id="$(jq -er '.release_set_id' "$asset_root/release-set.json")"
  schema="$(jq -er '.schema_version' "$asset_root/release-set.json")"
  test "$document_id" = "$release_id" || fail "release-set id mismatch: expected=$release_id observed=$document_id"

  local common=(control-plane.tar secret-resolver.tar runtime-bundle.tar profile-bridge.zip)
  local expected=(release-set.json)
  case "$mode:$schema" in
    current-v3:3)
      [[ "$release_id" =~ ^release-set-v3-sha256-[0-9a-f]{64}$ ]] || fail "current target must be canonical v3"
      expected+=(capability-policy-v1.json "${common[@]}")
      expected_count=6
      ;;
    current-v3:*)
      fail "current target must use Release Set schema v3"
      ;;
    known-good-v2-v3:2)
      [[ "$release_id" =~ ^release-set-v2-sha256-[0-9a-f]{64}$ ]] || fail "historical v2 document/id mismatch"
      expected+=("${common[@]}")
      expected_count=5
      ;;
    known-good-v2-v3:3)
      [[ "$release_id" =~ ^release-set-v3-sha256-[0-9a-f]{64}$ ]] || fail "known-good v3 document/id mismatch"
      expected+=(capability-policy-v1.json "${common[@]}")
      expected_count=6
      ;;
    *)
      fail "unsupported materialization mode/schema: $mode/$schema"
      ;;
  esac

  for name in "${expected[@]}"; do
    require_regular_file "$asset_root/$name"
  done
  test "$(find "$asset_root" -maxdepth 1 -type f | wc -l)" -eq "$expected_count" \
    || fail "flat GitHub Release asset set is not exact for schema v$schema"

  mkdir -p "$release_root/components"
  test "$(find "$release_root" -type f | wc -l)" -eq 0 || fail "release root must be empty before materialization"
  cp "$asset_root/release-set.json" "$release_root/release-set.json"
  if [ "$schema" = 3 ]; then
    cp "$asset_root/capability-policy-v1.json" "$release_root/capability-policy-v1.json"
  fi
  cp "$asset_root/control-plane.tar" "$release_root/components/control-plane.tar"
  cp "$asset_root/secret-resolver.tar" "$release_root/components/secret-resolver.tar"
  cp "$asset_root/runtime-bundle.tar" "$release_root/components/runtime-bundle.tar"
  cp "$asset_root/profile-bridge.zip" "$release_root/components/profile-bridge.zip"
  test "$(find "$release_root" -type f | wc -l)" -eq "$expected_count" \
    || fail "materialized Release Set root is incomplete"
}

self_test() {
  local root
  root="$(mktemp -d)"
  trap 'rm -rf "$root"' RETURN

  make_assets() {
    local schema="$1"
    local id="$2"
    local dir="$3"
    mkdir -p "$dir"
    printf '{"schema_version":%s,"release_set_id":"%s"}\n' "$schema" "$id" > "$dir/release-set.json"
    : > "$dir/control-plane.tar"
    : > "$dir/secret-resolver.tar"
    : > "$dir/runtime-bundle.tar"
    : > "$dir/profile-bridge.zip"
    if [ "$schema" = 3 ]; then
      : > "$dir/capability-policy-v1.json"
    fi
  }

  local v2="release-set-v2-sha256-$(printf '2%.0s' {1..64})"
  local v3="release-set-v3-sha256-$(printf '3%.0s' {1..64})"
  make_assets 3 "$v3" "$root/v3"
  materialize current-v3 "$v3" "$root/v3" "$root/v3-root"
  test -f "$root/v3-root/capability-policy-v1.json"

  make_assets 2 "$v2" "$root/v2"
  materialize known-good-v2-v3 "$v2" "$root/v2" "$root/v2-root"
  test ! -e "$root/v2-root/capability-policy-v1.json"

  materialize known-good-v2-v3 "$v3" "$root/v3" "$root/v3-known-good-root"

  cp -a "$root/v3" "$root/v3-missing"
  rm "$root/v3-missing/capability-policy-v1.json"
  if ( materialize current-v3 "$v3" "$root/v3-missing" "$root/should-fail-missing" ) >/dev/null 2>&1; then
    fail "v3 missing capability-policy fixture unexpectedly passed"
  fi

  if ( materialize current-v3 "$v2" "$root/v2" "$root/should-fail-v2-target" ) >/dev/null 2>&1; then
    fail "historical v2 fixture unexpectedly accepted as current target"
  fi

  cp -a "$root/v3" "$root/v3-extra"
  : > "$root/v3-extra/unexpected.bin"
  if ( materialize current-v3 "$v3" "$root/v3-extra" "$root/should-fail-extra" ) >/dev/null 2>&1; then
    fail "unexpected GitHub Release asset fixture unexpectedly passed"
  fi

  echo "AR-11 Release Set asset materialization self-test passed."
}

case "${1:-}" in
  materialize)
    [ "$#" -eq 5 ] || usage
    materialize "$2" "$3" "$4" "$5"
    ;;
  --self-test)
    [ "$#" -eq 1 ] || usage
    self_test
    ;;
  *)
    usage
    ;;
esac
