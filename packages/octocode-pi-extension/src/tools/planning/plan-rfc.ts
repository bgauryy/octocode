import fs from 'node:fs';
import path from 'node:path';
import type { RfcResolution } from './plan-types.js';

function isWithinWorkspace(workspace: string, candidate: string): boolean {
  const prefix = workspace.endsWith(path.sep) ? workspace : `${workspace}${path.sep}`;
  return candidate === workspace || candidate.startsWith(prefix);
}

export function resolveRfcPath(workspace: string, input: string): RfcResolution {
  const raw = String(input ?? '').trim();
  if (!raw) return { error: 'no RFC path given' };
  try {
    let resolvedWorkspace: string;
    try { resolvedWorkspace = fs.realpathSync(workspace); } catch { resolvedWorkspace = path.resolve(workspace); }

    let candidate = path.isAbsolute(raw) ? path.resolve(raw) : path.resolve(resolvedWorkspace, raw);
    let stat: fs.Stats | undefined;
    try {
      stat = fs.statSync(candidate);
    } catch {
      if (!isWithinWorkspace(resolvedWorkspace, candidate)) {
        return { error: `RFC path must be within the workspace (got ${raw})` };
      }
      return { path: candidate.toLowerCase().endsWith('.md') ? candidate : path.join(candidate, 'RFC.md') };
    }

    if (stat.isDirectory()) {
      const rfcMd = path.join(candidate, 'RFC.md');
      if (fs.existsSync(rfcMd)) {
        candidate = rfcMd;
      } else {
        let entries: string[] = [];
        try { entries = fs.readdirSync(candidate); } catch { /* handled by the missing candidate diagnostic below */ }
        const mdFile = entries.filter((file) => file.toLowerCase().endsWith('.md')).sort()[0];
        if (!mdFile) {
          return { error: `RFC directory has no .md file: ${path.relative(resolvedWorkspace, candidate) || candidate} — add a document and retry` };
        }
        candidate = path.join(candidate, mdFile);
      }
      try { stat = fs.statSync(candidate); } catch {
        return { error: `RFC directory candidate not readable: ${path.relative(resolvedWorkspace, candidate) || candidate}` };
      }
    }

    let real: string;
    try { real = fs.realpathSync(candidate); } catch { real = candidate; }
    if (!isWithinWorkspace(resolvedWorkspace, real)) {
      return { error: `RFC path must be within the workspace (got ${raw})` };
    }
    if (!stat.isFile()) return { error: `RFC path is not a file: ${raw}` };
    return { path: real };
  } catch (error) {
    return { error: `could not resolve RFC path: ${(error as Error).message}` };
  }
}
