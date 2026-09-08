from pathlib import Path


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match, got {count}")
    return text.replace(old, new, 1)


def expr(value: str) -> str:
    return "${{ " + value + " }}"


executor_path = Path('.github/workflows/d1-migration-executor.yml')
router_path = Path('.github/workflows/d1-operator-comment-router.yml')
executor = executor_path.read_text(encoding='utf-8')
router = router_path.read_text(encoding='utf-8')

if 'authorize_restore:' in executor or "operation_mode:" in executor:
    raise SystemExit('executor already contains restore mode')
if '/d1 time-travel-restore ' in router:
    raise SystemExit('router already contains restore route')

executor = replace_once(
    executor,
    "run-name: D1 authorization=${{ inputs.authorization_digest || inputs.expected_release_set_id || 'none' }}",
    "run-name: D1 authorization=${{ inputs.authorization_digest || inputs.expected_release_set_id || inputs.restore_authorization_comment_id || 'none' }}",
    'executor run-name',
)
executor = replace_once(
    executor,
    "      source_sha:\n        description: Exact accepted-main source SHA\n        required: true\n        type: string\n      component:\n",
    "      source_sha:\n        description: Exact accepted-main source SHA\n        required: true\n        type: string\n      operation_mode:\n        description: Protected D1 effect mode\n        required: false\n        default: migration\n        type: choice\n        options:\n          - migration\n          - time_travel_restore\n      restore_authorization_comment_id:\n        description: Exact OWNER authorization comment for one-shot Time Travel restore\n        required: false\n        default: ''\n        type: string\n      component:\n",
    'executor operation inputs',
)
executor = replace_once(
    executor,
    "  authorize:\n    name: Validate D1 mutation inputs before protected Environment\n",
    "  authorize:\n    if: inputs.operation_mode == 'migration'\n    name: Validate D1 mutation inputs before protected Environment\n",
    'migration authorize guard',
)
executor = replace_once(
    executor,
    "  migrate:\n    needs: authorize\n",
    "  migrate:\n    if: inputs.operation_mode == 'migration'\n    needs: authorize\n",
    'migration execution guard',
)

