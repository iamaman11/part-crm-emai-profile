import json
from pathlib import Path

root = Path('.')
script_path = root / 'scripts/test-cross-component-acceptance.py'
script = script_path.read_text(encoding='utf-8')
old = '''        (
            "frontend/src/shared/api/endpoints.ts",
            (
                "getSession",
                "createClient",
                "setClientGrant",
                "createProfile",
                "assignProfile",
                "setProfileGrant",
                "registerGeneration",
                "changeGenerationActivation",
                "getCoordinator",
                "createMailboxBinding",
                "runMailboxJob",
            ),
            "React endpoint composition",
        ),
'''
new = '''        (
            "frontend/src/shared/api/endpoint.ts",
            ("segment", "pagedPath", "mutate", "requestJson<MutationReceipt>"),
            "React shared transport helpers",
        ),
        (
            "frontend/src/features/session/api.ts",
            ("getSession", "requestJson<ActorSession>"),
            "Session frontend API ownership",
        ),
        (
            "frontend/src/features/access/api.ts",
            ("bootstrapOwner", "transferOwner", "createInvitation", "acceptInvitation", "listMembers", "updateMembershipStatus"),
            "Access frontend API ownership",
        ),
        (
            "frontend/src/features/clients/api.ts",
            ("createClient", "setClientGrant", "searchClientMail", "getClientMailMessage"),
            "Client frontend API ownership",
        ),
        (
            "frontend/src/features/profiles/api.ts",
            ("createProfile", "assignProfile", "setProfileGrant", "registerGeneration", "changeGenerationActivation", "getCoordinator", "commandCoordinator"),
            "Profile frontend API ownership",
        ),
        (
            "frontend/src/features/mailboxes/api.ts",
            ("listMailboxes", "createMailboxBinding", "runMailboxJob"),
            "Mailbox frontend API ownership",
        ),
'''
if script.count(old) != 1:
    raise SystemExit(f'expected one stale React endpoint composition block, found {script.count(old)}')
script = script.replace(old, new, 1)

anchor = '''    if (ROOT / "apps/control-plane-worker/src/api.rs").exists():
        fail("legacy api.rs must remain removed after identity application-boundary migration")
'''
replacement = anchor + '''
    for obsolete_frontend_facade in (
        "frontend/src/shared/api/endpoints.ts",
        "frontend/src/shared/api/types.ts",
        "frontend/src/shared/api/clientMail.ts",
    ):
        if (ROOT / obsolete_frontend_facade).exists():
            fail(f"central frontend capability facade must remain removed: {obsolete_frontend_facade}")
'''
if script.count(anchor) != 1:
    raise SystemExit('cross-component legacy-api anchor drifted')
script = script.replace(anchor, replacement, 1)
script_path.write_text(script, encoding='utf-8')

manifest_path = root / 'tests/cross-component/standalone-acceptance.json'
manifest = json.loads(manifest_path.read_text(encoding='utf-8'))
phases = manifest.get('phases')
if not isinstance(phases, list) or len(phases) != 6:
    raise SystemExit('unexpected cross-component phase manifest')
phase6 = phases[5]
expected = [
    'frontend/src/shared/api/client.ts',
    'frontend/src/shared/api/endpoints.ts',
    'frontend/src/shared/ui/StatusMessage.test.tsx',
]
if phase6.get('evidence') != expected:
    raise SystemExit(f'unexpected phase 6 evidence before R6 sync: {phase6.get("evidence")!r}')
phase6['evidence'] = [
    'frontend/src/shared/api/client.ts',
    'frontend/src/shared/api/endpoint.ts',
    'frontend/src/features/session/api.ts',
    'frontend/src/features/access/api.ts',
    'frontend/src/features/clients/api.ts',
    'frontend/src/features/mailboxes/api.ts',
    'frontend/src/features/profiles/api.ts',
    'frontend/src/shared/ui/StatusMessage.test.tsx',
]
manifest_path.write_text(json.dumps(manifest, indent=2) + '\n', encoding='utf-8')

(root / '.github/.r6-cross-user-finalize').write_text(
    'temporary marker; delete with user-authored finalization commit\n',
    encoding='utf-8',
)
