#!/usr/bin/env bash
set -euo pipefail

fail() {
  echo "AR-11 Release Set asset materialization failed: $*" >&2
  exit 1
}

usage() {
  cat >&2 <<'EOF'
usage:
  release-set-assets-ar11.sh materialize <current-v3|known-good-v2-v3> <release-set-id> <asset-root> <release-root> [verified-target-release-root]
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

# Only exact, self-consistent immutable runtime identities can share bytes.
# Empty output means distinct/historical/incomplete evidence: download anew.
runtime_share_identity() {
  local document="$1"
  local runtime_path="$2"
  jq -ce --arg path "$runtime_path" '
    . as $release
    | .components.runtime_bundle as $runtime
    | [.artifact_inventory[]? | select(.path == $path)] as $assets
    | select(($assets | length) == 1)
    | select($release.schema_version == 3
        and $assets[0].kind == "component"
        and $runtime.component_id == "runtime_bundle"
        and ($runtime.release_id | type == "string" and test("^runtime-bundle-v3-sha256-[0-9a-f]{64}$"))
        and $runtime.artifact_path == $path
        and ($runtime.artifact_sha256 | type == "string" and test("^[0-9a-f]{64}$"))
        and ($runtime.artifact_size_bytes | type == "number" and . > 0 and floor == .)
        and ($runtime.component_manifest_sha256 | type == "string" and test("^[0-9a-f]{64}$"))
        and $assets[0].sha256 == $runtime.artifact_sha256
        and $assets[0].size_bytes == $runtime.artifact_size_bytes)
    | {release_id:$runtime.release_id, artifact_path:$runtime.artifact_path,
       artifact_sha256:$runtime.artifact_sha256,
       artifact_size_bytes:$runtime.artifact_size_bytes,
       component_manifest_sha256:$runtime.component_manifest_sha256,
       inventory_sha256:$assets[0].sha256, inventory_size_bytes:$assets[0].size_bytes}
  ' "$document" 2>/dev/null || true
}

require_v3_core_inventory() {
  local document="$1"
  local runtime_path="$2"
  local path
  for path in \
    components/control-plane.tar \
    components/secret-resolver.tar \
    "$runtime_path" \
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
  local verified_target_root="${5:-}"
  if [ -n "$verified_target_root" ] && [ "$mode" != known-good-v2-v3 ]; then
    fail "runtime reuse input is restricted to independently verified known-good materialization"
  fi

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

  local runtime_release_id=""
  local runtime_external=0
  local runtime_path=components/runtime-bundle.tar.gz
  local runtime_asset=runtime-bundle.tar.gz
  if [ "$schema" = 3 ]; then
    runtime_release_id="$(jq -r '.components.runtime_bundle.release_id // ""' "$document")"
    runtime_path="$(jq -er '.components.runtime_bundle.artifact_path | strings' "$document")"
    case "$runtime_path" in
      components/runtime-bundle.tar.gz) ;;
      components/runtime-bundle.tar)
        test "$mode" = known-good-v2-v3 || fail "raw TAR runtime forbidden in current target"
        runtime_asset=runtime-bundle.tar
        ;;
      *) fail "unsupported immutable runtime component artifact path: $runtime_path" ;;
    esac
    if [[ "$runtime_release_id" =~ ^runtime-bundle-v3-sha256-[0-9a-f]{64}$ ]]; then
      runtime_external=1
    fi
  fi

  local common=(control-plane.tar secret-resolver.tar profile-bridge.zip)
  if [ "$runtime_external" -eq 0 ]; then
    common+=("$runtime_asset")
  fi
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
      require_v3_core_inventory "$document" "$runtime_path"

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
        if [ "$runtime_external" -eq 1 ]; then
          flat_expected_count=8
        else
          flat_expected_count=9
        fi
        # Historical deployed aggregates have no installer. New publications include the
        # signature and ordinary wizard together; a partial pair or extra asset fails closed.
        local has_signature=0 has_installer=0
        test ! -e "$asset_root/windows-delivery-signature.json" || has_signature=1
        test ! -e "$asset_root/ProfileBridgeSetup.exe" || has_installer=1
        test "$has_signature" -eq "$has_installer" || fail "Windows installer publication is partial"
        if [ "$has_installer" -eq 1 ]; then
          expected+=(windows-delivery-signature.json ProfileBridgeSetup.exe)
          flat_expected_count=$((flat_expected_count + 2))
        fi
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
  test ! -L "$release_root" && test ! -L "$release_root/components" \
    || fail "materialized Release Set directories cannot be symlinks"
  test "$(find "$release_root" -type f | wc -l)" -eq 0 || fail "release root must be empty before materialization"
  test ! -e "$release_root/$runtime_path" && test ! -L "$release_root/$runtime_path" \
    || fail "runtime destination already exists before immutable materialization"
  cp "$document" "$release_root/release-set.json"

  if [ "$profile" != historical-v2-core ] && inventory_has "$document" capability-policy-v1.json; then
    cp "$asset_root/capability-policy-v1.json" "$release_root/capability-policy-v1.json"
  fi
  cp "$asset_root/control-plane.tar" "$release_root/components/control-plane.tar"
  cp "$asset_root/secret-resolver.tar" "$release_root/components/secret-resolver.tar"
  if [ "$runtime_external" -eq 1 ]; then
    local reused_runtime=0
    if [ -n "$verified_target_root" ] && [ "$runtime_path" = components/runtime-bundle.tar.gz ]; then
      local known_identity target_identity verified_archive expected_sha expected_size
      known_identity="$(runtime_share_identity "$document" "$runtime_path")"
      if [ -n "$known_identity" ] && [ -d "$verified_target_root" ] && [ ! -L "$verified_target_root" ] \
         && [ -d "$verified_target_root/components" ] && [ ! -L "$verified_target_root/components" ] \
         && [ -f "$verified_target_root/release-set.json" ] && [ ! -L "$verified_target_root/release-set.json" ]; then
        target_identity="$(runtime_share_identity "$verified_target_root/release-set.json" "$runtime_path")"
        if [ -n "$target_identity" ] && [ "$known_identity" = "$target_identity" ]; then
          verified_archive="$verified_target_root/$runtime_path"
          require_regular_file "$verified_archive"
          expected_sha="$(jq -r '.artifact_sha256' <<<"$known_identity")"
          expected_size="$(jq -r '.artifact_size_bytes' <<<"$known_identity")"
          test "$(stat -c %s "$verified_archive")" = "$expected_size" \
            || fail "previously verified runtime size changed before reuse"
          test "$(sha256sum "$verified_archive" | cut -d' ' -f1)" = "$expected_sha" \
            || fail "previously verified runtime SHA-256 changed before reuse"
          if ln "$verified_archive" "$release_root/$runtime_path"; then
            reused_runtime=1
            echo "AR11 reused exact independently verified runtime $runtime_release_id (no second download/copy)"
          else
            test ! -e "$release_root/$runtime_path" && test ! -L "$release_root/$runtime_path" \
              || fail "runtime hardlink failed with partial destination"
            echo "AR11 hardlink unavailable; hydrate known-good immutable runtime independently"
          fi
        fi
      fi
    fi
    if [ "$reused_runtime" -eq 0 ]; then
      command -v gh >/dev/null 2>&1 || fail "GitHub CLI is required to hydrate exact runtime component"
      local runtime_download
      runtime_download="$(mktemp -d)"
      if ! gh release download "$runtime_release_id" --repo "${GITHUB_REPOSITORY:-iamaman11/part-crm-emai-profile}" \
           --dir "$runtime_download" --pattern "$runtime_asset"; then
        rm -rf "$runtime_download"
        fail "exact runtime component download failed: $runtime_release_id"
      fi
      require_regular_file "$runtime_download/$runtime_asset"
      cp "$runtime_download/$runtime_asset" "$release_root/$runtime_path"
      rm -rf "$runtime_download"
    fi
  else
    cp "$asset_root/$runtime_asset" "$release_root/$runtime_path"
  fi
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
    : > "$dir/runtime-bundle.tar.gz"
    : > "$dir/profile-bridge.zip"

    if [ "$schema" = 3 ]; then
      local inventory='[
        {"path":"components/control-plane.tar","sha256":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"},
        {"path":"components/secret-resolver.tar","sha256":"dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"},
        {"path":"components/runtime-bundle.tar.gz","sha256":"eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"},
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
        '{schema_version:$schema,release_set_id:$id,source:{commit_sha:$source},components:{runtime_bundle:{release_id:("runtime-bundle-v2-sha256-" + ("8" * 64)),artifact_path:"components/runtime-bundle.tar.gz"}},artifact_inventory:$inventory}' \
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

  cp -R "$root/v3-current" "$root/v3-installer"
  printf 'signed-delivery-fixture' > "$root/v3-installer/windows-delivery-signature.json"
  printf 'installer-fixture' > "$root/v3-installer/ProfileBridgeSetup.exe"
  materialize current-v3 "$v3" "$root/v3-installer" "$root/v3-installer-root"
  rm "$root/v3-installer/windows-delivery-signature.json"
  if ( materialize current-v3 "$v3" "$root/v3-installer" "$root/should-fail-partial-installer" ) >/dev/null 2>&1; then
    fail "partial installer publication unexpectedly passed"
  fi

  local external_runtime="runtime-bundle-v3-sha256-$(printf '8%.0s' {1..64})"
  cp -R "$root/v3-current" "$root/v3-external"
  jq --arg runtime "$external_runtime" '.components.runtime_bundle.release_id = $runtime'     "$root/v3-external/release-set.json" > "$root/v3-external/release-set.tmp"
  mv "$root/v3-external/release-set.tmp" "$root/v3-external/release-set.json"
  rm "$root/v3-external/runtime-bundle.tar.gz"
  printf 'exact-runtime-component' > "$root/runtime-component.tar"
  local runtime_sha runtime_size
  runtime_sha="$(sha256sum "$root/runtime-component.tar" | cut -d' ' -f1)"
  runtime_size="$(stat -c %s "$root/runtime-component.tar")"
  jq --arg digest "$runtime_sha" --argjson bytes "$runtime_size" '
    .components.runtime_bundle += {
      component_id:"runtime_bundle", artifact_sha256:$digest,
      artifact_size_bytes:$bytes, component_manifest_sha256:("a" * 64)}
    | (.artifact_inventory[] | select(.path == "components/runtime-bundle.tar.gz"))
      += {sha256:$digest,size_bytes:$bytes,kind:"component"}
  ' "$root/v3-external/release-set.json" > "$root/v3-external/release-set.tmp"
  mv "$root/v3-external/release-set.tmp" "$root/v3-external/release-set.json"
  mkdir -p "$root/fake-bin"
  cat > "$root/fake-bin/gh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
test "$1" = release
test "$2" = download
shift 2
destination=""
pattern=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --dir)
      destination="$2"
      shift 2
      ;;
    --pattern)
      pattern="$2"
      shift 2
      ;;
    --repo)
      shift 2
      ;;
    *)
      shift
      ;;
  esac
