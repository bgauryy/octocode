"""Portability boundaries not covered by the process-level protocol fixtures."""
import sqlite3
import sys
import tempfile
import unittest
from pathlib import Path
from contextlib import contextmanager

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'scripts'))
from communication import database
from communication.paths import lease_key
from communication.store import Store


@contextmanager
def sqlite_version(version):
    original = sqlite3.sqlite_version_info
    sqlite3.sqlite_version_info = version
    try:
        yield
    finally:
        sqlite3.sqlite_version_info = original


class PythonRuntimeTests(unittest.TestCase):
    def test_wal_reset_patched_branches(self):
        for version in [(3, 44, 6), (3, 44, 7), (3, 50, 7), (3, 50, 8), (3, 51, 3), (3, 52, 0)]:
            self.assertTrue(database.wal_safe(version), version)
        for version in [(3, 42, 0), (3, 44, 5), (3, 45, 0), (3, 49, 9), (3, 50, 6), (3, 51, 2)]:
            self.assertFalse(database.wal_safe(version), version)

    def test_old_sqlite_rejected_before_creating_database(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'db'
            with sqlite_version((3, 41, 2)):
                with self.assertRaisesRegex(ValueError, 'SQLite >=3.42'):
                    database.open(path, create=True)
            self.assertFalse(path.exists())

    def test_unpatched_sqlite_creates_delete_journal_and_preserves_it(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'db'
            with sqlite_version((3, 51, 0)):
                db = database.open(path, create=True)
                self.assertEqual(database.metadata(db)['journalMode'], 'delete')
                db.close()
            with sqlite_version((3, 51, 3)):
                db = database.open(path)
                self.assertEqual(database.metadata(db)['journalMode'], 'delete')
                db.close()

    def test_unsafe_existing_wal_fails_without_changing_it(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'db'
            with sqlite_version((3, 51, 3)):
                db = database.open(path, create=True)
                self.assertEqual(database.metadata(db)['journalMode'], 'wal')
                db.close()
            with sqlite_version((3, 51, 2)):
                with self.assertRaisesRegex(ValueError, 'This database uses WAL; upgrade Python'):
                    database.open(path)
                db = database.open(path, read_only=True)
                self.assertEqual(database.metadata(db)['journalMode'], 'wal')
                db.close()

    def test_schema_fingerprint_preserves_database_protocol(self):
        self.assertEqual(database.schema()['schemaSha256'], '562d823b51cf62ae87ae0e5c5e0d2001962b3dbde5be3b8a0511fda159979aa9')

    def test_leave_releases_pending_claims_without_acknowledging_mail(self):
        with tempfile.TemporaryDirectory() as directory:
            store = Store(Path(directory) / 'db', directory, create=True)
            try:
                author = store.call('', 'join', {'name': 'author', 'vendor': 'raw'})['id']
                recipient = store.call('', 'join', {'name': 'recipient', 'vendor': 'raw'})['id']
                sent = store.call(author, 'send_message', {'to': recipient, 'body': 'Review this', 'reasoning': 'Verify claim cleanup'})
                store.db.execute('UPDATE deliveries SET claimedBy=?,claimUntil=? WHERE message=? AND recipient=?', ('worker', 2**62, sent['id'], recipient))
                store.call(recipient, 'leave', {})
                state = store.db.execute('SELECT claimedBy,claimUntil,acknowledgedAt FROM deliveries WHERE message=? AND recipient=?', (sent['id'], recipient)).fetchone()
                self.assertEqual(state, (None, 0, None))
                self.assertEqual(store.fetch(author, {'type': 'delivery.acknowledged'})['items'], [])
            finally:
                store.db.close()

    def test_claude_inbound_follows_documented_hold_rules(self):
        from communication.transport import claude_inbound
        cases = [
            (None, False, 'unknown'),
            ({'permissionMode': 'bypassPermissions'}, False, 'held'),
            ({'permissionMode': 'bypassPermissions'}, True, 'delivered'),
            ({'permissionMode': 'default'}, False, 'delivered'),
            ({'permissionMode': 'auto'}, False, 'delivered'),
            ({'permissionMode': 'plan'}, False, 'unknown'),
            ({'permissionMode': 'default', 'crossSessionInbound': 'hold'}, True, 'held'),
            ({'permissionMode': 'default', 'crossSessionInbound': 'refuse'}, False, 'refused'),
            ({'permissionMode': 'bypassPermissions', 'crossSessionInbound': 'accept'}, False, 'delivered'),
            ({'permissionMode': 'default', 'crossSessionInbound': 'unrecognized'}, False, 'unknown'),
        ]
        for state, own_child, outcome in cases:
            self.assertEqual(claude_inbound(state, own_child)[0], outcome, (state, own_child))

    def test_lease_keys_use_frozen_unicode_16(self):
        self.assertEqual(lease_key('/workspace/Straße'), lease_key('/workspace/STRASSE'))
        self.assertEqual(lease_key('/workspace/é'), lease_key('/workspace/E\u0301'))
        # Cyrillic Tje was added in Unicode 16; older Python casefold leaves the
        # capital intact. Both spellings must still contend for the same lease.
        self.assertEqual(lease_key('/workspace/\u1c89'), lease_key('/workspace/\u1c8a'))


if __name__ == '__main__':
    unittest.main()
