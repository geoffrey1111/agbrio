import importlib.util
import sqlite3
import tempfile
import unittest
from pathlib import Path
import json

spec = importlib.util.spec_from_file_location('repair', Path(__file__).with_name('sync-pi-grants.py'))
repair = importlib.util.module_from_spec(spec)
spec.loader.exec_module(repair)


class RepairTests(unittest.TestCase):
    def setUp(self):
        self.c = sqlite3.connect(':memory:')
        self.c.executescript('''
          CREATE TABLE pi_scope_policies(scope_id TEXT,policy_json TEXT,updated_at INTEGER);
          CREATE TABLE pi_directory_scopes(id TEXT,enabled INTEGER,generation INTEGER);
          CREATE TABLE assistant_grants(id TEXT,scope TEXT,revoked_at INTEGER,expires_at INTEGER);
          INSERT INTO pi_directory_scopes VALUES('scope',1,7);
          INSERT INTO assistant_grants VALUES('live','INSTANCE',NULL,200),
            ('expired','INSTANCE',NULL,99),('revoked','INSTANCE',90,200),
            ('bridge','BRIDGE',NULL,200);
        ''')
        self.policy = dict(roots=['D:/work'], writable=True, terminal=True,
                           administrator=False, inheritContext=True, grantIds=['old'])
        self.c.execute('INSERT INTO pi_scope_policies VALUES(?,?,?)',
                       ('scope', json.dumps(self.policy), 0))
        self.c.commit()

    def tearDown(self):
        self.c.close()

    def test_plan_does_not_write(self):
        r = repair.sync(self.c, 'scope', now=100)
        self.assertEqual(r['addedGrantIds'], ['live'])
        self.assertEqual(json.loads(self.c.execute('SELECT policy_json FROM pi_scope_policies').fetchone()[0]), self.policy)

    def test_apply_is_additive_idempotent_and_preserves_generation(self):
        with tempfile.TemporaryDirectory() as d:
            r = repair.sync(self.c, 'scope', True, Path(d)/'first.json', 100)
            self.assertTrue(r['applied'])
            self.assertEqual(r['after'], dict(self.policy, grantIds=['old', 'live']))
            self.assertEqual(self.c.execute('SELECT generation FROM pi_directory_scopes').fetchone()[0], 7)
            r = repair.sync(self.c, 'scope', True, Path(d)/'second.json', 100)
            self.assertEqual(r['addedGrantIds'], [])

    def test_disabled_scope_fails_closed(self):
        self.c.execute('UPDATE pi_directory_scopes SET enabled=0'); self.c.commit()
        with self.assertRaisesRegex(ValueError, 'ENABLED_WORKSPACE'):
            repair.sync(self.c, 'scope', now=100)

    def test_existing_receipt_blocks_write(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d)/'receipt.json'; p.write_text('keep')
            with self.assertRaises(FileExistsError):
                repair.sync(self.c, 'scope', True, p, 100)
            self.assertEqual(json.loads(self.c.execute('SELECT policy_json FROM pi_scope_policies').fetchone()[0]), self.policy)


if __name__ == '__main__':
    unittest.main()
