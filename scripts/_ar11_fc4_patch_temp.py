from pathlib import Path

WORKFLOW = Path('.github/workflows/release-set-promotion.yml')
VALIDATOR = Path('.github/scripts/release-operational-ar11.mjs')

workflow = WORKFLOW.read_text()
start = workflow.index('  mutate:\n')
end = workflow.index('  post-verify:\n', start)
mutate = r'''  mutate:
    name: Execute exact same bits after READY
    needs: [resolve-verify, observe-preflight]
    if: needs.observe-preflight.outputs.decision == 'PLAN'
    runs-on: ubuntu-latest
    environment: staging
    permissions:
      contents: read
      deployments: write
    env:
      GH_TOKEN: ${{ github.token }}
      RELEASE_SET_ID: ${{ inputs.release_set_id }}
      EXPECTED_CURRENT: ${{ inputs.expected_current_release_set_id }}
    steps:
      - name: Checkout exact current policy
        uses: actions/checkout@f548e57e544e1ff5a4c46bf1e1b8685f8e4a348a
        with:
          ref: ${{ needs.resolve-verify.outputs.main_sha }}
          fetch-depth: 0
          persist-credentials: false

      - name: Checkout exact target source before mutation verification
        uses: actions/checkout@f548e57e544e1ff5a4c46bf1e1b8685f8e4a348a
        with:
          ref: ${{ needs.resolve-verify.outputs.source_sha }}
          path: target-source
          fetch-depth: 0
          persist-credentials: false

      - name: Download and bind exact preflight authority before provider use
        uses: actions/download-artifact@70fc10c6e5e1ce46ad2ea6f2b72d43f7d47b13c3
        with:
          name: ar11-preflight-${{ inputs.release_set_id }}
          path: .ar11-preflight

      - name: Re-verify fence and exact immutable Release Set before credentials
        run: |
          set -euo pipefail
          preflight_root="$GITHUB_WORKSPACE/.ar11-preflight"
          fence="$preflight_root/mutation-fence.json"
          preflight="$preflight_root/promotion-preflight.json"
          plan="$preflight_root/promotion-plan.json"
          test "$(sha256sum "$fence" | cut -d' ' -f1)" = "${{ needs.observe-preflight.outputs.mutation_fence_sha256 }}"
          test "$(jq -r '.release_set_id' "$fence")" = "$RELEASE_SET_ID"
          test "$(jq -r '.expected_current' "$fence")" = "$EXPECTED_CURRENT"
          test "$(jq -r '.promotion_id' "$fence")" = "${{ needs.observe-preflight.outputs.promotion_id }}"
          test "$(jq -r '.decision' "$fence")" = PLAN
          test "$(jq -r '.preflight_sha256' "$fence")" = "$(sha256sum "$preflight" | cut -d' ' -f1)"
          test "$(jq -r '.plan_sha256' "$fence")" = "$(sha256sum "$plan" | cut -d' ' -f1)"
          test "$(jq -r '.ready' "$preflight")" = true
          test "$(jq -r '.promotion_id' "$preflight")" = "${{ needs.observe-preflight.outputs.promotion_id }}"

          asset_root="$RUNNER_TEMP/target-assets"
          release_root="$RUNNER_TEMP/mutation-release-set"
          policy_dir="$RUNNER_TEMP/mutation-policy"
          mkdir -p "$asset_root" "$release_root/components" "$policy_dir"
          gh release download "$RELEASE_SET_ID" --repo "$GITHUB_REPOSITORY" --dir "$asset_root"
          for name in release-set.json control-plane.tar secret-resolver.tar runtime-bundle.tar profile-bridge.zip; do
            test -f "$asset_root/$name"
          done
          test "$(find "$asset_root" -maxdepth 1 -type f | wc -l)" -eq 5
          cmp --silent "$asset_root/release-set.json" "$preflight_root/release-set.json"
          cp "$preflight_root/release-set.json" "$policy_dir/release-set.json"
          cp "$preflight_root/accepted-source-evidence.json" "$policy_dir/accepted-source-evidence.json"
          cp "$asset_root/control-plane.tar" "$release_root/components/control-plane.tar"
          cp "$asset_root/secret-resolver.tar" "$release_root/components/secret-resolver.tar"
          cp "$asset_root/runtime-bundle.tar" "$release_root/components/runtime-bundle.tar"
          cp "$asset_root/profile-bridge.zip" "$release_root/components/profile-bridge.zip"
          cargo run --locked --quiet --manifest-path tools/opsctl/Cargo.toml -- \
            --root . release verify --release-set "$policy_dir/release-set.json" \
            --source-root "$GITHUB_WORKSPACE/target-source" --artifact-root "$release_root" \
            | jq -e '.decision == "VALID" and .release_set_schema_version == 2 and .source_accepted == true and .mutation_executed == false' >/dev/null

          control_extract="$RUNNER_TEMP/control-release"
          mkdir -p "$control_extract"
          tar -xf "$asset_root/control-plane.tar" -C "$control_extract"
          manifest="$(find "$control_extract" -type f -name release-manifest.json -print -quit)"
          test -n "$manifest"
          control_root="$(dirname "$manifest")"
          echo "CONTROL_RELEASE_ROOT=$control_root" >> "$GITHUB_ENV"

      - name: Set up pinned Node runtime before deploy credentials
        uses: actions/setup-node@48b55a011bda9f5d6aeb4c2d9c7362e8dae4041e
        with:
          node-version: '24.19.0'
          package-manager-cache: false

      - name: Materialize mutation overlay only after native verification
        env:
          CLOUDFLARE_DEPLOY_MANIFEST_JSON: ${{ secrets.CLOUDFLARE_DEPLOY_MANIFEST_JSON }}
        run: |
          set -euo pipefail
          umask 077
          printf '%s' "$CLOUDFLARE_DEPLOY_MANIFEST_JSON" > "$RUNNER_TEMP/deploy-manifest.json"
          python scripts/release-core-overlay-ar11.py \
            --source-config "$CONTROL_RELEASE_ROOT/deploy/cloudflare/wrangler.jsonc" \
            --release-root "$CONTROL_RELEASE_ROOT" --deploy-manifest "$RUNNER_TEMP/deploy-manifest.json" \
            --environment staging --output "$RUNNER_TEMP/wrangler.staging.json"
          echo "DEPLOY_MANIFEST=$RUNNER_TEMP/deploy-manifest.json" >> "$GITHUB_ENV"
          echo "WRANGLER_CONFIG=$RUNNER_TEMP/wrangler.staging.json" >> "$GITHUB_ENV"

      - name: Prove exact extracted bits can deploy without rebuilding
        run: |
          npx --yes wrangler@4.94.0 deploy --dry-run --config "$WRANGLER_CONFIG" --env staging --experimental-autoconfig=false

      - name: Activate deploy credential after READY and exact-byte verification
        env:
          DEPLOY_TOKEN: ${{ secrets.CLOUDFLARE_API_TOKEN }}
        run: |
          set -euo pipefail
          test -n "$DEPLOY_TOKEN"
          echo "::add-mask::$DEPLOY_TOKEN"
          echo "CLOUDFLARE_API_TOKEN=$DEPLOY_TOKEN" >> "$GITHUB_ENV"

      - name: Re-observe expected-current fence and deploy exact Release Set v2 bits
        run: |
          set -euo pipefail
          account_id="$(jq -r '.control_plane.account_id // .account_id' "$DEPLOY_MANIFEST")"
          worker_name="$(jq -r '.control_plane.worker_name // .worker_name' "$DEPLOY_MANIFEST")"
          test -n "$account_id"
          test -n "$worker_name"
          http_code="$(curl --silent --show-error --output "$RUNNER_TEMP/mutation-current.json" --write-out '%{http_code}' \
            -H "Authorization: Bearer $CLOUDFLARE_API_TOKEN" \
            "https://api.cloudflare.com/client/v4/accounts/$account_id/workers/scripts/$worker_name/deployments")"
          if [ "$http_code" = 200 ]; then
            jq '.result.deployments[0] // {}' "$RUNNER_TEMP/mutation-current.json" > "$RUNNER_TEMP/mutation-current-deployment.json"
          elif [ "$http_code" = 404 ]; then
            printf '{}\n' > "$RUNNER_TEMP/mutation-current-deployment.json"
          else
            cat "$RUNNER_TEMP/mutation-current.json" >&2
            exit 1
          fi
          python scripts/deployment-identity-ar11.py \
            --status "$RUNNER_TEMP/mutation-current-deployment.json" \
            --output "$RUNNER_TEMP/mutation-current-identity.json"
          current_id="$(jq -r '.release_set_id // "NONE"' "$RUNNER_TEMP/mutation-current-identity.json")"
          test "$current_id" = "$EXPECTED_CURRENT"
          npx --yes wrangler@4.94.0 deploy \
            --config "$WRANGLER_CONFIG" --env staging --experimental-autoconfig=false \
            --message "release_set=$RELEASE_SET_ID profile=rehearsal-core-v1"

'''
WORKFLOW.write_text(workflow[:start] + mutate + workflow[end:])

