import { existsSync, realpathSync } from 'node:fs';
import { isAbsolute, resolve } from 'node:path';
import type { DatabaseSync } from 'node:sqlite';
import { fileURLToPath } from 'node:url';
import { normalizeWorkspacePath } from './git.js';
import { AwarenessQueryParams, BindValue, MemoryDbRow, SCOPE_CACHE, Scope } from './repo-model.js';

export function scopeFromParams(params: AwarenessQueryParams): Scope {
  const cached = SCOPE_CACHE.get(params);
  if (cached) return cached;
  const cwd = params.cwd ? resolve(params.cwd) : process.cwd();
  const rawWorkspace = params.workspacePath ?? params.workspace_path ?? params.workspace ?? cwd;
  const workspacePath = rawWorkspace ? resolve(String(rawWorkspace)) : null;
  const scope = {
    // Keep the raw resolved path for projection output / echo; the alias set
    // below carries the extra keys used for DB row matching.
    workspacePath,
    workspacePaths: workspacePath ? workspaceAliases(workspacePath, cwd) : [],
    artifact: params.artifact ? String(params.artifact) : null,
    repo: params.repo ? String(params.repo) : null,
    ref: params.ref ? String(params.ref) : null,
  };
  SCOPE_CACHE.set(params, scope);
  return scope;
}

export function withScope(
  params: AwarenessQueryParams,
  overrides: Partial<AwarenessQueryParams>,
): AwarenessQueryParams {
  const derived = { ...params, ...overrides };
  SCOPE_CACHE.set(derived, scopeFromParams(params));
  return derived;
}

/** Plans, tasks, runs, locks, and edit logs are workspace/artifact scoped only. */
export function workspaceArtifactScope(scope: Scope): Scope {
  if (!scope.repo && !scope.ref) return scope;
  return { ...scope, repo: null, ref: null };
}

export function workspaceAliases(workspacePath: string, cwd?: string): string[] {
  const aliases = new Set<string>([workspacePath]);
  try {
    aliases.add(realpathSync.native(workspacePath));
  } catch {
    try { aliases.add(realpathSync(workspacePath)); } catch { /* path may not exist yet */ }
  }
  // D1 fix: also match the git-root workspace key that write paths store via
  // fillScope, so reads run from a package/subdir meet the rows writes stored.
  // Additive — the raw resolved path stays in the set, so non-git and raw-path
  // rows keep matching.
  try {
    const gitRoot = normalizeWorkspacePath(workspacePath, cwd ?? workspacePath);
    if (gitRoot) aliases.add(gitRoot);
  } catch { /* leave aliases as-is if git detection fails */ }
  return [...aliases];
}

export function addNullableScope(where: string[], binds: BindValue[], scope: Scope, alias = ''): void {
  const p = alias ? `${alias}.` : '';
  if (scope.workspacePaths.length > 0) {
    where.push(`(${p}workspace_path IN (${scope.workspacePaths.map(() => '?').join(',')}) OR ${p}workspace_path IS NULL)`);
    binds.push(...scope.workspacePaths);
  }
  if (scope.artifact) {
    where.push(`(${p}artifact = ? OR ${p}artifact IS NULL)`);
    binds.push(scope.artifact);
  }
  if (scope.repo) {
    where.push(`(${p}repo = ? OR ${p}repo IS NULL)`);
    binds.push(scope.repo);
  }
  if (scope.ref) {
    where.push(`(${p}ref = ? OR ${p}ref IS NULL)`);
    binds.push(scope.ref);
  }
}

export function addExactScope(where: string[], binds: BindValue[], scope: Scope, alias = ''): void {
  const p = alias ? `${alias}.` : '';
  if (scope.workspacePaths.length > 0) {
    where.push(`${p}workspace_path IN (${scope.workspacePaths.map(() => '?').join(',')})`);
    binds.push(...scope.workspacePaths);
  }
  if (scope.artifact) {
    where.push(`(${p}artifact = ? OR ${p}artifact IS NULL)`);
    binds.push(scope.artifact);
  }
  if (scope.repo) {
    where.push(`(${p}repo = ? OR ${p}repo IS NULL)`);
    binds.push(scope.repo);
  }
  if (scope.ref) {
    where.push(`(${p}ref = ? OR ${p}ref IS NULL)`);
    binds.push(scope.ref);
  }
}

export function addTextFilter(where: string[], binds: BindValue[], query: string | null | undefined, columns: string[]): void {
  const q = query?.trim();
  if (!q) return;
  where.push(`LOWER(${columns.map(c => `COALESCE(${c}, '')`).join(" || ' ' || ")}) LIKE LOWER(?)`);
  binds.push(`%${q}%`);
}

