import { createHash } from 'node:crypto';
import { existsSync, lstatSync, mkdirSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { isAbsolute, join, relative, resolve } from 'node:path';
import type { DatabaseSync } from 'node:sqlite';
import { ATTEND_COMPACT_BUDGET, AwarenessQueryParams, AwarenessQueryResult, AwarenessQueryRow, PROJECTION_MARKDOWN_BUDGETS, ProjectionMarkdownBudgetStatus, RepoContextInjectParams, RepoContextInjectResult, SCOPE_CACHE, WORKBOARD_BUDGET, limitOf, normalizeMode, utcNow } from './repo-model.js';
import { resolveDbPath } from './db-runtime.js';
import { queryAwareness } from './repo-query.js';
import { scopeFromParams } from './repo-scope.js';
import { renderRepoAgentsMd, renderRowsDoc } from './repo-docs.js';
import { gitCheckIgnored, lineCount } from './repo-formats.js';

export function resolveWorkspaceOutputPath(output: string | null | undefined, workspacePath: string, defaultPath: string): string {
  const target = output?.trim() || defaultPath;
  return isAbsolute(target) ? resolve(target) : resolve(workspacePath, target);
}

export let atomicWriteSequence = 0;

export function atomicWriteText(path: string, content: string): void {
  mkdirSync(join(path, '..'), { recursive: true });
  const temp = `${path}.tmp-${process.pid}-${Date.now()}-${atomicWriteSequence++}`;
  try {
    writeFileSync(temp, content, { encoding: 'utf8', flag: 'wx' });
    renameSync(temp, path);
  } finally {
    rmSync(temp, { force: true });
  }
}

export function sanitizeShareString(value: string, workspacePath: string): string {
  const sanitizeAbsolute = (token: string): string => {
    const filePrefix = token.startsWith('file:') ? 'file:' : '';
    const raw = filePrefix ? token.slice(filePrefix.length) : token;
    if (!isAbsolute(raw)) return token;
    if (raw === workspacePath || raw.startsWith(`${workspacePath}/`)) {
      return `${filePrefix}${relative(workspacePath, raw) || '.'}`;
    }
    return filePrefix ? 'file:<external-path-redacted>' : '<absolute-path-redacted>';
  };

  const redactSecrets = (text: string): string => text
    .replace(/-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----/g, '<redacted-secret>')
    .replace(/\b(?:ghp_|github_pat_|sk-)[A-Za-z0-9_-]{16,}\b/g, '<redacted-secret>')
    .replace(/\bAKIA[A-Z0-9]{16}\b/g, '<redacted-secret>')
    .replace(/\b(api[_-]?key|token|password|secret)\s*[:=]\s*[^\s,;]{8,}/gi, '$1=<redacted-secret>');

  if (value.startsWith('file:') || isAbsolute(value)) return redactSecrets(sanitizeAbsolute(value));
  const withoutWorkspace = value.split(workspacePath).join('<workspace>');
  return redactSecrets(withoutWorkspace.replace(
    /(?:file:)?\/(?:Users|home|private|tmp|var|Volumes|opt)\/[^\s,;)"'\]]+/g,
    sanitizeAbsolute,
  ));
}

export function sanitizeShareRow(row: AwarenessQueryRow, workspacePath: string): AwarenessQueryRow {
  const sanitized: AwarenessQueryRow = {};
  for (const [key, value] of Object.entries(row)) {
    if (key === 'body' || (key === 'detail' && (row['item_type'] === 'signal' || row['kind'] === 'signal'))) {
      if (value) sanitized['body_redacted'] = true;
      continue;
    }
    if (value == null || value === '' || (Array.isArray(value) && value.length === 0)) continue;
    if (typeof value === 'string') {
      sanitized[key] = sanitizeShareString(value, workspacePath);
    } else if (Array.isArray(value)) {
      sanitized[key] = value.map(item => sanitizeShareString(String(item), workspacePath));
    } else {
      sanitized[key] = value;
    }
  }
  return sanitized;
}

export function sanitizeQueryResultForShare(
  result: AwarenessQueryResult,
  workspacePath: string,
): AwarenessQueryResult {
  const sections = result.sections
    ? Object.fromEntries(Object.entries(result.sections).map(([name, section]) => [name, {
      ...section,
      rows: section.rows.map(row => sanitizeShareRow(row, workspacePath)),
    }]))
    : undefined;
  return {
    ...result,
    workspace_path: '<workspace>',
    rows: result.rows.map(row => sanitizeShareRow(row, workspacePath)),
    ...(sections ? { sections } : {}),
  };
}

export function projectionRevisionFromResult(result: AwarenessQueryResult): string {
  const sections = Object.fromEntries(
    Object.entries(result.sections ?? {})
      // Workboard contains a synthetic generated timestamp. Its underlying DB
      // rows and counts are represented by the stable source sections.
      .filter(([name]) => name !== 'workboard')
      .map(([name, section]) => [name, {
        rows: section.rows,
        count: section.count,
        total: section.total,
        omitted_count: section.omitted_count,
        is_partial: section.is_partial,
      }]),
  );
  const digest = createHash('sha256').update(JSON.stringify({
    workspace_path: result.workspace_path,
    artifact: result.artifact,
    repo: result.repo,
    ref: result.ref,
    sections,
  })).digest('hex');
  return `sha256:${digest}`;
}

/** Fingerprint exactly the bounded live sections that would feed repo inject. */
export function projectionSourceRevision(
  db: DatabaseSync,
  params: AwarenessQueryParams = {},
): string {
  return projectionRevisionFromResult(queryAwareness(db, {
    ...params,
    view: 'all',
    limit: limitOf(params.limit, 50, 500),
  }));
}

export interface PreviousProjectionManifest {
  generator?: string;
  files?: string[];
  orphan_cleanup?: { candidates?: string[] };
}

// Exact Awareness-owned artifacts retired before projection manifests existed.
// Delete silently during projection; unknown files remain user-owned.
const RETIRED_AWARENESS_ARTIFACTS = ['.agent-id'] as const;

export function previousProjectionManifest(outDir: string): PreviousProjectionManifest | null {
  const path = join(outDir, 'awareness', 'manifest.json');
  if (!existsSync(path)) return null;
  try {
    const parsed = JSON.parse(readFileSync(path, 'utf8')) as PreviousProjectionManifest;
    return parsed.generator === '@octocodeai/octocode-awareness repo inject' ? parsed : null;
  } catch {
    return null;
  }
}

export function isInside(root: string, candidate: string): boolean {
  const rel = relative(root, candidate);
  return rel === '' || (!rel.startsWith('..') && !isAbsolute(rel));
}

export function priorOwnedPath(file: string, workspacePath: string, outDir: string): string | null {
  const workspaceRelative = resolve(workspacePath, file);
  if (isInside(outDir, workspaceRelative)) return workspaceRelative;
  const outputRelative = resolve(outDir, file);
  return isInside(outDir, outputRelative) ? outputRelative : null;
}

export function injectRepoContext(db: DatabaseSync, params: RepoContextInjectParams = {}): RepoContextInjectResult {
  const scope = scopeFromParams(params);
  const workspacePath = scope.workspacePath ?? process.cwd();
  const rawOutDir = params.outDir ?? params.out_dir;
  const outDir = resolveWorkspaceOutputPath(rawOutDir, workspacePath, join(workspacePath, '.octocode'));
  const mode = normalizeMode(params.mode);
  const includeView = params.includeView ?? params.include_view ?? false;
  const pruneOrphans = params.pruneOrphans ?? params.prune_orphans ?? false;
  const check = params.check ?? true;
  const previousManifest = previousProjectionManifest(outDir);
  const queryParams: AwarenessQueryParams = {
    ...params,
    workspacePath,
    limit: limitOf(params.limit, 50, 500),
    view: 'all',
  };
  SCOPE_CACHE.set(queryParams, scope);
  const queried = queryAwareness(db, queryParams);
  const all = mode === 'share' ? sanitizeQueryResultForShare(queried, workspacePath) : queried;
  const filesWritten: string[] = [];
  const writtenContent: Record<string, string> = {};
  const warnings: string[] = [];

  function write(relPath: string, content: string): void {
    const full = join(outDir, relPath);
    atomicWriteText(full, content);
    writtenContent[relPath] = content;
    filesWritten.push(full);
  }

  const sections = all.sections ?? {};
  const counts = Object.fromEntries(Object.entries(sections).map(([name, section]) => [name, section.count]));
  const completeness = Object.fromEntries(Object.entries(sections).map(([name, section]) => [name, {
    visible: section.count,
    total: section.total,
    omitted_count: section.omitted_count,
    is_partial: section.is_partial,
    continuation: section.continuation,
  }]));
  for (const [name, section] of Object.entries(sections)) {
    if (section.is_partial) warnings.push(`projection section ${name} is partial: ${section.continuation ?? 'narrow the query'}`);
  }
  const profileCounts = Object.fromEntries(
    (sections['repo-profile']?.rows ?? []).map(row => [String(row['metric']), Number(row['count'] ?? 0)]),
  );
  write('AGENTS.md', renderRepoAgentsMd(all));
  const knowledgeCandidates = [
    ...(sections['gotchas']?.rows ?? []),
    ...(sections['lessons']?.rows ?? []),
    ...(sections['memories']?.rows ?? []),
    ...(sections['developer-review']?.rows ?? []),
  ];
  const knowledgeRows = [...new Map(knowledgeCandidates.map((row, index) => [
    String(row['memory_id'] ?? row['refinement_id'] ?? row['id'] ?? `row:${index}`),
    row,
  ])).values()];
  if (knowledgeRows.length > 0) {
    const projectedKnowledge = knowledgeRows.map(row => sanitizeShareRow(row, workspacePath));
    write('KNOWLEDGE.md', renderRowsDoc(
      'Octocode Knowledge',
      projectedKnowledge,
      'Bounded knowledge leads from Awareness. SQLite is canonical; verify current evidence before acting.',
      PROJECTION_MARKDOWN_BUDGETS['KNOWLEDGE.md']!.max_lines,
      Math.max(knowledgeRows.length, Number(profileCounts['active_memories'] ?? 0), Number(profileCounts['developer_review'] ?? 0)),
    ));
  }
  if (includeView) warnings.push('include-view is retired for wiki sync; use an explicit query --format html export');
  if (check) {
    const ignored = gitCheckIgnored(workspacePath, outDir);
    if (ignored.ignored) {
      warnings.push(`generated path is gitignored: ${relative(workspacePath, outDir) || outDir}; remove the ignore intentionally if this repo should share .octocode`);
    }
    if (mode === 'share' && ignored.ignored) {
      warnings.push('mode=share requested, but git currently ignores the generated .octocode path');
    }
  }

  const projectionBudgets: Record<string, ProjectionMarkdownBudgetStatus> = Object.fromEntries(Object.entries(PROJECTION_MARKDOWN_BUDGETS).map(([relPath, budget]) => {
    const actualLines = lineCount(writtenContent[relPath] ?? '');
    return [relPath, {
      ...budget,
      actual_lines: actualLines,
      within_budget: actualLines <= budget.max_lines,
    }];
  }));
  for (const [relPath, budget] of Object.entries(projectionBudgets)) {
    if (!budget.within_budget) warnings.push(`projection budget exceeded: ${relPath} has ${budget.actual_lines}/${budget.max_lines} lines`);
  }

  const generatedAt = utcNow();
  const manifestRelPath = join('awareness', 'manifest.json');
  const manifestPath = join(outDir, manifestRelPath);
  const currentManaged = new Set([...filesWritten, manifestPath].map(file => resolve(file)));
  const previousOwned = (previousManifest?.files ?? [])
    .map(file => priorOwnedPath(file, workspacePath, outDir))
    .filter((file): file is string => Boolean(file));
  previousOwned.push(...(previousManifest?.orphan_cleanup?.candidates ?? [])
    .map(file => priorOwnedPath(file, workspacePath, outDir))
    .filter((file): file is string => Boolean(file)));
  // Known output retired before manifest-owned cleanup existed.
  previousOwned.push(join(outDir, 'awareness', 'csv', 'all.csv'));
  for (const retired of ['MEMORY.md', 'GOTCHAS.md', 'LEARN.md', 'BOOKMARKS.md', 'DEVELOPER_REVIEW.md', join('awareness', 'index.html'), join('references', 'repo-map.md')]) {
    previousOwned.push(join(outDir, retired));
  }
  for (const view of ['memories', 'gotchas', 'lessons', 'plans', 'tasks', 'runs', 'agents', 'locks', 'signals', 'refinements', 'files', 'activity', 'workboard']) {
    previousOwned.push(join(outDir, 'awareness', 'csv', `${view}.csv`));
  }
  for (const retired of ['commands.md', 'testing.md', 'architecture.md']) {
    previousOwned.push(join(outDir, 'references', retired));
  }
  const prunedOrphans: string[] = [];
  for (const retired of RETIRED_AWARENESS_ARTIFACTS) {
    const file = join(outDir, retired);
    if (!existsSync(file)) continue;
    try {
      const entry = lstatSync(file);
      if (!entry.isFile() && !entry.isSymbolicLink()) continue;
      rmSync(file, { force: true });
    } catch (error) {
      warnings.push(`could not remove retired Awareness artifact ${relative(workspacePath, file)}: ${String(error)}`);
    }
  }
  const orphanCandidates = [...new Set(previousOwned.map(file => resolve(file)))]
    .filter(file => isInside(outDir, file))
    .filter(file => !currentManaged.has(file))
    .filter(file => !relative(outDir, file).replace(/\\/g, '/').startsWith('plan/'))
    .filter(file => existsSync(file))
    .sort();
  if (pruneOrphans) {
    for (const file of orphanCandidates) {
      try {
        const entry = lstatSync(file);
        if (!entry.isFile() && !entry.isSymbolicLink()) continue;
        rmSync(file, { force: true });
        prunedOrphans.push(file);
      } catch (error) {
        warnings.push(`could not prune retired projection ${relative(workspacePath, file)}: ${String(error)}`);
      }
    }
  } else if (orphanCandidates.length > 0) {
    warnings.push(`${orphanCandidates.length} retired Awareness-owned projection artifact(s) found; rerun wiki sync with --prune-orphans after review`);
  }
  const sourceRevision = projectionRevisionFromResult(queried);
  const manifestFiles = [
    ...filesWritten.map(file => relative(workspacePath, file)),
    relative(workspacePath, manifestPath),
  ];
  const manifest = {
    generated_at: generatedAt,
    generator: '@octocodeai/octocode-awareness repo inject',
    mode,
    workspace_path: mode === 'share' ? '<workspace>' : workspacePath,
    artifact: scope.artifact,
    repo: scope.repo,
    ref: scope.ref,
    source: {
      canonical: params.dbPath ?? resolveDbPath(),
      projection: '.octocode',
      revision: sourceRevision,
      revision_algorithm: 'sha256:bounded-live-sections-v1',
    },
    policy: {
      gitignore_modified: false,
      share_decision: 'user-owned',
    },
    counts,
    completeness,
    budgets: {
      markdown: projectionBudgets,
      workboard: WORKBOARD_BUDGET,
      attend_compact: ATTEND_COMPACT_BUDGET,
    },
    files: manifestFiles,
    orphan_cleanup: {
      candidates: orphanCandidates.map(file => relative(workspacePath, file)),
      pruned: prunedOrphans.map(file => relative(workspacePath, file)),
    },
    warnings,
  };
  write(manifestRelPath, JSON.stringify(manifest, null, 2) + '\n');

  return {
    ok: true,
    generated_at: generatedAt,
    workspace_path: workspacePath,
    out_dir: outDir,
    mode,
    count: filesWritten.length,
    files: filesWritten,
    warnings,
    orphan_candidates: orphanCandidates,
    pruned_orphans: prunedOrphans,
    manifest,
  };
}
