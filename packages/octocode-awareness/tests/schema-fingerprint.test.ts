import { afterEach, describe, expect, it } from 'vitest';
import { DatabaseSync } from '../src/sqlite.js';
import { SCHEMA_DDL, SCHEMA_INDEX_DDL } from '../src/db-schema.js';
import { initDb } from '../src/db-init.js';
import { assertCanonicalSchemaFingerprint } from '../src/db-introspection.js';
import { normalizeSchemaSql, readSchemaObjects, schemaObjectsFingerprint } from '../src/agent-store-schema.js';
import { AWARENESS_APPLICATION_ID, AWARENESS_SCHEMA_VERSION } from '../src/storage-scope.js';

// A DDL change must intentionally mint a new generation, filename, and fingerprint.
const GENERATION_FINGERPRINTS = {
  4: 'c7a8e65cdabc20e301931c9a2e3cb6590e6dd19d963dbfea3d6dcb1d635c357f',
  5: 'dec4da0e8e5147a95f189dc8a4cd85190c62cd8d3acfbaa3bee2be468426f060',
} as const;

const databases: DatabaseSync[] = [];
function database() {
  const db = new DatabaseSync(':memory:');
  databases.push(db);
  return db;
}
afterEach(() => { databases.splice(0).forEach(db => db.close()); });

describe('canonical schema fingerprint semantics', () => {
  it('binds current DDL to the current schema generation', () => {
    const canonical = database();
    canonical.exec(SCHEMA_DDL);
    canonical.exec(SCHEMA_INDEX_DDL);
    expect(schemaObjectsFingerprint(readSchemaObjects(canonical)))
      .toBe(GENERATION_FINGERPRINTS[AWARENESS_SCHEMA_VERSION]);
  });

  it('rejects unmarked populated stores and empty stores with a claimed identity', () => {
    const unmarked = database();
    unmarked.exec(SCHEMA_DDL);
    unmarked.exec(SCHEMA_INDEX_DDL);
    expect(() => initDb(unmarked)).toThrow(/unrecognized|canonical/);
    const empty = database();
    empty.exec(`PRAGMA application_id = ${AWARENESS_APPLICATION_ID}`);
    expect(() => initDb(empty)).toThrow(/canonical/);
  });
  it.each([
    ["DEFAULT 'ACTIVE'", "DEFAULT 'active'"],
    ["DEFAULT 'a  b'", "DEFAULT 'a b'"],
    ["DEFAULT 'a--first'", "DEFAULT 'a--second'"],
    ["DEFAULT 'a,b'", "DEFAULT 'a, b'"],
    ["DEFAULT 'IF NOT EXISTS'", "DEFAULT ''"],
    ['DEFAULT "ACTIVE"', 'DEFAULT "active"'],
  ])('preserves literal differences: %s vs %s', (left, right) => {
    expect(normalizeSchemaSql(left)).not.toBe(normalizeSchemaSql(right));
  });

  it('ignores formatting and comments outside literals', () => {
    expect(normalizeSchemaSql('CREATE TABLE IF NOT EXISTS x ( value TEXT /* comment */ );'))
      .toBe(normalizeSchemaSql('create table x(value text);'));
  });

  it('rejects altered enum literals in otherwise canonical DDL', () => {
    const db = database();
    db.exec(SCHEMA_DDL.replace("DEFAULT 'ACTIVE'", "DEFAULT 'active'"));
    db.exec(SCHEMA_INDEX_DDL);
    expect(() => assertCanonicalSchemaFingerprint(db)).toThrow(/schema.*mismatch/);
  });

  it.each(['memories_fts_extra', 'memory_fts_extra', 'sqliteXextra'])('does not hide user relations with reserved-looking prefixes: %s', name => {
    const db = database();
    initDb(db);
    db.exec(`CREATE TABLE ${name}(value TEXT)`);
    expect(() => initDb(db)).toThrow(/unrecognized|mismatch/);
  });
});
