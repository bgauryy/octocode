import { writeCommandText } from '../command-output.js';
/** Shared repository command handlers; adapters own process and shell behavior. */
import { writeFileSync, mkdirSync } from 'node:fs';
import { dirname, isAbsolute, resolve } from 'node:path';
import type { DatabaseSync } from 'node:sqlite';
import { mineDocStaleness, proposeDocRefresh } from '../docs.js';
import { listSkillDocs, showSkillDoc } from '../docs-catalog.js';
import { attendWorkspace } from '../attend-presence.js';
import { formatAwarenessQueryResult, queryAwareness } from '../repo-query.js';
import { ParsedArgs } from './args.js';
import { EmitOptions, emit } from '../command-output.js';
import { flagBool, resolveAgentId } from './args.js';

export function cmdQuery(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const view = String(args['view'] ?? args._[0] ?? 'all');
  const format = String(args['format'] ?? 'json').toLowerCase();
  const workspacePath = args['workspace'] ? String(args['workspace']) : process.cwd();
  const result = queryAwareness(db, {
    view,
    workspacePath,
    artifact: args['artifact'] ? String(args['artifact']) : null,
    repo: args['repo'] ? String(args['repo']) : null,
    ref: args['ref'] ? String(args['ref']) : null,
    query: args['query'] ? String(args['query']) : null,
    limit: args['limit']
      ? parseInt(String(args['limit']), 10)
      : opts.compact ? (view === 'workboard' ? 1 : 5) : undefined,
    agentId: args['agent_id'] ? String(args['agent_id']) : null,
    state: Array.isArray(args['state']) ? args['state'].map(String) : args['state'] ? String(args['state']) : null,
    label: Array.isArray(args['label']) ? args['label'].map(String) : args['label'] ? String(args['label']) : null,
    file: args['file'] ? String(Array.isArray(args['file']) ? args['file'][0] : args['file']) : null,
    since: args['since'] ? String(args['since']) : null,
    includeBodies: flagBool(args['include_bodies']),
  });

  const outPath = args['out'] ? String(args['out']) : null;
  if (outPath) {
    const resolvedOutPath = isAbsolute(outPath) ? resolve(outPath) : resolve(workspacePath, outPath);
    mkdirSync(dirname(resolvedOutPath), { recursive: true });
    writeFileSync(resolvedOutPath, formatAwarenessQueryResult(result, format), 'utf8');
    return emit({ db_path: dbPath, path: resolvedOutPath, view: result.view, count: result.count }, 0, opts);
  }

  if (opts.compact && format === 'json' && result.count === 0) {
    return emit({ view: result.view, count: 0, rows: [] }, 0, opts);
  }
  if (format === 'json') return emit({ db_path: dbPath, ...result }, 0, opts);
  writeCommandText(formatAwarenessQueryResult(result, format));
  return 0;
}

export function cmdAttend(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const rawFile = args['file'];
  const files = Array.isArray(rawFile) ? rawFile.map(String) : rawFile ? [String(rawFile)] : [];
  const result = attendWorkspace(db, {
    details: flagBool(args['details']),
    changes: flagBool(args['changes']),
    offset: args['offset'] === undefined ? undefined : Number(args['offset']),
    revision: args['revision'] === undefined ? undefined : String(args['revision']),
    agentId: String(args['agent_id'] ?? '').trim() || undefined,
    workspacePath: args['workspace'] ? String(args['workspace']) : process.cwd(),
    artifact: args['artifact'] ? String(args['artifact']) : null,
    repo: args['repo'] ? String(args['repo']) : null,
    ref: args['ref'] ? String(args['ref']) : null,
    query: args['query'] ? String(args['query']) : null,
    limit: args['limit'] ? parseInt(String(args['limit']), 10) : undefined,
    file: files,
    includeBodies: flagBool(args['include_bodies']),
    explainOrgan: flagBool(args['explain_organ']),
    compact: opts.compact,
  });
  return emit({ db_path: dbPath, ...result }, result.ok ? 0 : 1, opts);
}

