// One writer for every JSON file a run publishes (data.json, the checkpoint, links.json).
import { randomUUID } from 'node:crypto';
import { renameSync, rmSync, writeFileSync } from 'node:fs';

// Readers may have the previous result open during refresh. Rename only a complete JSON file.
export function writeGraph(path: string, data: unknown) {
  const tmp = `${path}.${randomUUID()}.tmp`;
  try {
    writeFileSync(tmp, JSON.stringify(data));
    renameSync(tmp, path);
  } finally {
    rmSync(tmp, { force: true });
  }
}