restore_jobs = r'''

  authorize_restore:
    if: inputs.operation_mode == 'time_travel_restore'
    name: Validate one-shot D1 Time Travel restore authorization
    runs-on: ubuntu-24.04
    timeout-minutes: 5
    env:
      SOURCE_SHA: __EXPR_inputs.source_sha__
      TARGET_ENVIRONMENT: __EXPR_inputs.environment__
      AUTHORIZATION_COMMENT_ID: __EXPR_inputs.restore_authorization_comment_id__
      MUTATION_AUTHORIZED: __EXPR_inputs.mutation_authorized__
      CONFIRMATION: __EXPR_inputs.confirmation__
    steps:
      - name: Fail closed before any restore Environment binding
        run: |
          set -euo pipefail
          test "$GITHUB_EVENT_NAME" = workflow_dispatch
          test "$GITHUB_REF" = refs/heads/main
          test "$GITHUB_SHA" = "$SOURCE_SHA"
          test "$GITHUB_ACTOR" = "$GITHUB_REPOSITORY_OWNER"
          test "$TARGET_ENVIRONMENT" = staging
          test "$MUTATION_AUTHORIZED" = true
          test "$GITHUB_RUN_ATTEMPT" = 1
          [[ "$AUTHORIZATION_COMMENT_ID" =~ ^[1-9][0-9]*$ ]]
          test "$CONFIRMATION" = "$SOURCE_SHA:staging:time_travel_restore:$AUTHORIZATION_COMMENT_ID"

      - name: Bind restore authorization to immutable OWNER comment and exact accepted source
        id: restore_auth
        env:
          GH_TOKEN: __EXPR_github.token__
        run: |
          set -euo pipefail
          root="$RUNNER_TEMP/d1-time-travel-restore-auth"
          rm -rf "$root"
          mkdir -p "$root"
          gh api "repos/$GITHUB_REPOSITORY/issues/comments/$AUTHORIZATION_COMMENT_ID" > "$root/comment.json"
          gh api "repos/$GITHUB_REPOSITORY/issues/266" > "$root/tracker.json"
          gh api "repos/$GITHUB_REPOSITORY/git/commits/$SOURCE_SHA" > "$root/commit.json"
          python - "$root" "$GITHUB_OUTPUT" <<'PY_AUTH'
          import hashlib
          import json
          import os
          import re
          import sys
          from pathlib import Path

          root = Path(sys.argv[1])
          output = Path(sys.argv[2])
          comment = json.loads((root / 'comment.json').read_text(encoding='utf-8'))
          tracker = json.loads((root / 'tracker.json').read_text(encoding='utf-8'))
          commit = json.loads((root / 'commit.json').read_text(encoding='utf-8'))
          owner = os.environ['GITHUB_REPOSITORY_OWNER']
          repository = os.environ['GITHUB_REPOSITORY']
          source = os.environ['SOURCE_SHA']

          tracker_body = tracker.get('body')
          if not isinstance(tracker_body, str):
              raise SystemExit('live stage pointer body missing')
          current = re.findall(r'^CURRENT_STAGE_ISSUE = #([1-9][0-9]*)$', tracker_body, flags=re.MULTILINE)
          if len(current) != 1:
              raise SystemExit('CURRENT_STAGE_ISSUE missing or ambiguous')
          expected_issue_url = f'https://api.github.com/repos/{repository}/issues/{current[0]}'
          if comment.get('issue_url') != expected_issue_url:
              raise SystemExit('restore authorization is not on CURRENT stage issue')
          if (comment.get('user') or {}).get('login') != owner or comment.get('author_association') != 'OWNER':
              raise SystemExit('restore authorization must be recorded by repository OWNER')
          created_at = comment.get('created_at')
          if not isinstance(created_at, str) or comment.get('updated_at') != created_at:
              raise SystemExit('restore authorization comment must be immutable')
          body = comment.get('body')
          if not isinstance(body, str):
              raise SystemExit('restore authorization comment body missing')
          lines = body.strip().splitlines()
          if len(lines) != 2 or lines[0] != 'D1_TIME_TRAVEL_RESTORE_AUTHORIZATION_V1':
              raise SystemExit('restore authorization must use exact two-line V1 envelope')
          raw = lines[1]

          def strict_object(pairs):
              result = {}
              for key, value in pairs:
                  if key in result:
                      raise ValueError(f'duplicate JSON key: {key}')
                  result[key] = value
              return result

          try:
              value = json.loads(raw, object_pairs_hook=strict_object)
          except (json.JSONDecodeError, ValueError) as exc:
              raise SystemExit(f'invalid strict restore authorization JSON: {exc}')
          canonical = json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=False)
          if raw != canonical:
              raise SystemExit('restore authorization JSON must be exact canonical compact sorted JSON')
          expected_keys = {
              'schema_version', 'source_sha', 'source_tree_sha', 'target',
              'restore_timestamp_unix_seconds', 'restore_bookmark',
              'expected_pre_restore_ledger', 'expected_post_restore_ledger',
              'authorized_provider_effects', 'forbidden_provider_effects', 'observation',
          }
          if set(value) != expected_keys or value.get('schema_version') != 1:
              raise SystemExit('restore authorization fields/schema mismatch')
          if value.get('source_sha') != source:
              raise SystemExit('restore authorization source_sha mismatch')
          tree_sha = (commit.get('tree') or {}).get('sha')
          if value.get('source_tree_sha') != tree_sha:
              raise SystemExit('restore authorization source_tree_sha mismatch')
          target = value.get('target')
          if not isinstance(target, dict) or set(target) != {'environment','component','account_id','database_name','database_id'}:
              raise SystemExit('restore target contract mismatch')
          if target.get('environment') != 'staging' or target.get('component') != 'catalog':
              raise SystemExit('restore target must be staging catalog')
          for key in ('account_id', 'database_name', 'database_id'):
              if not isinstance(target.get(key), str) or not target[key]:
                  raise SystemExit(f'restore target {key} missing')
          timestamp = value.get('restore_timestamp_unix_seconds')
          bookmark = value.get('restore_bookmark')
          if not isinstance(timestamp, int) or timestamp <= 0:
              raise SystemExit('restore timestamp invalid')
          if not isinstance(bookmark, str) or re.fullmatch(r'[0-9a-f]{8}-[0-9a-f]{8}-[0-9a-f]{8}-[0-9a-f]{32}', bookmark) is None:
              raise SystemExit('restore bookmark format invalid')
          pre = value.get('expected_pre_restore_ledger')
          post = value.get('expected_post_restore_ledger')
          for label, ledger in (('pre', pre), ('post', post)):
              if not isinstance(ledger, list) or not ledger or len(ledger) != len(set(ledger)):
                  raise SystemExit(f'{label} restore ledger invalid')
              if not all(isinstance(item, str) and re.fullmatch(r'[0-9]{4}_[a-z0-9_]+\.sql', item) for item in ledger):
                  raise SystemExit(f'{label} restore ledger item invalid')
          if len(post) >= len(pre) or pre[:len(post)] != post:
              raise SystemExit('post-restore ledger must be strict prefix of pre-restore ledger')
          if value.get('authorized_provider_effects') != ['D1_TIME_TRAVEL_RESTORE_ONLY']:
              raise SystemExit('restore effect scope mismatch')
          if value.get('forbidden_provider_effects') != ['D1_CREATE','D1_DELETE','D1_MIGRATION_APPLY','D1_TIME_TRAVEL_SECOND_RESTORE','PRODUCTION_MUTATION']:
              raise SystemExit('restore forbidden-effect contract mismatch')
          observation = value.get('observation')
          if not isinstance(observation, dict) or set(observation) != {'run_id','artifact_id','artifact_digest','lookup_json_sha256'}:
              raise SystemExit('restore observation contract mismatch')
          if not isinstance(observation.get('run_id'), int) or not isinstance(observation.get('artifact_id'), int):
              raise SystemExit('restore observation ids invalid')
          if not isinstance(observation.get('artifact_digest'), str) or re.fullmatch(r'sha256:[0-9a-f]{64}', observation['artifact_digest']) is None:
              raise SystemExit('restore observation artifact digest invalid')
          if not isinstance(observation.get('lookup_json_sha256'), str) or re.fullmatch(r'[0-9a-f]{64}', observation['lookup_json_sha256']) is None:
              raise SystemExit('restore observation JSON digest invalid')

          digest = hashlib.sha256(raw.encode('utf-8')).hexdigest()
          validated = {
              'schema_version': 1,
              'kind': 'D1_TIME_TRAVEL_RESTORE_AUTHORIZATION_VALIDATED',
              'authorization_comment_id': int(os.environ['AUTHORIZATION_COMMENT_ID']),
              'authorization_digest': digest,
              'authorization_comment_url': comment.get('html_url'),
              'intent': value,
              'provider_mutation_executed': False,
          }
          (root / 'validated-authorization.json').write_text(
              json.dumps(validated, sort_keys=True, indent=2) + '\n', encoding='utf-8'
          )
          with output.open('a', encoding='utf-8') as handle:
              handle.write(f'authorization_digest={digest}\n')
          PY_AUTH

      - name: Prove restore authorization has not been consumed
        env:
          GH_TOKEN: __EXPR_github.token__
        run: |
          set -euo pipefail
          expected_title="D1 authorization=$AUTHORIZATION_COMMENT_ID"
          prior="$RUNNER_TEMP/prior-restore-runs.json"
          gh api --paginate "repos/$GITHUB_REPOSITORY/actions/workflows/d1-migration-executor.yml/runs?event=workflow_dispatch&per_page=100" |
            jq -s --arg title "$expected_title" --argjson current "$GITHUB_RUN_NUMBER" \
              '[.[].workflow_runs[] | select(.display_title == $title and .run_number < $current) | .id] | unique' > "$prior"
          while IFS= read -r run_id; do
            jobs="$RUNNER_TEMP/prior-restore-jobs-$run_id.json"
            gh api --paginate "repos/$GITHUB_REPOSITORY/actions/runs/$run_id/jobs?filter=all&per_page=100" |
              jq -s '[.[].jobs[]?]' > "$jobs"
            hits="$(jq '[.[] | .steps[]? | select(.name == "Consume Time Travel restore authorization" and .conclusion == "success")] | length' "$jobs")"
            if [ "$hits" -ne 0 ]; then
              echo 'restore authorization already consumed' >&2
              exit 1
            fi
          done < <(jq -r '.[]' "$prior")

      - name: Persist validated restore authorization evidence
        uses: actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a
        with:
          name: d1-time-travel-restore-authorization-__EXPR_inputs.restore_authorization_comment_id__-__EXPR_github.run_id__-__EXPR_github.run_attempt__
          path: __EXPR_runner.temp__/d1-time-travel-restore-auth/validated-authorization.json
          if-no-files-found: error
          retention-days: 30

  restore:
    if: inputs.operation_mode == 'time_travel_restore'
    needs: authorize_restore
    name: staging / exact D1 Time Travel restore
    runs-on: ubuntu-24.04
    timeout-minutes: 15
    environment: staging
    env:
      SOURCE_SHA: __EXPR_inputs.source_sha__
      AUTHORIZATION_COMMENT_ID: __EXPR_inputs.restore_authorization_comment_id__
    steps:
      - name: Checkout exact accepted-main source
        uses: actions/checkout@f548e57e544e1ff5a4c46bf1e1b8685f8e4a348a
        with:
          ref: __EXPR_inputs.source_sha__
          persist-credentials: false
          fetch-depth: 1

      - name: Download validated restore authorization
        uses: actions/download-artifact@fa0a91b85d4f404e444e00e005971372dc801d16
        with:
          name: d1-time-travel-restore-authorization-__EXPR_inputs.restore_authorization_comment_id__-__EXPR_github.run_id__-__EXPR_github.run_attempt__
          path: artifacts/d1-time-travel-restore

      - name: Load exact restore envelope and verify accepted source
        run: |
          set -euo pipefail
          auth=artifacts/d1-time-travel-restore/validated-authorization.json
          test -f "$auth"
          test "$(git rev-parse HEAD)" = "$SOURCE_SHA"
          tree_sha="$(git rev-parse 'HEAD^{tree}')"
          test "$(jq -r '.intent.source_tree_sha' "$auth")" = "$tree_sha"
          jq '.intent.expected_pre_restore_ledger' "$auth" > artifacts/d1-time-travel-restore/expected-pre-ledger.json
          jq '.intent.expected_post_restore_ledger' "$auth" > artifacts/d1-time-travel-restore/expected-post-ledger.json
          {
            echo "CLOUDFLARE_ACCOUNT_ID=$(jq -r '.intent.target.account_id' "$auth")"
            echo "DATABASE_NAME=$(jq -r '.intent.target.database_name' "$auth")"
            echo "DATABASE_ID=$(jq -r '.intent.target.database_id' "$auth")"
            echo "RESTORE_TIMESTAMP=$(jq -r '.intent.restore_timestamp_unix_seconds' "$auth")"
            echo "RESTORE_BOOKMARK=$(jq -r '.intent.restore_bookmark' "$auth")"
            echo "AUTHORIZATION_DIGEST=$(jq -r '.authorization_digest' "$auth")"
          } >> "$GITHUB_ENV"

      - name: Set up pinned Node for exact provider tooling
        uses: actions/setup-node@48b55a011bda9f5d6aeb4c2d9c7362e8dae4041e
        with:
          node-version: '24.19.0'
          package-manager-cache: false

      - name: Materialize exact isolated target provider config
        run: |
          set -euo pipefail
          npm install --global npm@11.17.0
          test "$(node --version)" = v24.19.0
          test "$(npm --version)" = 11.17.0
          test "$(npx --yes wrangler@4.94.0 --version)" = 4.94.0
          jq -n --arg account "$CLOUDFLARE_ACCOUNT_ID" --arg name "$DATABASE_NAME" --arg id "$DATABASE_ID" \
            '{name:"d1-time-travel-restore",compatibility_date:"2026-08-15",account_id:$account,d1_databases:[{binding:"D1_REHEARSAL_DB",database_name:$name,database_id:$id}]}' \
            > artifacts/d1-time-travel-restore/wrangler.json

      - name: Fresh pre-write provider identity, ledger, diagnostics and bookmark proof
        env:
          CLOUDFLARE_API_TOKEN: __EXPR_secrets.CLOUDFLARE_OBSERVE_API_TOKEN__
        run: |
          set -euo pipefail
          test -n "$CLOUDFLARE_API_TOKEN"
          cfg=artifacts/d1-time-travel-restore/wrangler.json
          npx --yes wrangler@4.94.0 d1 info "$DATABASE_NAME" --config "$cfg" --json \
            --experimental-provision=false --experimental-auto-create=false > artifacts/d1-time-travel-restore/provider-identity.json
          python - <<'PY_ID'
          import json, os
          from pathlib import Path
          value = json.loads(Path('artifacts/d1-time-travel-restore/provider-identity.json').read_text())
          expected = {os.environ['DATABASE_NAME'], os.environ['DATABASE_ID']}
          def paired(item):
              if isinstance(item, dict):
                  values = {v for v in item.values() if isinstance(v, str)}
                  return expected.issubset(values) or any(paired(v) for v in item.values())
              if isinstance(item, list):
                  return any(paired(v) for v in item)
              return False
          if not paired(value):
              raise SystemExit('fresh provider identity mismatch')
          PY_ID
          npx --yes wrangler@4.94.0 d1 execute "$DATABASE_NAME" --remote --config "$cfg" \
            --command "SELECT id, name FROM d1_migrations ORDER BY id" --json \
            --experimental-provision=false --experimental-auto-create=false > artifacts/d1-time-travel-restore/ledger-before.json
          python scripts/d1-executor-plan.py normalize-ledger \
            --ledger artifacts/d1-time-travel-restore/ledger-before.json \
            --output artifacts/d1-time-travel-restore/ledger-before-names.json
          cmp --silent artifacts/d1-time-travel-restore/expected-pre-ledger.json artifacts/d1-time-travel-restore/ledger-before-names.json
          npx --yes wrangler@4.94.0 d1 time-travel info "$DATABASE_NAME" --config "$cfg" --json \
            --timestamp "$RESTORE_TIMESTAMP" --experimental-provision=false --experimental-auto-create=false \
            > artifacts/d1-time-travel-restore/bookmark-proof.json
          jq -e --arg bookmark "$RESTORE_BOOKMARK" '.bookmark == $bookmark' artifacts/d1-time-travel-restore/bookmark-proof.json >/dev/null
          npx --yes wrangler@4.94.0 d1 time-travel info "$DATABASE_NAME" --config "$cfg" --json \
            --experimental-provision=false --experimental-auto-create=false > artifacts/d1-time-travel-restore/pre-restore-current-bookmark.json
          npx --yes wrangler@4.94.0 d1 execute "$DATABASE_NAME" --remote --config "$cfg" \
            --command "PRAGMA foreign_key_check" --json --experimental-provision=false --experimental-auto-create=false \
            > artifacts/d1-time-travel-restore/foreign-key-before.json
          npx --yes wrangler@4.94.0 d1 execute "$DATABASE_NAME" --remote --config "$cfg" \
            --command "PRAGMA quick_check" --json --experimental-provision=false --experimental-auto-create=false \
            > artifacts/d1-time-travel-restore/quick-before.json
          jq -e 'length == 1 and .[0].success == true and .[0].results == []' artifacts/d1-time-travel-restore/foreign-key-before.json >/dev/null
          jq -e 'length == 1 and .[0].success == true and (.[0].results | length) == 1 and (.[0].results[0] | to_entries | map(.value)) == ["ok"]' artifacts/d1-time-travel-restore/quick-before.json >/dev/null

      - name: Consume Time Travel restore authorization
        run: |
          set -euo pipefail
          now="$(date -u +%s)"
          jq -n --arg kind D1_TIME_TRAVEL_RESTORE_AUTHORIZATION_CONSUMPTION --arg source "$SOURCE_SHA" \
            --arg comment "$AUTHORIZATION_COMMENT_ID" --arg digest "$AUTHORIZATION_DIGEST" \
            --arg account "$CLOUDFLARE_ACCOUNT_ID" --arg name "$DATABASE_NAME" --arg id "$DATABASE_ID" \
            --arg bookmark "$RESTORE_BOOKMARK" --argjson timestamp "$RESTORE_TIMESTAMP" \
            --argjson run_id "$GITHUB_RUN_ID" --argjson run_attempt "$GITHUB_RUN_ATTEMPT" --argjson consumed "$now" \
            '{schema_version:1,kind:$kind,source_sha:$source,authorization_comment_id:($comment|tonumber),authorization_digest:$digest,target:{environment:"staging",account_id:$account,database_name:$name,database_id:$id},restore_timestamp_unix_seconds:$timestamp,restore_bookmark:$bookmark,executor_run_id:$run_id,run_attempt:$run_attempt,consumed_at_unix_seconds:$consumed,provider_mutation_executed:false}' \
            > artifacts/d1-time-travel-restore/authorization-consumption.json

      - name: Persist one-shot restore authorization consumption
        uses: actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a
        with:
          name: d1-time-travel-restore-authorization-consumption-__EXPR_inputs.restore_authorization_comment_id__
          path: artifacts/d1-time-travel-restore/authorization-consumption.json
          if-no-files-found: error
          retention-days: 30

      - name: Persist restore MUTATION_STARTED receipt
        run: |
          set -euo pipefail
          now="$(date -u +%s)"
          jq -n --arg status MUTATION_STARTED --arg source "$SOURCE_SHA" --arg comment "$AUTHORIZATION_COMMENT_ID" \
            --arg digest "$AUTHORIZATION_DIGEST" --arg account "$CLOUDFLARE_ACCOUNT_ID" --arg name "$DATABASE_NAME" \
            --arg id "$DATABASE_ID" --arg bookmark "$RESTORE_BOOKMARK" --argjson timestamp "$RESTORE_TIMESTAMP" \
            --argjson run_id "$GITHUB_RUN_ID" --argjson run_attempt "$GITHUB_RUN_ATTEMPT" --argjson started "$now" \
            '{schema_version:1,kind:"D1_TIME_TRAVEL_RESTORE_RECEIPT_V1",status:$status,source_sha:$source,authorization_comment_id:($comment|tonumber),authorization_digest:$digest,target:{environment:"staging",account_id:$account,database_name:$name,database_id:$id},restore_timestamp_unix_seconds:$timestamp,restore_bookmark:$bookmark,executor_run_id:$run_id,run_attempt:$run_attempt,mutation_started_at_unix_seconds:$started,provider_mutation_executed:false,migration_apply_executed:false,production_mutation:false,second_restore_executed:false}' \
            > artifacts/d1-time-travel-restore/receipt.json

      - name: Restore exact D1 bookmark with deploy credential
        env:
          CLOUDFLARE_API_TOKEN: __EXPR_secrets.CLOUDFLARE_API_TOKEN__
        run: |
          set -euo pipefail
          test -n "$CLOUDFLARE_API_TOKEN"
          url="https://api.cloudflare.com/client/v4/accounts/$CLOUDFLARE_ACCOUNT_ID/d1/database/$DATABASE_ID/time_travel/restore?bookmark=$RESTORE_BOOKMARK"
          code="$(curl --silent --show-error --request POST \
            --output artifacts/d1-time-travel-restore/restore-result.json --write-out '%{http_code}' \
            -H "Authorization: Bearer $CLOUDFLARE_API_TOKEN" "$url")"
          test "$code" = 200
          jq -e '.success == true and (.errors | length) == 0 and (.result.bookmark | type == "string") and (.result.bookmark | length > 0) and (.result.previous_bookmark | type == "string") and (.result.previous_bookmark | length > 0)' \
            artifacts/d1-time-travel-restore/restore-result.json >/dev/null
          jq '.provider_mutation_executed=true' artifacts/d1-time-travel-restore/receipt.json \
            > artifacts/d1-time-travel-restore/receipt.next.json
          mv artifacts/d1-time-travel-restore/receipt.next.json artifacts/d1-time-travel-restore/receipt.json

      - name: Fresh post-restore ledger and diagnostics proof
        env:
          CLOUDFLARE_API_TOKEN: __EXPR_secrets.CLOUDFLARE_OBSERVE_API_TOKEN__
        run: |
          set -euo pipefail
          cfg=artifacts/d1-time-travel-restore/wrangler.json
          npx --yes wrangler@4.94.0 d1 execute "$DATABASE_NAME" --remote --config "$cfg" \
            --command "SELECT id, name FROM d1_migrations ORDER BY id" --json \
            --experimental-provision=false --experimental-auto-create=false > artifacts/d1-time-travel-restore/ledger-after.json
          python scripts/d1-executor-plan.py normalize-ledger \
            --ledger artifacts/d1-time-travel-restore/ledger-after.json \
            --output artifacts/d1-time-travel-restore/ledger-after-names.json
          cmp --silent artifacts/d1-time-travel-restore/expected-post-ledger.json artifacts/d1-time-travel-restore/ledger-after-names.json
          npx --yes wrangler@4.94.0 d1 execute "$DATABASE_NAME" --remote --config "$cfg" \
            --command "PRAGMA foreign_key_check" --json --experimental-provision=false --experimental-auto-create=false \
            > artifacts/d1-time-travel-restore/foreign-key-after.json
          npx --yes wrangler@4.94.0 d1 execute "$DATABASE_NAME" --remote --config "$cfg" \
            --command "PRAGMA quick_check" --json --experimental-provision=false --experimental-auto-create=false \
            > artifacts/d1-time-travel-restore/quick-after.json
          jq -e 'length == 1 and .[0].success == true and .[0].results == []' artifacts/d1-time-travel-restore/foreign-key-after.json >/dev/null
          jq -e 'length == 1 and .[0].success == true and (.[0].results | length) == 1 and (.[0].results[0] | to_entries | map(.value)) == ["ok"]' artifacts/d1-time-travel-restore/quick-after.json >/dev/null
          now="$(date -u +%s)"
          previous="$(jq -r '.result.previous_bookmark' artifacts/d1-time-travel-restore/restore-result.json)"
          jq --arg status COMPLETED --arg previous "$previous" --argjson completed "$now" \
            '.status=$status | .previous_bookmark=$previous | .completed_at_unix_seconds=$completed | .provider_mutation_executed=true' \
            artifacts/d1-time-travel-restore/receipt.json > artifacts/d1-time-travel-restore/receipt.next.json
          mv artifacts/d1-time-travel-restore/receipt.next.json artifacts/d1-time-travel-restore/receipt.json

      - name: Terminalize restore receipt fail closed
        if: always()
        run: |
          set -euo pipefail
          receipt=artifacts/d1-time-travel-restore/receipt.json
          if [ ! -f "$receipt" ]; then exit 0; fi
          status="$(jq -r '.status' "$receipt")"
          if [ "$status" = MUTATION_STARTED ]; then
            now="$(date -u +%s)"
            jq --arg status RECOVERY_REQUIRED --argjson failed "$now" '.status=$status | .failed_at_unix_seconds=$failed' "$receipt" \
              > artifacts/d1-time-travel-restore/receipt.next.json
            mv artifacts/d1-time-travel-restore/receipt.next.json "$receipt"
          fi
          jq -e '.status == "COMPLETED" or .status == "RECOVERY_REQUIRED"' "$receipt" >/dev/null

      - name: Persist terminal Time Travel restore receipt
        if: always() && hashFiles('artifacts/d1-time-travel-restore/receipt.json') != ''
        uses: actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a
        with:
          name: d1-time-travel-restore-receipt-__EXPR_inputs.restore_authorization_comment_id__-__EXPR_github.run_id__-__EXPR_github.run_attempt__
          path: |
            artifacts/d1-time-travel-restore/receipt.json
            artifacts/d1-time-travel-restore/ledger-before-names.json
            artifacts/d1-time-travel-restore/ledger-after-names.json
            artifacts/d1-time-travel-restore/restore-result.json
          if-no-files-found: error
          retention-days: 30
'''

