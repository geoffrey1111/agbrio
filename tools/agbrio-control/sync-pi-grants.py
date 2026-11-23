"""Owner-run, additive Pi connection repair. No task, generation or grant changes.

Dry run by default. --apply requires a retained receipt path. This compatibility
maintenance command is for an installed Host without Pi policy CLI coverage.
Never give this owner-side command to a remote MCP assistant.
"""
import argparse
import json
import sqlite3
import time
from pathlib import Path


def sync(connection, scope_id, apply=False, receipt=None, now=None):
    now = int(time.time() * 1000) if now is None else now
    connection.execute('BEGIN IMMEDIATE' if apply else 'BEGIN')
    try:
        row = connection.execute(
            'SELECT p.policy_json,s.enabled,s.generation FROM pi_scope_policies p '
            'JOIN pi_directory_scopes s ON s.id=p.scope_id WHERE p.scope_id=?',
            (scope_id,)).fetchone()
        if not row or not row[1]:
            raise ValueError('PI_ENABLED_WORKSPACE_REQUIRED')
        policy = json.loads(row[0])
        active = [r[0] for r in connection.execute(
            "SELECT id FROM assistant_grants WHERE scope='INSTANCE' "
            'AND revoked_at IS NULL AND expires_at>? ORDER BY id', (now,))]
        if not active:
            raise ValueError('PI_ACTIVE_INSTANCE_GRANT_REQUIRED')
        added = [g for g in active if g not in policy['grantIds']]
        updated = dict(policy, grantIds=policy['grantIds'] + added)
        report = dict(scopeId=scope_id, scopeGeneration=row[2], addedGrantIds=added,
                      before=policy, after=updated, applied=False,
                      notice='Additive owner-authorized repair; no task creation or sends.')
        if apply:
            if receipt is None:
                raise ValueError('RETAINED_RECEIPT_REQUIRED')
            # Preserve the original before changing the live row. Refuse overwrite.
            with Path(receipt).open('x', encoding='utf-8') as output:
                json.dump(report, output, ensure_ascii=False, indent=2)
            if added:
                cursor = connection.execute(
                    'UPDATE pi_scope_policies SET policy_json=?,updated_at=? '
                    'WHERE scope_id=? AND policy_json=?',
                    (json.dumps(updated, ensure_ascii=False, separators=(',', ':')),
                     now, scope_id, row[0]))
                if cursor.rowcount != 1:
                    raise ValueError('PI_POLICY_CHANGED')
            connection.commit()
            report['applied'] = True
            with Path(receipt).open('w', encoding='utf-8') as output:
                json.dump(report, output, ensure_ascii=False, indent=2)
        else:
            connection.rollback()
        return report
    except BaseException:
        connection.rollback()
        raise


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--database', required=True, type=Path)
    parser.add_argument('--scope-id', required=True)
    parser.add_argument('--apply', action='store_true')
    parser.add_argument('--receipt', type=Path)
    args = parser.parse_args()
    # mode=rw prevents accidentally creating a new profile database.
    with sqlite3.connect(args.database.resolve().as_uri() + '?mode=rw', uri=True,
                         timeout=3) as connection:
        result = sync(connection, args.scope_id, args.apply, args.receipt)
        print(json.dumps({k: result[k] for k in
                          ['scopeId', 'scopeGeneration', 'addedGrantIds', 'applied']}))
