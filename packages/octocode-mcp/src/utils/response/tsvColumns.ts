/**
 * Per-tool TSV column projections.
 *
 * Each remote tool defines (a) the column header order it wants in TSV mode
 * and (b) a function that walks the tool's `data` payload and yields one row
 * per logical record. Finalizers / bulk.ts consult this registry when a
 * caller passes `format: "tsv"`.
 *
 * Keeping the registry in one place lets us audit token shapes without
 * grepping six tools and prevents per-tool drift in column naming.
 */

import { STATIC_TOOL_NAMES } from '../../tools/toolNames.js';

export type TsvProjection = {
  columns: readonly string[];
  toRows: (data: unknown) => ReadonlyArray<Record<string, unknown>>;
};

function arr(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function obj(value: unknown): Record<string, unknown> {
  return value && typeof value === 'object'
    ? (value as Record<string, unknown>)
    : {};
}

// ---------------------------------------------------------------------------
// githubSearchCode — flatten owner/repo groups into one row per match
// ---------------------------------------------------------------------------
const searchCodeProjection: TsvProjection = {
  columns: ['repo', 'path', 'value'],
  toRows: data => {
    const groups = arr((data as { results?: unknown }).results);
    const rows: Array<Record<string, unknown>> = [];
    for (const g of groups) {
      const group = obj(g);
      const repo = `${group.owner ?? ''}/${group.repo ?? ''}`;
      for (const m of arr(group.matches)) {
        const match = obj(m);
        rows.push({
          repo,
          path: match.path ?? '',
          value: match.value ?? '',
        });
      }
    }
    return rows;
  },
};

// ---------------------------------------------------------------------------
// githubGetFileContent — one row per file; content stays as a single cell
// ---------------------------------------------------------------------------
const fetchContentProjection: TsvProjection = {
  columns: [
    'repo',
    'path',
    'startLine',
    'endLine',
    'totalLines',
    'lastModifiedBy',
    'content',
  ],
  toRows: data => {
    const groups = arr((data as { results?: unknown }).results);
    const rows: Array<Record<string, unknown>> = [];
    for (const g of groups) {
      const group = obj(g);
      const repo = `${group.owner ?? ''}/${group.repo ?? ''}`;
      for (const f of arr(group.files)) {
        const file = obj(f);
        rows.push({
          repo,
          path: file.path ?? '',
          startLine: file.startLine ?? '',
          endLine: file.endLine ?? '',
          totalLines: file.totalLines ?? '',
          lastModifiedBy: file.lastModifiedBy ?? '',
          content: file.content ?? '',
        });
      }
    }
    return rows;
  },
};

// ---------------------------------------------------------------------------
// githubSearchRepositories — one row per repository
// ---------------------------------------------------------------------------
const searchReposProjection: TsvProjection = {
  columns: [
    'owner',
    'repo',
    'stars',
    'language',
    'pushedAt',
    'forksCount',
    'openIssuesCount',
    'topics',
    'description',
  ],
  toRows: data => {
    const list = arr((data as { repositories?: unknown }).repositories);
    return list.map(r => {
      const row = obj(r);
      const topics = Array.isArray(row.topics) ? row.topics.join(' ') : '';
      return {
        owner: row.owner ?? '',
        repo: row.repo ?? '',
        stars: row.stars ?? '',
        language: row.language ?? '',
        pushedAt: row.pushedAt ?? '',
        forksCount: row.forksCount ?? '',
        openIssuesCount: row.openIssuesCount ?? '',
        topics,
        description: row.description ?? '',
      };
    });
  },
};

// ---------------------------------------------------------------------------
// githubSearchPullRequests — one row per PR; nested fields stay JSON
// ---------------------------------------------------------------------------
const searchPrsProjection: TsvProjection = {
  columns: [
    'number',
    'state',
    'author',
    'title',
    'createdAt',
    'updatedAt',
    'additions',
    'deletions',
    'changedFilesCount',
    'url',
    'mergedAt',
    'body',
    'draft',
    'assignees',
    'labels',
    'sourceBranch',
    'targetBranch',
    'sourceSha',
    'targetSha',
    'closedAt',
    'commentsCount',
    'comments',
    'fileChanges',
  ],
  toRows: data => {
    const list = arr((data as { pull_requests?: unknown }).pull_requests);
    return list.map(p => {
      const pr = obj(p);
      return {
        number: pr.number ?? '',
        state: pr.state ?? '',
        author: pr.author ?? '',
        title: pr.title ?? '',
        createdAt: pr.createdAt ?? '',
        updatedAt: pr.updatedAt ?? '',
        additions: pr.additions ?? '',
        deletions: pr.deletions ?? '',
        changedFilesCount: pr.changedFilesCount ?? '',
        url: pr.url ?? '',
        mergedAt: pr.mergedAt ?? '',
        body: pr.body ?? '',
        draft: pr.draft ?? '',
        assignees: pr.assignees ?? '',
        labels: pr.labels ?? '',
        sourceBranch: pr.sourceBranch ?? '',
        targetBranch: pr.targetBranch ?? '',
        sourceSha: pr.sourceSha ?? '',
        targetSha: pr.targetSha ?? '',
        closedAt: pr.closedAt ?? '',
        commentsCount: pr.commentsCount ?? '',
        comments: pr.comments ?? '',
        fileChanges: pr.fileChanges ?? '',
      };
    });
  },
};

// ---------------------------------------------------------------------------
// githubViewRepoStructure — flatten { path -> { files, folders } } map
// ---------------------------------------------------------------------------
const viewRepoStructureProjection: TsvProjection = {
  columns: ['parent', 'name', 'type'],
  toRows: data => {
    const tree = obj((data as { structure?: unknown }).structure);
    const rows: Array<Record<string, unknown>> = [];
    for (const [parent, node] of Object.entries(tree)) {
      const entry = obj(node);
      for (const name of arr(entry.files)) {
        rows.push({ parent, name: String(name), type: 'file' });
      }
      for (const name of arr(entry.folders)) {
        rows.push({ parent, name: String(name), type: 'dir' });
      }
    }
    return rows;
  },
};

// ---------------------------------------------------------------------------
// packageSearch — one row per package
// ---------------------------------------------------------------------------
const packageSearchProjection: TsvProjection = {
  columns: [
    'name',
    'version',
    'owner',
    'repo',
    'weeklyDownloads',
    'lastPublished',
    'license',
    'description',
  ],
  toRows: data => {
    const list = arr((data as { packages?: unknown }).packages);
    return list.map(p => {
      const pkg = obj(p);
      return {
        name: pkg.name ?? '',
        version: pkg.version ?? '',
        owner: pkg.owner ?? '',
        repo: pkg.repo ?? '',
        weeklyDownloads: pkg.weeklyDownloads ?? '',
        lastPublished: pkg.lastPublished ?? '',
        license: pkg.license ?? '',
        description: pkg.description ?? '',
      };
    });
  },
};

// ---------------------------------------------------------------------------
// localSearchCode (ripgrep) — flatten files[].matches[] into one row per hit
// ---------------------------------------------------------------------------
const localSearchCodeProjection: TsvProjection = {
  columns: ['path', 'line', 'column', 'value'],
  toRows: data => {
    const files = arr((data as { files?: unknown }).files);
    const rows: Array<Record<string, unknown>> = [];
    for (const f of files) {
      const file = obj(f);
      const path = file.path ?? '';
      for (const m of arr(file.matches)) {
        const match = obj(m);
        rows.push({
          path,
          line: match.line ?? '',
          column: match.column ?? '',
          value: match.value ?? '',
        });
      }
    }
    return rows;
  },
};

// ---------------------------------------------------------------------------
// localFindFiles — one row per file
// ---------------------------------------------------------------------------
const localFindFilesProjection: TsvProjection = {
  columns: ['path', 'type', 'size', 'modified', 'permissions'],
  toRows: data => {
    const files = arr((data as { files?: unknown }).files);
    return files.map(f => {
      const file = obj(f);
      return {
        path: file.path ?? '',
        type: file.type ?? '',
        size: file.size ?? '',
        modified: file.modified ?? '',
        permissions: file.permissions ?? '',
      };
    });
  },
};

// ---------------------------------------------------------------------------
// localViewStructure — one row per entry
// ---------------------------------------------------------------------------
const localViewStructureProjection: TsvProjection = {
  columns: ['name', 'type', 'size', 'modified', 'depth'],
  toRows: data => {
    const entries = arr((data as { entries?: unknown }).entries);
    return entries.map(e => {
      const entry = obj(e);
      return {
        name: entry.name ?? '',
        type: entry.type ?? '',
        size: entry.size ?? '',
        modified: entry.modified ?? '',
        depth: entry.depth ?? '',
      };
    });
  },
};

// ---------------------------------------------------------------------------
// localGetFileContent — single row (one file slice per query)
// ---------------------------------------------------------------------------
const localFetchContentProjection: TsvProjection = {
  columns: [
    'path',
    'startLine',
    'endLine',
    'totalLines',
    'isPartial',
    'content',
  ],
  toRows: data => {
    const d = obj(data);
    // Some empty paths only carry `totalLines`; render a single row anyway
    // so the agent gets a stable shape.
    if (typeof d.content !== 'string' && typeof d.totalLines !== 'number') {
      return [];
    }
    return [
      {
        path: d.path ?? '',
        startLine: d.startLine ?? '',
        endLine: d.endLine ?? '',
        totalLines: d.totalLines ?? '',
        isPartial: d.isPartial ?? '',
        content: d.content ?? '',
      },
    ];
  },
};

// ---------------------------------------------------------------------------
// LSP tools — flatten reference/definition/call locations
// ---------------------------------------------------------------------------
function locationRow(loc: Record<string, unknown>): Record<string, unknown> {
  const range = obj(loc.range);
  const start = obj(range.start);
  return {
    uri: loc.uri ?? loc.path ?? '',
    line: start.line ?? loc.line ?? '',
    column: start.character ?? start.column ?? loc.column ?? '',
    snippet: loc.snippet ?? loc.context ?? '',
  };
}

const lspGotoDefinitionProjection: TsvProjection = {
  columns: ['uri', 'line', 'column', 'snippet'],
  toRows: data => {
    const defs = arr(
      (data as { definitions?: unknown; locations?: unknown }).definitions ??
        (data as { locations?: unknown }).locations
    );
    return defs.map(d => locationRow(obj(d)));
  },
};

const lspFindReferencesProjection: TsvProjection = {
  columns: ['uri', 'line', 'column', 'snippet'],
  toRows: data => {
    const refs = arr(
      (data as { references?: unknown; locations?: unknown }).references ??
        (data as { locations?: unknown }).locations
    );
    return refs.map(r => locationRow(obj(r)));
  },
};

const lspCallHierarchyProjection: TsvProjection = {
  columns: ['direction', 'name', 'uri', 'line', 'column'],
  toRows: data => {
    const calls = arr((data as { calls?: unknown }).calls);
    return calls.map(c => {
      const call = obj(c);
      const node = obj(call.from ?? call.to ?? call.item);
      const range = obj(node.range ?? call.range);
      const start = obj(range.start);
      return {
        direction: call.direction ?? '',
        name: node.name ?? '',
        uri: node.uri ?? '',
        line: start.line ?? '',
        column: start.character ?? start.column ?? '',
      };
    });
  },
};

const PROJECTIONS: Record<string, TsvProjection> = {
  // Remote
  [STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE]: searchCodeProjection,
  [STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT]: fetchContentProjection,
  [STATIC_TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES]: searchReposProjection,
  [STATIC_TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS]: searchPrsProjection,
  [STATIC_TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE]: viewRepoStructureProjection,
  [STATIC_TOOL_NAMES.PACKAGE_SEARCH]: packageSearchProjection,
  // Local
  [STATIC_TOOL_NAMES.LOCAL_RIPGREP]: localSearchCodeProjection,
  [STATIC_TOOL_NAMES.LOCAL_FIND_FILES]: localFindFilesProjection,
  [STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE]: localViewStructureProjection,
  [STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT]: localFetchContentProjection,
  // LSP
  [STATIC_TOOL_NAMES.LSP_GOTO_DEFINITION]: lspGotoDefinitionProjection,
  [STATIC_TOOL_NAMES.LSP_FIND_REFERENCES]: lspFindReferencesProjection,
  [STATIC_TOOL_NAMES.LSP_CALL_HIERARCHY]: lspCallHierarchyProjection,
};

export function getTsvProjection(toolName: string): TsvProjection | undefined {
  return PROJECTIONS[toolName];
}