for token in (
    'inputs.source_sha', 'inputs.environment', 'inputs.restore_authorization_comment_id',
    'inputs.mutation_authorized', 'inputs.confirmation', 'github.token', 'github.run_id',
    'github.run_attempt', 'runner.temp', 'secrets.CLOUDFLARE_OBSERVE_API_TOKEN',
    'secrets.CLOUDFLARE_API_TOKEN',
):
    restore_jobs = restore_jobs.replace(f'__EXPR_{token}__', expr(token))
executor += restore_jobs
executor_path.write_text(executor, encoding='utf-8')

# Router: one more transport route to the same sole D1 mutation owner.
path_line = "      - '.github/workflows/d1-isolated-target-observation.yml'\n      - 'architecture/credential-authority-ar11-extension.json'"
router = router.replace(
    path_line,
    "      - '.github/workflows/d1-isolated-target-observation.yml'\n      - '.github/workflows/d1-migration-executor.yml'\n      - 'architecture/credential-authority-ar11-extension.json'",
)
if router.count("      - '.github/workflows/d1-migration-executor.yml'") != 2:
    raise SystemExit('router must watch executor exactly twice')
router = replace_once(
    router,
    "          observer = Path('.github/workflows/d1-isolated-target-observation.yml').read_text(encoding='utf-8')\n",
    "          observer = Path('.github/workflows/d1-isolated-target-observation.yml').read_text(encoding='utf-8')\n          executor = Path('.github/workflows/d1-migration-executor.yml').read_text(encoding='utf-8')\n",
    'router executor contract input',
)
router = replace_once(
    router,
    "          assert router.count(dispatch_token) == 2, 'comment router must expose exactly two bounded downstream dispatches'\n          assert ('actions/workflows/d1-operator.yml/' + 'dispatches') in router\n          assert ('actions/workflows/d1-isolated-target-observation.yml/' + 'dispatches') in router\n          assert \"github.event.comment.body == '/d1 operator'\" in router\n          assert \"startsWith(github.event.comment.body, '/d1 time-travel-info ')\" in router\n",
    "          assert router.count(dispatch_token) == 3, 'comment router must expose exactly three bounded downstream dispatches'\n          assert ('actions/workflows/d1-operator.yml/' + 'dispatches') in router\n          assert ('actions/workflows/d1-isolated-target-observation.yml/' + 'dispatches') in router\n          assert ('actions/workflows/d1-migration-executor.yml/' + 'dispatches') in router\n          assert \"github.event.comment.body == '/d1 operator'\" in router\n          assert \"startsWith(github.event.comment.body, '/d1 time-travel-info ')\" in router\n          assert \"startsWith(github.event.comment.body, '/d1 time-travel-restore ')\" in router\n          assert 'operation_mode:' in executor and 'time_travel_restore' in executor\n          assert 'authorize_restore:' in executor and 'Restore exact D1 bookmark with deploy credential' in executor\n",
    'router dispatch contract',
)
router = replace_once(
    router,
    "        github.event.comment.body == '/d1 operator' ||\n        startsWith(github.event.comment.body, '/d1 time-travel-info ')\n",
    "        github.event.comment.body == '/d1 operator' ||\n        startsWith(github.event.comment.body, '/d1 time-travel-info ') ||\n        startsWith(github.event.comment.body, '/d1 time-travel-restore ')\n",
    'router route predicate',
)
router = replace_once(
    router,
    "      REPOSITORY_OWNER: ${{ github.repository_owner }}\n",
    "      REPOSITORY_OWNER: ${{ github.repository_owner }}\n      ADMIN_GH_TOKEN: ${{ secrets.GH_ADMIN_OPERATOR_TOKEN }}\n",
    'router governed admin token',
)
router = replace_once(
    router,
    "          if body == '/d1 operator':\n              kind = 'operator'\n              timestamp = ''\n          else:\n              match = re.fullmatch(r'/d1 time-travel-info ([0-9]{10})', body)\n              if match is None:\n                  raise SystemExit('owner D1 command is not an exact supported transport command')\n              kind = 'time_travel_info'\n              timestamp = match.group(1)\n          with open(env_path, 'a', encoding='utf-8') as handle:\n              handle.write(f'D1_ROUTE_KIND={kind}\\n')\n              handle.write(f'D1_TIME_TRAVEL_UNIX_SECONDS={timestamp}\\n')\n",
    "          restore_comment = ''\n          if body == '/d1 operator':\n              kind = 'operator'\n              timestamp = ''\n          else:\n              info = re.fullmatch(r'/d1 time-travel-info ([0-9]{10})', body)\n              restore = re.fullmatch(r'/d1 time-travel-restore ([1-9][0-9]*)', body)\n              if info is not None:\n                  kind = 'time_travel_info'\n                  timestamp = info.group(1)\n              elif restore is not None:\n                  kind = 'time_travel_restore'\n                  timestamp = ''\n                  restore_comment = restore.group(1)\n              else:\n                  raise SystemExit('owner D1 command is not an exact supported transport command')\n          with open(env_path, 'a', encoding='utf-8') as handle:\n              handle.write(f'D1_ROUTE_KIND={kind}\\n')\n              handle.write(f'D1_TIME_TRAVEL_UNIX_SECONDS={timestamp}\\n')\n              handle.write(f'D1_RESTORE_AUTHORIZATION_COMMENT_ID={restore_comment}\\n')\n",
    'router exact command parser',
)
router = replace_once(
    router,
    "        env:\n          GH_TOKEN: ${{ secrets.GH_ADMIN_OPERATOR_TOKEN }}\n",
    "        env:\n          GH_TOKEN: ${{ env.ADMIN_GH_TOKEN }}\n",
    'operator admin token projection',
)

