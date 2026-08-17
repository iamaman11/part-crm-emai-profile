import { readFile } from 'node:fs/promises';

const WORKFLOW_PATH = '.github/workflows/ar8c-resolver-schema-convergence.yml';
const CHECKOUT_PIN = 'actions/checkout@f548e57e544e1ff5a4c46bf1e1b8685f8e4a348a';
const NODE_PIN = 'actions/setup-node@48b55a011bda9f5d6aeb4c2d9c7362e8dae4041e';
const WRANGLER_APPLY = 'npx --yes wrangler@4.94.0 d1 migrations apply "$AR8C_RESOLVER_DB_NAME" --remote --config "$AR8C_WRANGLER_CONFIG" --experimental-provision=false --experimental-auto-create=false --install-skills=false';

function invariant(condition, message) {
  if (!condition) throw new Error(message);
}

function count(source, fragment) {
  return source.split(fragment).length - 1;
}

async function validateExecutionWorkflow() {
  const source = await readFile(WORKFLOW_PATH, 'utf8');
  const lower = source.toLowerCase();

  invariant(source.startsWith('name: AR-8C Resolver Schema Convergence\n'), 'resolver convergence workflow name drifted');
  invariant(count(source, 'workflow_dispatch:') === 1, 'resolver convergence workflow must expose exactly one workflow_dispatch trigger');
  invariant(!/\bpull_request(?:_target)?:/i.test(source), 'resolver convergence mutation workflow must never run from pull requests');
  invariant(!/^\s*push:/m.test(source), 'resolver convergence mutation workflow must never run from push');
  invariant(source.includes('permissions:\n  contents: read\n'), 'resolver convergence workflow must keep contents read-only');
  invariant(!/\bcontents:\s*write\b/i.test(source), 'resolver convergence workflow must never grant contents write');
  invariant(source.includes('group: ar8c-resolver-schema-convergence-staging'), 'resolver convergence workflow concurrency group drifted');
  invariant(source.includes('cancel-in-progress: false'), 'resolver convergence workflow must not cancel an in-flight migration');
  invariant(source.includes("if: github.ref == 'refs/heads/main'"), 'resolver convergence job must reject non-main dispatches before secret exposure');
  invariant(source.includes('environment: staging'), 'resolver convergence workflow must use the protected staging Environment');
  invariant(source.includes('AR8C_TARGET_ENVIRONMENT: staging'), 'resolver convergence target environment must be staging');
  invariant(source.includes('CLOUDFLARE_BOOTSTRAP_TOKEN: ${{ secrets.CLOUDFLARE_BOOTSTRAP_TOKEN }}'), 'protected bootstrap token binding is missing');
  invariant(source.includes('CLOUDFLARE_API_TOKEN: ${{ secrets.CLOUDFLARE_BOOTSTRAP_TOKEN }}'), 'Wrangler must authenticate only through the protected bootstrap token during convergence');
  invariant(!source.includes('secrets.CLOUDFLARE_API_TOKEN'), 'steady-state CLOUDFLARE_API_TOKEN must not be consumed by bootstrap convergence');
  invariant(!source.includes('GH_BOOTSTRAP_ADMIN_TOKEN'), 'GitHub bootstrap admin authority is unnecessary for resolver convergence');
  invariant(!source.includes('CLOUDFLARE_TOKEN_ISSUER_TOKEN'), 'token issuer authority is unnecessary for resolver convergence');
  invariant(source.includes(`uses: ${CHECKOUT_PIN}`), 'checkout action pin drifted');
  invariant(source.includes(`uses: ${NODE_PIN}`), 'setup-node action pin drifted');
  invariant(source.includes('ref: ${{ github.sha }}'), 'resolver convergence checkout must use the exact dispatched main SHA');
  invariant(source.includes('EXPECTED_SOURCE_SHA: ${{ github.sha }}'), 'exact-source verification must bind to github.sha');
  invariant(source.includes('test "$GITHUB_EVENT_NAME" = "workflow_dispatch"'), 'workflow event boundary check is missing');
  invariant(source.includes('test "$GITHUB_REF_NAME" = "main"'), 'accepted-main ref check is missing');
  invariant(source.includes("node-version: '24.19.0'"), 'Node runtime pin drifted');
  invariant(source.includes('test "$(node --version)" = "v24.19.0"'), 'Node exact-version enforcement is missing');
  invariant(count(source, 'node .github/scripts/ar8c-resolver-schema-convergence.mjs prepare') === 1, 'prepare must run exactly once before mutation');
  invariant(count(source, WRANGLER_APPLY) === 1, 'Wrangler apply command or safety flags drifted');
  invariant(count(source, 'node .github/scripts/ar8c-resolver-schema-convergence.mjs verify') === 1, 'verify must run exactly once after mutation/no-op');
  invariant(source.includes('case "$AR8C_RESOLVER_ACTION" in'), 'resolver action must be fail-closed');
  invariant(source.includes('APPLY)') && source.includes('NOOP)') && source.includes('exit 1'), 'resolver action case must allow only APPLY/NOOP');

  for (const forbidden of ['d1 create', 'd1 execute', 'd1 delete', 'terraform', 'production']) {
    invariant(!lower.includes(forbidden), `resolver convergence workflow contains forbidden surface: ${forbidden}`);
  }

  console.log('AR-8C resolver convergence execution workflow contract: PASS');
}

await validateExecutionWorkflow();
