from pathlib import Path

path = Path('.github/workflows/d1-operator-comment-router.yml')
text = path.read_text(encoding='utf-8')

def once(old, new, label):
    global text
    n = text.count(old)
    if n != 1:
        raise SystemExit(f'{label}: expected one match, got {n}')
    text = text.replace(old, new, 1)

once(
    "      REPOSITORY_OWNER: ${{ github.repository_owner }}\n      ADMIN_GH_TOKEN: ${{ secrets.GH_ADMIN_OPERATOR_TOKEN }}\n",
    "      REPOSITORY_OWNER: ${{ github.repository_owner }}\n",
    'remove job-wide admin secret',
)
once(
    "          assert router.count(secret_prefix) == 1, 'comment router may consume only one governed GitHub transport secret'\n          assert dispatch_secret in router, 'comment router must reuse the governed GitHub admin operator credential only for owner-actor operator dispatch'\n",
    "          assert router.count(secret_prefix) == 2, 'governed GitHub admin credential may appear only in the two privileged dispatch steps'\n          assert router.count(dispatch_secret) == 2, 'operator and restore dispatch must reuse only the governed GitHub admin credential'\n",
    'secret contract',
)
once(
    "          assert 'operation_mode:' in executor and 'time_travel_restore' in executor\n          assert 'authorize_restore:' in executor and 'Restore exact D1 bookmark with deploy credential' in executor\n",
    "          assert 'operation_mode:' in executor and 'time_travel_restore' in executor\n          assert 'authorize_restore:' in executor and 'Restore exact D1 bookmark with deploy credential' in executor\n          assert executor.count(\"if: inputs.operation_mode == 'migration'\") == 2, 'existing migration authorize+execute jobs must both be guarded from restore mode'\n          restore_source = executor.split('\\n  authorize_restore:', 1)[1]\n          assert 'd1 migrations apply' not in restore_source, 'restore mode must never apply migrations'\n          assert 'd1 create' not in restore_source and 'd1 delete' not in restore_source\n          assert restore_source.count('secrets.CLOUDFLARE_API_TOKEN') == 1, 'restore mode may expose deploy credential only to the exact restore write step'\n          assert restore_source.count('secrets.CLOUDFLARE_OBSERVE_API_TOKEN') == 2, 'restore mode must use read-only credential only for pre/post provider proof'\n          assert 'environment: staging' in restore_source\n          assert 'D1_TIME_TRAVEL_RESTORE_AUTHORIZATION_V1' in restore_source\n          assert '/time_travel/restore?bookmark=$RESTORE_BOOKMARK' in restore_source\n          order = [\n              restore_source.index('Prove restore authorization has not been consumed'),\n              restore_source.index('Consume Time Travel restore authorization'),\n              restore_source.index('Persist restore MUTATION_STARTED receipt'),\n              restore_source.index('Restore exact D1 bookmark with deploy credential'),\n              restore_source.index('Fresh post-restore ledger and diagnostics proof'),\n              restore_source.index('Terminalize restore receipt fail closed'),\n          ]\n          assert order == sorted(order), 'restore one-shot/receipt/postverify ordering drifted'\n",
    'restore static contract hardening',
)
once(
    "        env:\n          GH_TOKEN: ${{ env.ADMIN_GH_TOKEN }}\n",
    "        env:\n          GH_TOKEN: ${{ secrets.GH_ADMIN_OPERATOR_TOKEN }}\n",
    'operator secret scope',
)
once(
    "        env:\n          GH_TOKEN: ${{ env.ADMIN_GH_TOKEN }}\n        run: |\n          set -euo pipefail\n          test -n \"$GH_TOKEN\"\n          confirmation=\"$GITHUB_SHA:staging:time_travel_restore:$D1_RESTORE_AUTHORIZATION_COMMENT_ID\"\n",
    "        env:\n          GH_TOKEN: ${{ secrets.GH_ADMIN_OPERATOR_TOKEN }}\n        run: |\n          set -euo pipefail\n          test -n \"$GH_TOKEN\"\n          confirmation=\"$GITHUB_SHA:staging:time_travel_restore:$D1_RESTORE_AUTHORIZATION_COMMENT_ID\"\n",
    'restore secret scope',
)
once(
    "            echo '- standard operator owner actor: governed GitHub admin operator credential only'\n            echo '- read-only observer dispatch: ephemeral GitHub Actions token only'\n",
    "            echo '- privileged operator/restore dispatch actor: governed GitHub admin operator credential only'\n            echo '- read-only observer and authorization reads: ephemeral GitHub Actions token only'\n",
    'summary wording',
)
path.write_text(text, encoding='utf-8')
Path('.github/tx7_restore_hardening.py').unlink()
Path('.github/workflows/tx7-restore-hardening-patcher.yml').unlink()