validator = VALIDATOR.read_text()
old = "    'CLOUDFLARE_API_TOKEN: ${{ secrets.CLOUDFLARE_API_TOKEN }}',"
new = "\n".join([
    "    'Checkout exact target source before mutation verification',",
    "    'Re-verify fence and exact immutable Release Set before credentials',",
    "    \"'.preflight_sha256'\",",
    "    \"'.plan_sha256'\",",
    "    'cmp --silent \"$asset_root/release-set.json\" \"$preflight_root/release-set.json\"',",
    "    'release verify',",
    "    'Activate deploy credential after READY and exact-byte verification',",
    "    'DEPLOY_TOKEN: ${{ secrets.CLOUDFLARE_API_TOKEN }}',",
    "    'Re-observe expected-current fence and deploy exact Release Set v2 bits',",
    "    'deployment-identity-ar11.py',",
    "    'test \"$current_id\" = \"$EXPECTED_CURRENT\"',",
])
if old not in validator:
    raise SystemExit('deploy token structural marker missing')
validator = validator.replace(old, new, 1)

anchor = """  errors.push(...forbidMarkers(mutate, [
    'worker-build --release',
    'cargo build',
    'npm run build',
    'release-set-ar11.py build',
    'release compatibility',
    'promotion plan',
    'promotion preflight',
  ], 'promotion phase 3 protected mutation'));
"""
ordering = """  const nativeVerify = mutate.indexOf('Re-verify fence and exact immutable Release Set before credentials');
  const deployCredential = mutate.indexOf('DEPLOY_TOKEN: ${{ secrets.CLOUDFLARE_API_TOKEN }}');
  const reobserveFence = mutate.indexOf('Re-observe expected-current fence and deploy exact Release Set v2 bits');
  const currentFence = mutate.indexOf('test "$current_id" = "$EXPECTED_CURRENT"', reobserveFence);
  const actualDeploy = mutate.indexOf('--message "release_set=$RELEASE_SET_ID profile=rehearsal-core-v1"', reobserveFence);
  if (!(nativeVerify >= 0 && deployCredential > nativeVerify && reobserveFence > deployCredential && currentFence > reobserveFence && actualDeploy > currentFence)) {
    errors.push('mutation credential/fence ordering must be native verify -> credential activation -> expected-current re-observe -> deploy');
  }
"""
if anchor not in validator:
    raise SystemExit('mutate forbid anchor missing')
