import type { DatabaseSync } from 'node:sqlite';

let sequence = 0;

/** A domain mutation owns only its transaction or savepoint, never its caller's. */
export function beginWrite(db: DatabaseSync): { commit(): void; rollback(): void } {
  const savepoint = db.isTransaction ? `awareness_${++sequence}` : null;
  db.exec(savepoint ? `SAVEPOINT ${savepoint}` : 'BEGIN IMMEDIATE');
  let active = true;
  return {
    commit() {
      if (!active) return;
      db.exec(savepoint ? `RELEASE SAVEPOINT ${savepoint}` : 'COMMIT');
      active = false;
    },
    rollback() {
      if (!active) return;
      if (savepoint) {
        db.exec(`ROLLBACK TO SAVEPOINT ${savepoint}`);
        db.exec(`RELEASE SAVEPOINT ${savepoint}`);
      } else db.exec('ROLLBACK');
      active = false;
    },
  };
}