done
test -n "$destination"
test "${AR11_RUNTIME_REUSE_GH_FORBID:-0}" != 1
[[ "$pattern" = runtime-bundle.tar.gz || "$pattern" = runtime-bundle.tar ]]
mkdir -p "$destination"
cp "$AR11_RUNTIME_COMPONENT_FIXTURE" "$destination/$pattern"
EOF
  chmod +x "$root/fake-bin/gh"
  PATH="$root/fake-bin:$PATH"     AR11_RUNTIME_COMPONENT_FIXTURE="$root/runtime-component.tar"     materialize current-v3 "$v3" "$root/v3-external" "$root/v3-external-root"
  cmp --silent "$root/runtime-component.tar" "$root/v3-external-root/components/runtime-bundle.tar.gz"

  # A distinct accepted known-good Release Set can share the immutable bytes
  # only after the target itself has passed native verification in the caller.
  local known_good_v3="release-set-v3-sha256-$(printf '4%.0s' {1..64})"
  cp -R "$root/v3-external" "$root/v3-share"
  jq --arg id "$known_good_v3" --arg source "$(printf '2%.0s' {1..40})" \
    '.release_set_id = $id | .source.commit_sha = $source' \
    "$root/v3-share/release-set.json" > "$root/v3-share/release-set.tmp"
  mv "$root/v3-share/release-set.tmp" "$root/v3-share/release-set.json"
  jq --arg id "$known_good_v3" --arg source "$(printf '2%.0s' {1..40})" \
    '.release_set_id = $id | .source_commit_sha = $source' \
    "$root/v3-share/windows-delivery-manifest.json" > "$root/v3-share/delivery.tmp"
  mv "$root/v3-share/delivery.tmp" "$root/v3-share/windows-delivery-manifest.json"
  PATH="$root/fake-bin:$PATH" AR11_RUNTIME_REUSE_GH_FORBID=1 \
    materialize known-good-v2-v3 "$known_good_v3" "$root/v3-share" \
      "$root/v3-share-root" "$root/v3-external-root"
  test "$root/v3-share-root/components/runtime-bundle.tar.gz" -ef \
    "$root/v3-external-root/components/runtime-bundle.tar.gz" \
    || fail "identical independently verified runtime was not hardlinked"

  # No optional proof must keep the ordinary physical download path.
  PATH="$root/fake-bin:$PATH" AR11_RUNTIME_COMPONENT_FIXTURE="$root/runtime-component.tar" \
    materialize known-good-v2-v3 "$known_good_v3" "$root/v3-share" "$root/v3-without-proof"
  test ! "$root/v3-without-proof/components/runtime-bundle.tar.gz" -ef \
    "$root/v3-external-root/components/runtime-bundle.tar.gz" \
    || fail "runtime was shared without an independently verified target"

  # Filesystem policy/EXDEV can reject hardlinks even with valid identities.
  # That is not a license to skip known-good hydration: use the real download.
  mkdir -p "$root/fake-link-bin"
  cat > "$root/fake-link-bin/ln" <<'EOF'