record_marker = "      - name: Record transport result\n"
if router.count(record_marker) != 1:
    raise SystemExit('router record marker missing/ambiguous')
restore_route = r'''      - name: Resolve exact restore transport envelope from OWNER authorization
        if: env.D1_ROUTE_KIND == 'time_travel_restore'
        env:
          GH_TOKEN: __EXPR_github.token__
        run: |
          set -euo pipefail
          auth="$RUNNER_TEMP/d1-restore-authorization.json"
          gh api "repos/$GITHUB_REPOSITORY/issues/comments/$D1_RESTORE_AUTHORIZATION_COMMENT_ID" > "$auth"
          python - "$auth" "$GITHUB_ENV" <<'PY_RESTORE_ROUTE'
          import json, os, sys
          path, env_path = sys.argv[1:]
          comment = json.load(open(path, encoding='utf-8'))
          if (comment.get('user') or {}).get('login') != os.environ['REPOSITORY_OWNER'] or comment.get('author_association') != 'OWNER':
              raise SystemExit('restore authorization must be OWNER-authored')
          body = comment.get('body')
          if not isinstance(body, str):
              raise SystemExit('restore authorization body missing')
          lines = body.strip().splitlines()
          if len(lines) != 2 or lines[0] != 'D1_TIME_TRAVEL_RESTORE_AUTHORIZATION_V1':
              raise SystemExit('restore authorization envelope invalid')
          raw = lines[1]
          value = json.loads(raw)
          if json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=False) != raw:
              raise SystemExit('restore authorization must be canonical JSON')
          if value.get('source_sha') != os.environ['GITHUB_SHA']:
              raise SystemExit('restore authorization source does not match protected main')
          if value.get('authorized_provider_effects') != ['D1_TIME_TRAVEL_RESTORE_ONLY']:
              raise SystemExit('restore authorization effect mismatch')
          target = value.get('target') or {}
          if target.get('environment') != 'staging' or target.get('component') != 'catalog':
              raise SystemExit('restore target must be staging catalog')
          with open(env_path, 'a', encoding='utf-8') as handle:
              handle.write(f"RESTORE_COMPONENT={target.get('component','')}\n")
              handle.write(f"RESTORE_ACCOUNT_ID={target.get('account_id','')}\n")
              handle.write(f"RESTORE_DATABASE_NAME={target.get('database_name','')}\n")
              handle.write(f"RESTORE_DATABASE_ID={target.get('database_id','')}\n")
          PY_RESTORE_ROUTE

      - name: Dispatch one-shot Time Travel restore through sole D1 mutation owner
        if: env.D1_ROUTE_KIND == 'time_travel_restore'
        env:
          GH_TOKEN: __EXPR_env.ADMIN_GH_TOKEN__
        run: |
          set -euo pipefail
          test -n "$GH_TOKEN"
          confirmation="$GITHUB_SHA:staging:time_travel_restore:$D1_RESTORE_AUTHORIZATION_COMMENT_ID"
          payload="$RUNNER_TEMP/d1-time-travel-restore-dispatch.json"
          jq -nc --arg ref main --arg source "$GITHUB_SHA" --arg component "$RESTORE_COMPONENT" \
            --arg account "$RESTORE_ACCOUNT_ID" --arg name "$RESTORE_DATABASE_NAME" --arg id "$RESTORE_DATABASE_ID" \
            --arg comment "$D1_RESTORE_AUTHORIZATION_COMMENT_ID" --arg confirmation "$confirmation" \
            '{ref:$ref,inputs:{operation_mode:"time_travel_restore",restore_authorization_comment_id:$comment,environment:"staging",source_sha:$source,component:$component,account_id:$account,database_name:$name,database_id:$id,target_release_manifest_json:"{}",current_release_manifest_json:"{}",known_good_release_manifest_json:"{}",preconditions_json:"{}",transition_mode:"ordinary",expected_release_set_id:"",prepared_transaction_run_id:"",prepared_transaction_run_attempt:"",transaction_id:"",authorization_digest:"",transaction_authorization_json:"",mutation_authorized:true,confirmation:$confirmation}}' \
            > "$payload"
          gh api --method POST "repos/$GITHUB_REPOSITORY/actions/workflows/d1-migration-executor.yml/dispatches" --input "$payload"

'''
for token in ('github.token', 'env.ADMIN_GH_TOKEN'):
    restore_route = restore_route.replace(f'__EXPR_{token}__', expr(token))