validator = validator.replace(anchor, anchor + ordering, 1)

self_anchor = """  const rebuild = promotion.replace(
    'Deploy exact Release Set v2 bits',
    'run: cargo build --release\\n      - name: Deploy exact Release Set v2 bits',
  );
  if (!promotionErrors(rebuild).some((error) => error.includes('cargo build'))) {
    throw new Error('promotion rebuild fixture unexpectedly passed');
  }
"""
self_tests = """
  const staleFenceBypass = promotion.replace(
    'test "$current_id" = "$EXPECTED_CURRENT"',
    'test -n "$current_id"',
  );
  if (!promotionErrors(staleFenceBypass).some((error) => error.includes('test \\"$current_id\\" = \\"$EXPECTED_CURRENT\\"'))) {
    throw new Error('mutation stale-fence bypass fixture unexpectedly passed');
  }

  const earlyCredential = promotion
    .replace('DEPLOY_TOKEN: ${{ secrets.CLOUDFLARE_API_TOKEN }}', 'DEPLOY_TOKEN: EARLY_FIXTURE_REMOVED')
    .replace(
      'Re-verify fence and exact immutable Release Set before credentials',
      'DEPLOY_TOKEN: ${{ secrets.CLOUDFLARE_API_TOKEN }}\\n      - name: Re-verify fence and exact immutable Release Set before credentials',
    );
  if (!promotionErrors(earlyCredential).some((error) => error.includes('credential/fence ordering'))) {
    throw new Error('early deploy credential fixture unexpectedly passed');
  }
"""
if self_anchor not in validator:
    raise SystemExit('self-test anchor missing')
validator = validator.replace(self_anchor, self_anchor + self_tests, 1)
VALIDATOR.write_text(validator)
