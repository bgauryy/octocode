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
        self.assertEqual(database.schema()['schemaSha256'], '911e1b36522d7737d343f94aecf7f9e22f0fca2dfa7ce1cd3ba7dbab6d97ab7b')

    def test_lease_keys_use_frozen_unicode_16(self):
        self.assertEqual(lease_key('/workspace/Straße'), lease_key('/workspace/STRASSE'))
        self.assertEqual(lease_key('/workspace/é'), lease_key('/workspace/E\u0301'))
        # Cyrillic Tje was added in Unicode 16; older Python casefold leaves the
        # capital intact. Both spellings must still contend for the same lease.
        self.assertEqual(lease_key('/workspace/\u1c89'), lease_key('/workspace/\u1c8a'))


if __name__ == '__main__':
    unittest.main()