router = router.replace(record_marker, restore_route + record_marker, 1)
router = replace_once(
    router,
    "          if [ \"$D1_ROUTE_KIND\" = operator ]; then\n            downstream='d1-operator.yml / main / zero inputs / owner actor'\n          else\n            downstream=\"d1-isolated-target-observation.yml / main / read-only timestamp=$D1_TIME_TRAVEL_UNIX_SECONDS\"\n          fi\n",
    "          if [ \"$D1_ROUTE_KIND\" = operator ]; then\n            downstream='d1-operator.yml / main / zero inputs / owner actor'\n          elif [ \"$D1_ROUTE_KIND\" = time_travel_info ]; then\n            downstream=\"d1-isolated-target-observation.yml / main / read-only timestamp=$D1_TIME_TRAVEL_UNIX_SECONDS\"\n          else\n            downstream=\"d1-migration-executor.yml / main / one-shot time_travel_restore authorization_comment=$D1_RESTORE_AUTHORIZATION_COMMENT_ID\"\n          fi\n",
    'router summary route',
)
router_path.write_text(router, encoding='utf-8')

# Temporary helpers must not survive the resulting source commit.
Path('.github/workflows/tx7-restore-source-patcher.yml').unlink()
Path('.github/tx7_restore_source_patch.py').unlink()
