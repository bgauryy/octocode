import { existsSync, readdirSync, lstatSync } from 'node:fs';
import { join } from 'node:path';

export function getDirectorySizeBytes(targetPath: string): number {
  if (!existsSync(targetPath)) return 0;

  let total = 0;
  const stack = [targetPath];

  while (stack.length > 0) {
    const current = stack.pop()!;

    let entries: string[];
    try {
      entries = readdirSync(current);
    } catch {
      continue;
    }

    for (const entry of entries) {
      const fullPath = join(current, entry);
      try {
        const st = lstatSync(fullPath);
        if (st.isSymbolicLink()) {
          continue;
        }
        if (st.isDirectory()) {
          stack.push(fullPath);
        } else if (st.isFile()) {
          total += st.size;
        }
      } catch {
        void 0;
      }
    }
  }

  return total;
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const kb = bytes / 1024;
  if (kb < 1024) return `${kb.toFixed(1)} KB`;
  const mb = kb / 1024;
  if (mb < 1024) return `${mb.toFixed(1)} MB`;
  const gb = mb / 1024;
  return `${gb.toFixed(2)} GB`;
}
