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

inventory_has() {
  local document="$1"
  local path="$2"
  jq -e --arg path "$path" 'any(.artifact_inventory[]?; .path == $path)' "$document" >/dev/null
}

inventory_sha() {
  local document="$1"
  local path="$2"
  jq -er --arg path "$path" '
    [.artifact_inventory[]? | select(.path == $path)]
    | if length == 1 and (.[0].sha256 | type) == "string" and (.[0].sha256 | test("^[0-9a-f]{64}$"))
      then .[0].sha256
      else error("inventory path missing, ambiguous, or sha256 invalid: " + $path)
      end
  ' "$document"
}

require_v3_core_inventory() {
  local document="$1"
  local path
  for path in \
    components/control-plane.tar \
    components/secret-resolver.tar \
    components/runtime-bundle.tar \
    components/profile-bridge.zip; do
    inventory_has "$document" "$path" || fail "schema v3 inventory missing required core artifact: $path"
  done
}

verify_windows_delivery_manifest() {
  local document="$1"
  local manifest="$2"
  local release_id="$3"
  local source_sha sbom_sha provenance_sha
  require_regular_file "$manifest"
  source_sha="$(jq -er '.source.commit_sha | select(type == "string" and test("^[0-9a-f]{40}$"))' "$document")"
  sbom_sha="$(inventory_sha "$document" 'windows/windows-sbom-v1.json')"
  provenance_sha="$(inventory_sha "$document" 'windows/windows-provenance-v1.json')"
  jq -e \
    --arg release_id "$release_id" \
    --arg source_sha "$source_sha" \
    --arg sbom_sha "$sbom_sha" \
    --arg provenance_sha "$provenance_sha" \
    '.schema_version == 1
      and .kind == "WINDOWS_PROFILE_BRIDGE_DELIVERY"
      and .release_set_id == $release_id
      and .source_commit_sha == $source_sha
      and .evidence.sbom_sha256 == $sbom_sha
      and .evidence.provenance_sha256 == $provenance_sha
      and (.sequence | type) == "number"
      and .sequence > 0' \
    "$manifest" >/dev/null \
    || fail "Windows delivery manifest does not bind exact Release Set evidence"
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

  local document="$asset_root/release-set.json"
  local document_id schema profile flat_expected_count materialized_expected_count
  document_id="$(jq -er '.release_set_id' "$document")"
  schema="$(jq -er '.schema_version' "$document")"
  test "$document_id" = "$release_id" || fail "release-set id mismatch: expected=$release_id observed=$document_id"

  local common=(control-plane.tar secret-resolver.tar runtime-bundle.tar profile-bridge.zip)
  local expected=(release-set.json)

  case "$schema" in
    2)
      test "$mode" = known-good-v2-v3 || fail "current target must use Release Set schema v3"
      [[ "$release_id" =~ ^release-set-v2-sha256-[0-9a-f]{64}$ ]] || fail "historical v2 document/id mismatch"
      profile=historical-v2-core
      expected+=("${common[@]}")
      flat_expected_count=5
      materialized_expected_count=5
      ;;
    3)
      [[ "$release_id" =~ ^release-set-v3-sha256-[0-9a-f]{64}$ ]] || fail "v3 document/id mismatch"
      require_v3_core_inventory "$document"

      local has_capability=0 has_sbom=0 has_provenance=0
      inventory_has "$document" capability-policy-v1.json && has_capability=1
      inventory_has "$document" windows/windows-sbom-v1.json && has_sbom=1
      inventory_has "$document" windows/windows-provenance-v1.json && has_provenance=1

      test "$has_sbom" -eq "$has_provenance" || fail "schema v3 Windows evidence inventory is partial"
      if [ "$has_sbom" -eq 1 ] && [ "$has_capability" -ne 1 ]; then
        fail "schema v3 Windows evidence profile requires capability policy"
      fi

      if [ "$has_sbom" -eq 1 ]; then
        profile=current-v3-windows-delivery
        expected+=(capability-policy-v1.json "${common[@]}" windows-sbom-v1.json windows-provenance-v1.json windows-delivery-manifest.json)
        flat_expected_count=9
        materialized_expected_count=8
      elif [ "$has_capability" -eq 1 ]; then
        profile=historical-v3-capability
        expected+=(capability-policy-v1.json "${common[@]}")
        flat_expected_count=6
        materialized_expected_count=6
      else
        profile=historical-v3-core
        expected+=("${common[@]}")
        flat_expected_count=5
        materialized_expected_count=5
      fi

      if [ "$mode" = current-v3 ]; then
        test "$profile" = current-v3-windows-delivery \
          || fail "current schema v3 target must use the canonical Windows delivery publication profile"
      elif [ "$mode" != known-good-v2-v3 ]; then
        fail "unsupported materialization mode/schema: $mode/$schema"
      fi
      ;;
    *)
      fail "unsupported materialization mode/schema: $mode/$schema"
      ;;
  esac

  local name
  for name in "${expected[@]}"; do
    require_regular_file "$asset_root/$name"
  done
  test "$(find "$asset_root" -maxdepth 1 -type f | wc -l)" -eq "$flat_expected_count" \
    || fail "flat GitHub Release asset set is not exact for publication profile $profile"

  mkdir -p "$release_root/components"
  test "$(find "$release_root" -type f | wc -l)" -eq 0 || fail "release root must be empty before materialization"
  cp "$document" "$release_root/release-set.json"

  if [ "$profile" != historical-v2-core ] && inventory_has "$document" capability-policy-v1.json; then
    cp "$asset_root/capability-policy-v1.json" "$release_root/capability-policy-v1.json"
  fi
  cp "$asset_root/control-plane.tar" "$release_root/components/control-plane.tar"
  cp "$asset_root/secret-resolver.tar" "$release_root/components/secret-resolver.tar"
  cp "$asset_root/runtime-bundle.tar" "$release_root/components/runtime-bundle.tar"
  cp "$asset_root/profile-bridge.zip" "$release_root/components/profile-bridge.zip"

  if [ "$profile" = current-v3-windows-delivery ]; then
    mkdir -p "$release_root/windows"
    cp "$asset_root/windows-sbom-v1.json" "$release_root/windows/windows-sbom-v1.json"
    cp "$asset_root/windows-provenance-v1.json" "$release_root/windows/windows-provenance-v1.json"
    verify_windows_delivery_manifest "$document" "$asset_root/windows-delivery-manifest.json" "$release_id"
  fi

  test "$(find "$release_root" -type f | wc -l)" -eq "$materialized_expected_count" \
    || fail "materialized Release Set root is incomplete for publication profile $profile"
}