export function cmdDocsCatalog(args: ParsedArgs, opts: EmitOptions): number {
  const action = String(args['action'] ?? args._[0] ?? 'list').trim().toLowerCase();
  if (action === 'list') {
    const full = Boolean(args['full']);
    if (!full) {
      const result = listSkillDocs({ lean: true });
      return emit({
        ok: true,
        count: result.count,
        docs: result.docs,
        next: 'npx @octocodeai/octocode-awareness docs show <name>',
      }, 0, opts);
    }
    const result = listSkillDocs();
    return emit({
      ok: true,
      count: result.count,
      root: result.root,
      docs: result.docs.map((doc) => ({
        name: doc.name,
        title: doc.title,
        description: doc.description,
        kind: doc.kind,
        path: doc.path,
      })),
      next: 'npx @octocodeai/octocode-awareness docs show <name>',
    }, 0, opts);
  }
  if (action === 'show') {
    const name = String(args['name'] ?? args._[0] ?? '').trim();
    if (!name) return emit({ ok: false, error: 'docs show requires a name. Run docs list --compact.' }, 1, opts);
    const result = showSkillDoc(name);
    if (!result.ok) {
      return emit({ ok: false, error: result.error, suggestions: result.suggestions }, 1, opts);
    }
    if (opts.compact) {
      return emit({
        ok: true,
        name: result.name,
        title: result.title,
        description: result.description,
        kind: result.kind,
        path: result.path,
        content: result.content,
      }, 0, opts);
    }
    writeCommandText(`${result.content}${result.content.endsWith('\n') ? '' : '\n'}`);
    return 0;
  }
  return emit({ ok: false, error: `unknown docs action "${action}". Use docs list|show|staleness.` }, 1, opts);
}

export function cmdDocStaleness(db: DatabaseSync, args: ParsedArgs, dbPath: string, opts: EmitOptions): number {
  const rawTargets = args['targets_json'];
  if (!rawTargets || typeof rawTargets !== 'string') {
    return emit({ error: '--targets-json is required, e.g. \'[{"docFile":"pkg/ARCHITECTURE.md","sourceDirs":["pkg/src"]}]\'' }, 1, opts);
  }
  let targets: Array<{ docFile: string; sourceDirs: string[] }>;
  try {
    const parsed = JSON.parse(rawTargets) as unknown;
    if (!Array.isArray(parsed)) throw new Error('not an array');
    targets = parsed.map((t) => {
      const obj = t as { docFile?: unknown; doc_file?: unknown; sourceDirs?: unknown; source_dirs?: unknown };
      const docFile = String(obj.docFile ?? obj.doc_file ?? '');
      const rawDirs = obj.sourceDirs ?? obj.source_dirs;
      const sourceDirs = Array.isArray(rawDirs) ? rawDirs.map(String) : [];
      if (!docFile || sourceDirs.length === 0) throw new Error('each target needs docFile and sourceDirs');
      return { docFile, sourceDirs };
    });
  } catch (err) {
    return emit({ error: `--targets-json is invalid: ${(err as Error).message}` }, 1, opts);
  }

  const workspacePath = args['workspace'] ? String(args['workspace']) : null;
  const artifact = args['artifact'] ? String(args['artifact']) : null;
  const result = mineDocStaleness(db, {
    targets,
    workspacePath,
    artifact,
    minEditsSinceSync: args['min_edits'] ? Number(args['min_edits']) : undefined,
    minLinesSinceSync: args['min_lines'] ? Number(args['min_lines']) : undefined,
  });

  const proposed: Array<{ target_file: string; harness_id: string }> = [];
  if (Boolean(args['propose'])) {
    const agentId = resolveAgentId(args);
    const sessionId = args['session_id'] ? String(args['session_id']) : null;
    for (const entry of result.entries) {
      if (!entry.stale) continue;
      const harnessId = proposeDocRefresh(db, entry, { agentId, sessionId, workspacePath, artifact });
      proposed.push({ target_file: entry.doc_file, harness_id: harnessId });
    }
  }

  return emit({ db_path: dbPath, ...result, proposed }, 0, opts);
}