#!/usr/bin/env bash
exit 1
EOF
  chmod +x "$root/fake-link-bin/ln"
  PATH="$root/fake-link-bin:$root/fake-bin:$PATH" \
    AR11_RUNTIME_COMPONENT_FIXTURE="$root/runtime-component.tar" \
    materialize known-good-v2-v3 "$known_good_v3" "$root/v3-share" \
      "$root/v3-link-unavailable" "$root/v3-external-root"
  cmp --silent "$root/runtime-component.tar" "$root/v3-link-unavailable/components/runtime-bundle.tar.gz"
  test ! "$root/v3-link-unavailable/components/runtime-bundle.tar.gz" -ef \
    "$root/v3-external-root/components/runtime-bundle.tar.gz" \
    || fail "link failure did not restore the independent download"

  local field mutation fixture
  for field in release_id artifact_path artifact_sha256 artifact_size_bytes component_manifest_sha256 inventory_digest inventory_size inventory_kind duplicate_inventory; do
    fixture="$root/mismatch-$field"
    cp -R "$root/v3-share" "$fixture"
    case "$field" in
      release_id) mutation='.components.runtime_bundle.release_id = ("runtime-bundle-v3-sha256-" + ("9" * 64))' ;;
      artifact_path) mutation='.components.runtime_bundle.artifact_path = "components/runtime-bundle.tar" | (.artifact_inventory[] | select(.path == "components/runtime-bundle.tar.gz").path) = "components/runtime-bundle.tar"' ;;
      artifact_sha256) mutation='.components.runtime_bundle.artifact_sha256 = ("9" * 64)' ;;
      artifact_size_bytes) mutation='.components.runtime_bundle.artifact_size_bytes += 1' ;;
      component_manifest_sha256) mutation='.components.runtime_bundle.component_manifest_sha256 = ("9" * 64)' ;;
      inventory_digest) mutation='(.artifact_inventory[] | select(.path == "components/runtime-bundle.tar.gz").sha256) = ("9" * 64)' ;;
      inventory_size) mutation='(.artifact_inventory[] | select(.path == "components/runtime-bundle.tar.gz").size_bytes) += 1' ;;
      inventory_kind) mutation='(.artifact_inventory[] | select(.path == "components/runtime-bundle.tar.gz").kind) = "untrusted"' ;;
      duplicate_inventory) mutation='.artifact_inventory += [(.artifact_inventory[] | select(.path == "components/runtime-bundle.tar.gz"))]' ;;
    esac
    jq "$mutation" "$fixture/release-set.json" > "$fixture/release-set.tmp"
    mv "$fixture/release-set.tmp" "$fixture/release-set.json"
    PATH="$root/fake-bin:$PATH" AR11_RUNTIME_COMPONENT_FIXTURE="$root/runtime-component.tar" \
      materialize known-good-v2-v3 "$known_good_v3" "$fixture" "$root/mismatch-out-$field" "$root/v3-external-root"
    local materialized_runtime=runtime-bundle.tar.gz
    [ "$field" = artifact_path ] && materialized_runtime=runtime-bundle.tar
    test ! "$root/mismatch-out-$field/components/$materialized_runtime" -ef \
      "$root/v3-external-root/components/runtime-bundle.tar.gz" \
      || fail "mismatched $field unexpectedly shared runtime"
  done

  # A missing required inventory row must fail even before network hydration.
  cp -R "$root/v3-share" "$root/missing-inventory"
  jq '.artifact_inventory |= map(select(.path != "components/runtime-bundle.tar.gz"))' \
    "$root/missing-inventory/release-set.json" > "$root/missing-inventory/release-set.tmp"
  mv "$root/missing-inventory/release-set.tmp" "$root/missing-inventory/release-set.json"
  if ( PATH="$root/fake-bin:$PATH" AR11_RUNTIME_COMPONENT_FIXTURE="$root/runtime-component.tar" \
       materialize known-good-v2-v3 "$known_good_v3" "$root/missing-inventory" \
         "$root/rejected-missing-inventory" "$root/v3-external-root" ) >/dev/null 2>&1; then
    fail "missing immutable runtime inventory unexpectedly passed"
  fi

  for mutation in tampered truncated absent symlink; do
    fixture="$root/target-$mutation"
    cp -R "$root/v3-external-root" "$fixture"
    case "$mutation" in
      tampered) printf 'other-runtime-component' > "$fixture/components/runtime-bundle.tar.gz" ;;
      truncated) : > "$fixture/components/runtime-bundle.tar.gz" ;;
      absent) rm "$fixture/components/runtime-bundle.tar.gz" ;;
      symlink)
        rm "$fixture/components/runtime-bundle.tar.gz"
        ln -s "$root/runtime-component.tar" "$fixture/components/runtime-bundle.tar.gz"
        ;;
    esac
    if [ "$mutation" = tampered ]; then
      test "$(stat -c %s "$fixture/components/runtime-bundle.tar.gz")" = "$runtime_size" \
        || fail "tampering fixture must exercise SHA-256 rather than size"
    fi
    if ( PATH="$root/fake-bin:$PATH" AR11_RUNTIME_REUSE_GH_FORBID=1 \
         materialize known-good-v2-v3 "$known_good_v3" "$root/v3-share" \
           "$root/rejected-$mutation" "$fixture" ) >/dev/null 2>&1; then
      fail "tampered/invalid previously verified runtime unexpectedly reused: $mutation"
    fi
  done

  # A published pre-158 v3 source may be raw TAR for read-only known-good.
  cp -R "$root/v3-external" "$root/v3-old-raw"
  jq '.components.runtime_bundle.artifact_path = "components/runtime-bundle.tar"
      | (.artifact_inventory[] | select(.path == "components/runtime-bundle.tar.gz").path) = "components/runtime-bundle.tar"' \
    "$root/v3-old-raw/release-set.json" > "$root/v3-old-raw/tmp.json"
  mv "$root/v3-old-raw/tmp.json" "$root/v3-old-raw/release-set.json"
  PATH="$root/fake-bin:$PATH" \
    AR11_RUNTIME_COMPONENT_FIXTURE="$root/runtime-component.tar" \
    materialize known-good-v2-v3 "$v3" "$root/v3-old-raw" "$root/v3-old-raw-root"
  cmp --silent "$root/runtime-component.tar" "$root/v3-old-raw-root/components/runtime-bundle.tar"
  if ( PATH="$root/fake-bin:$PATH" \
       AR11_RUNTIME_COMPONENT_FIXTURE="$root/runtime-component.tar" \
       materialize current-v3 "$v3" "$root/v3-old-raw" "$root/should-fail-current-raw" \
     ) >/dev/null 2>&1; then
    fail "raw legacy runtime was admitted as current official browser target"
  fi

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
    { [ "$#" -eq 5 ] || [ "$#" -eq 6 ]; } || usage
    materialize "$2" "$3" "$4" "$5" "${6:-}"
    ;;
  --self-test)
    [ "$#" -eq 1 ] || usage
    self_test
    ;;
  *)
    usage
    ;;
esac
