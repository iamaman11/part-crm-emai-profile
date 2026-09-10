#!/usr/bin/env python3
from pathlib import Path

path = Path('.github/scripts/release-operational-ar11.mjs')
text = path.read_text(encoding='utf-8')


def once(old: str, new: str, label: str) -> None:
    global text
    count = text.count(old)
    if count != 1:
        raise SystemExit(f'{label}: expected one anchor, observed {count}')
    text = text.replace(old, new, 1)


once(
    "  const post = jobBlock(promotion, 'post-verify');\n  const rollback = jobBlock(promotion, 'rollback-negative-evidence');\n  for (const [name, block] of Object.entries({ route, 'resolve-preflight': resolvePreflight, 'preflight-ready': ready, 'resolve-verify': resolveAuthorized, mutate, 'post-verify': post, 'rollback-negative-evidence': rollback })) {",
    "  const post = jobBlock(promotion, 'post-verify');\n  const manualOutcome = jobBlock(promotion, 'manual-outcome');\n  const rollback = jobBlock(promotion, 'rollback-negative-evidence');\n  for (const [name, block] of Object.entries({ route, 'resolve-preflight': resolvePreflight, 'preflight-ready': ready, 'resolve-verify': resolveAuthorized, mutate, 'post-verify': post, 'manual-outcome': manualOutcome, 'rollback-negative-evidence': rollback })) {",
    'manual structural job',
)

once(
    "  errors.push(...requireMarkers(post, [\n    'secrets.CLOUDFLARE_OBSERVE_API_TOKEN',\n    'Re-observe provider and verify exact convergence',\n    'promotion verify',\n    '.verified == true',\n  ], 'post-deploy verifier'));\n  errors.push(...forbidMarkers(post, ['secrets.CLOUDFLARE_API_TOKEN }}', 'wrangler deploy --'], 'post-deploy verifier'));\n  errors.push(...secretObservationErrors(post, 'post-deploy verifier'));\n",
    "  errors.push(...requireMarkers(post, [\n    'secrets.CLOUDFLARE_OBSERVE_API_TOKEN',\n    'Re-observe provider and capture promotion.verify natural-owner verdict',\n    'promotion verify',\n    '> \\\"$RUNNER_TEMP/promotion-verify.json\\\"',\n    'decision=\\\"$(jq -er \\\' .decision \\\' \\\"$RUNNER_TEMP/promotion-verify.json\\\")\\\"'.replace('\\\' .decision \\\'', \\\"'.decision'\\\"),\n    \"if: steps.observe_verify.outputs.decision == 'VERIFIED'\",\n  ], 'post-deploy verifier'));\n  errors.push(...forbidMarkers(post, ['secrets.CLOUDFLARE_API_TOKEN }}', 'wrangler deploy --'], 'post-deploy verifier'));\n  errors.push(...secretObservationErrors(post, 'post-deploy verifier'));\n\n  errors.push(...requireMarkers(manualOutcome, [\n    \"if: always() && needs.route.outputs.mode == 'promote'\",\n    'Terminalize one lossless manual AR11 OperationalOutcome',\n    '--mode manual',\n    'Upload terminal manual AR11 OperationalOutcome evidence',\n    'Enforce terminal manual AR11 disposition after evidence publication',\n    '.contract == \\\"PROMOTION_OPERATOR_OUTCOME_V1\\\"',\n    '.procedure == \\\"AR11_RELEASE_SET_PROMOTION\\\"',\n    '.status == \\\"COMPLETED\\\"',\n    '.provider_mutation_started == true',\n    '.provider_mutation_executed == true',\n    '.production_mutation_executed == false',\n    '.effect_state == \\\"EFFECT_VERIFIED\\\"',\n  ], 'manual terminal outcome'));\n  errors.push(...forbidMarkers(manualOutcome, ['secrets.CLOUDFLARE_', 'wrangler deploy --', 'environment: production'], 'manual terminal outcome'));\n  const manualTerminalize = manualOutcome.indexOf('Terminalize one lossless manual AR11 OperationalOutcome');\n  const manualUpload = manualOutcome.indexOf('Upload terminal manual AR11 OperationalOutcome evidence');\n  const manualEnforce = manualOutcome.indexOf('Enforce terminal manual AR11 disposition after evidence publication');\n  if (!(manualTerminalize >= 0 && manualUpload > manualTerminalize && manualEnforce > manualUpload)) {\n    errors.push('manual AR11 must terminalize -> publish evidence -> enforce final disposition');\n  }\n",
    'post verifier semantic gate',
)

# Make the existing early-deploy negative fixture independent of cosmetic step metadata
# such as an `id:` inserted between `name:` and `env:`. The fixture moves the one
# actual deploy credential before the pinned-node/dry-run boundary and proves the
# checker rejects that semantic ordering violation.
start = text.index("  const earlyDeployCredential = files.promotion.replace(")
end = text.index("  const legacyFixture = legacyAuthorityErrors", start)
manual_fixture_insert = "  const missingManualTerminalization = files.promotion.replace('Terminalize one lossless manual AR11 OperationalOutcome', 'Terminalization fixture removed');"
if manual_fixture_insert in text[start:end]:
    raise SystemExit('manual fixtures unexpectedly already present before checker patch')
secret_expr = '${{ secrets.CLOUDFLARE_API_TOKEN }}'
replacement = f"""  const deployToken = '          DEPLOY_TOKEN: {secret_expr}';
  const earlyCredentialAnchor = '      - name: Set up pinned Node before deploy credential';
  let earlyDeployCredential = files.promotion.replace(deployToken, '          DEPLOY_TOKEN: inherited');
  earlyDeployCredential = earlyDeployCredential.replace(
    earlyCredentialAnchor,
    '      - name: Early deploy credential fixture\\n        env:\\n          DEPLOY_TOKEN: {secret_expr}\\n        run: echo early\\n\\n' + earlyCredentialAnchor,
  );
  if (earlyDeployCredential === files.promotion || !earlyDeployCredential.includes('Early deploy credential fixture')) {{
    throw new Error('early deploy credential fixture setup failed');
  }}
  if (!promotionErrors(earlyDeployCredential).some((error) => error.includes('explicit activation proof boundary'))) {{
    throw new Error('early deploy credential fixture unexpectedly passed');
  }}
"""
text = text[:start] + replacement + text[end:]

once(
    "  const legacyFixture = legacyAuthorityErrors((candidate) => candidate.endsWith(LEGACY_FILES[0]));",
    "  const missingManualTerminalization = files.promotion.replace('Terminalize one lossless manual AR11 OperationalOutcome', 'Terminalization fixture removed');\n  if (!promotionErrors(missingManualTerminalization).some((error) => error.includes('manual terminal outcome'))) {\n    throw new Error('missing manual terminalization fixture unexpectedly passed');\n  }\n  const weakManualSuccess = files.promotion.replace('.status == \\\"COMPLETED\\\"', '.status == \\\"RECOVERY_REQUIRED\\\"');\n  if (!promotionErrors(weakManualSuccess).some((error) => error.includes('manual terminal outcome'))) {\n    throw new Error('manual success-without-COMPLETED fixture unexpectedly passed');\n  }\n  const legacyFixture = legacyAuthorityErrors((candidate) => candidate.endsWith(LEGACY_FILES[0]));",
    'manual terminal negative fixtures',
)

path.write_text(text, encoding='utf-8')
Path(__file__).unlink()
print('AR11 semantic checker patch applied; temporary checker helper removed.')