self_test() {
  local root
  root="$(mktemp -d)"
  trap 'rm -rf "$root"' RETURN

  local source_sha="$(printf '1%.0s' {1..40})"
  local sbom_sha="$(printf 'a%.0s' {1..64})"
  local provenance_sha="$(printf 'b%.0s' {1..64})"

  make_assets() {
    local schema="$1"
    local id="$2"
    local dir="$3"
    local profile="$4"
    mkdir -p "$dir"
    : > "$dir/control-plane.tar"
    : > "$dir/secret-resolver.tar"
    : > "$dir/runtime-bundle.tar"
    : > "$dir/profile-bridge.zip"

    if [ "$schema" = 3 ]; then
      local inventory='[
        {"path":"components/control-plane.tar","sha256":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"},
        {"path":"components/secret-resolver.tar","sha256":"dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"},
        {"path":"components/runtime-bundle.tar","sha256":"eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"},
        {"path":"components/profile-bridge.zip","sha256":"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"}
      ]'
      if [ "$profile" = capability ] || [ "$profile" = windows ]; then
        inventory="$(jq -c '. + [{"path":"capability-policy-v1.json","sha256":"9999999999999999999999999999999999999999999999999999999999999999"}]' <<<"$inventory")"
        : > "$dir/capability-policy-v1.json"
      fi
      if [ "$profile" = windows ]; then
        inventory="$(jq -c --arg sbom "$sbom_sha" --arg provenance "$provenance_sha" '. + [{"path":"windows/windows-sbom-v1.json","sha256":$sbom},{"path":"windows/windows-provenance-v1.json","sha256":$provenance}]' <<<"$inventory")"
        : > "$dir/windows-sbom-v1.json"
        : > "$dir/windows-provenance-v1.json"
        jq -n \
          --arg id "$id" --arg source "$source_sha" --arg sbom "$sbom_sha" --arg provenance "$provenance_sha" \
          '{schema_version:1,kind:"WINDOWS_PROFILE_BRIDGE_DELIVERY",release_set_id:$id,source_commit_sha:$source,evidence:{sbom_sha256:$sbom,provenance_sha256:$provenance},sequence:1}' \
          > "$dir/windows-delivery-manifest.json"
      fi
      jq -n --argjson schema "$schema" --arg id "$id" --arg source "$source_sha" --argjson inventory "$inventory" \
        '{schema_version:$schema,release_set_id:$id,source:{commit_sha:$source},artifact_inventory:$inventory}' \
        > "$dir/release-set.json"
    else
      printf '{"schema_version":%s,"release_set_id":"%s"}\n' "$schema" "$id" > "$dir/release-set.json"
    fi
  }

  local v2="release-set-v2-sha256-$(printf '2%.0s' {1..64})"
  local v3="release-set-v3-sha256-$(printf '3%.0s' {1..64})"

  make_assets 3 "$v3" "$root/v3-current" windows
  materialize current-v3 "$v3" "$root/v3-current" "$root/v3-current-root"
  test -f "$root/v3-current-root/capability-policy-v1.json"
  test -f "$root/v3-current-root/windows/windows-sbom-v1.json"
  test -f "$root/v3-current-root/windows/windows-provenance-v1.json"
  test ! -e "$root/v3-current-root/windows-delivery-manifest.json"

  make_assets 3 "$v3" "$root/v3-capability" capability
  materialize known-good-v2-v3 "$v3" "$root/v3-capability" "$root/v3-capability-root"

  make_assets 3 "$v3" "$root/v3-core" core
  materialize known-good-v2-v3 "$v3" "$root/v3-core" "$root/v3-core-root"

  make_assets 2 "$v2" "$root/v2" core
  materialize known-good-v2-v3 "$v2" "$root/v2" "$root/v2-root"

  if ( materialize current-v3 "$v3" "$root/v3-capability" "$root/should-fail-stale-current-profile" ) >/dev/null 2>&1; then
    fail "historical capability-only v3 unexpectedly accepted as current target"
  fi
  if ( materialize current-v3 "$v3" "$root/v3-core" "$root/should-fail-core-current-profile" ) >/dev/null 2>&1; then
    fail "historical core-only v3 unexpectedly accepted as current target"
  fi
  if ( materialize current-v3 "$v2" "$root/v2" "$root/should-fail-v2-target" ) >/dev/null 2>&1; then
    fail "historical v2 fixture unexpectedly accepted as current target"
  fi

  cp -a "$root/v3-current" "$root/v3-extra"
  : > "$root/v3-extra/unexpected.bin"
  if ( materialize current-v3 "$v3" "$root/v3-extra" "$root/should-fail-extra" ) >/dev/null 2>&1; then
    fail "unexpected GitHub Release asset fixture unexpectedly passed"
  fi

  cp -a "$root/v3-current" "$root/v3-partial"
  jq '(.artifact_inventory) |= map(select(.path != "windows/windows-provenance-v1.json"))' "$root/v3-partial/release-set.json" > "$root/v3-partial/release-set.tmp"
  mv "$root/v3-partial/release-set.tmp" "$root/v3-partial/release-set.json"
  rm "$root/v3-partial/windows-provenance-v1.json" "$root/v3-partial/windows-delivery-manifest.json"
  if ( materialize known-good-v2-v3 "$v3" "$root/v3-partial" "$root/should-fail-partial" ) >/dev/null 2>&1; then
    fail "partial Windows evidence publication profile unexpectedly passed"
  fi

  cp -a "$root/v3-current" "$root/v3-bad-delivery"
  jq '.evidence.sbom_sha256 = "0000000000000000000000000000000000000000000000000000000000000000"' "$root/v3-bad-delivery/windows-delivery-manifest.json" > "$root/v3-bad-delivery/windows-delivery.tmp"
  mv "$root/v3-bad-delivery/windows-delivery.tmp" "$root/v3-bad-delivery/windows-delivery-manifest.json"
  if ( materialize current-v3 "$v3" "$root/v3-bad-delivery" "$root/should-fail-bad-delivery" ) >/dev/null 2>&1; then
    fail "misbound Windows delivery manifest unexpectedly passed"
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