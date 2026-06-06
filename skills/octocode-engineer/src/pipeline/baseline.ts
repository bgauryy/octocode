import fs from 'node:fs';
import path from 'node:path';

import type { Finding } from '../types/index.js';

export interface BaselineEntry {
  category: string;
  file: string;
  title: string;
}


export function saveBaseline(
  root: string,
  findings: Finding[]
): string {
  const baselinePath = path.join(root, '.octocode', 'baseline.json');
  const dir = path.dirname(baselinePath);
  fs.mkdirSync(dir, { recursive: true });

  const entries: BaselineEntry[] = findings.map(f => ({
    category: f.category,
    file: f.file,
    title: f.title,
  }));

  const payload = {
    generatedAt: new Date().toISOString(),
    count: entries.length,
    entries,
  };

  fs.writeFileSync(baselinePath, JSON.stringify(payload, null, 2), 'utf8');
  return baselinePath;
}


export function filterKnownFindings<T extends Pick<Finding, 'category' | 'file'>>(
  findings: T[],
  baselinePath: string,
  root: string
): { filtered: T[]; suppressedCount: number } {
  const absPath = path.isAbsolute(baselinePath)
    ? baselinePath
    : path.resolve(root, baselinePath);

  if (!fs.existsSync(absPath)) {
    return { filtered: findings, suppressedCount: 0 };
  }

  try {
    const raw = JSON.parse(fs.readFileSync(absPath, 'utf8'));
    const entries: BaselineEntry[] = raw.entries || [];

    const knownKeys = new Set(
      entries.map(e => `${e.category}::${e.file}`)
    );

    const filtered = findings.filter(
      f => !knownKeys.has(`${f.category}::${f.file}`)
    );

    return {
      filtered,
      suppressedCount: findings.length - filtered.length,
    };
  } catch {
    return { filtered: findings, suppressedCount: 0 };
  }
}