export function addStateFilter(
  where: string[],
  binds: BindValue[],
  states: string[],
  column: string,
  normalize: (state: string) => string = (state) => state,
): void {
  if (states.length === 0) return;
  where.push(`${column} IN (${states.map(() => '?').join(',')})`);
  binds.push(...states.map(normalize));
}

export function addLabelsFilter(where: string[], binds: BindValue[], labels: string[], column = 'label'): void {
  if (labels.length === 0) return;
  where.push(`${column} IN (${labels.map(() => '?').join(',')})`);
  binds.push(...labels.map(l => l.toUpperCase()));
}

export function fileRefCandidates(file: string, workspacePath: string | null): string[] {
  const trimmed = file.trim();
  if (!trimmed) return [];
  if (trimmed.startsWith('file:')) return [trimmed];
  const absolute = isAbsolute(trimmed) ? resolve(trimmed) : resolve(workspacePath ?? process.cwd(), trimmed);
  return [`file:${absolute}`, `%${trimmed}%`];
}

export function stripLocationSuffix(value: string): string {
  return value.trim().replace(/:(\d+)(?::\d+)?$/, '');
}

export function localPathFromReference(reference: string, workspacePath: string | null): string | null {
  const trimmed = reference.trim();
  if (!trimmed) return null;
  let rawPath: string | null = null;
  if (/^file:\/\//i.test(trimmed)) {
    try {
      rawPath = fileURLToPath(trimmed);
    } catch {
      rawPath = trimmed.replace(/^file:\/+/i, '/');
    }
  } else if (/^file:/i.test(trimmed)) {
    rawPath = trimmed.slice('file:'.length);
  } else if (/^path:/i.test(trimmed)) {
    rawPath = trimmed.slice('path:'.length);
  } else if (/^[a-z][a-z0-9+.-]*:/i.test(trimmed)) {
    return null;
  } else if (isAbsolute(trimmed) || trimmed.startsWith('./') || trimmed.startsWith('../') || /^[^/\s]+\/.+/.test(trimmed)) {
    rawPath = trimmed;
  }
  if (!rawPath) return null;
  const clean = stripLocationSuffix(rawPath);
  return isAbsolute(clean) ? resolve(clean) : resolve(workspacePath ?? process.cwd(), clean);
}

export function referenceHealth(references: string[], workspacePath: string | null): Record<string, string[] | number> {
  const fileReferences: string[] = [];
  const existingFiles: string[] = [];
  const missingFiles: string[] = [];
  const missingReferences: string[] = [];
  for (const reference of references) {
    const localPath = localPathFromReference(reference, workspacePath);
    if (!localPath) continue;
    fileReferences.push(localPath);
    if (existsSync(localPath)) {
      existingFiles.push(localPath);
    } else {
      missingFiles.push(localPath);
      missingReferences.push(reference);
    }
  }
  return {
    reference_count: references.length,
    file_reference_count: fileReferences.length,
    missing_reference_count: missingReferences.length,
    file_references: [...new Set(fileReferences)],
    existing_files: [...new Set(existingFiles)],
    missing_files: [...new Set(missingFiles)],
    missing_references: [...new Set(missingReferences)],
  };
}

export function addMemoryFileFilter(where: string[], binds: BindValue[], file: string | null | undefined, scope: Scope): void {
  if (!file) return;
  const candidates = fileRefCandidates(file, scope.workspacePath);
  if (candidates.length === 0) return;
  where.push(`EXISTS (
    SELECT 1 FROM memory_refs r
    WHERE r.memory_id = memories.memory_id
      AND (${candidates.map(() => 'r.reference LIKE ?').join(' OR ')})
  )`);
  binds.push(...candidates);
}

export function withReferences(db: DatabaseSync, rows: MemoryDbRow[]): MemoryDbRow[] {
  if (rows.length === 0) return rows;
  const ids = rows.map(row => row.memory_id);
  const refs = db.prepare(
    `SELECT memory_id, reference
       FROM memory_refs
      WHERE memory_id IN (${ids.map(() => '?').join(',')})
      ORDER BY memory_id, ordinal`
  ).all(...ids) as unknown as Array<{ memory_id: string; reference: string }>;
  const map = new Map<string, string[]>();
  for (const ref of refs) {
    const list = map.get(ref.memory_id) ?? [];
    list.push(ref.reference);
    map.set(ref.memory_id, list);
  }
  for (const row of rows) row.references = map.get(row.memory_id) ?? [];
  return rows;
}
