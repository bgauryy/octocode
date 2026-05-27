/**
 * Per-tool TSV column projections.
 *
 * Each tool exports:
 *   1. a `<tool>Columns` constant — the full TSV field contract,
 *   2. a `<tool>Projection` — row extraction logic,
 *   3. a `<tool>ToTsv(data)` helper — direct TSV rendering for tests/CLI use.
 *
 * The generic bulk runner and custom finalizers use `getTsvProjection()` /
 * `exportToolDataToTsv()` so every tool shares one TSV implementation.
 */

import { STATIC_TOOL_NAMES } from '../../tools/toolNames.js';
import { tsvFormat } from './tsvFormat.js';

export type TsvProjection = {
  columns: readonly string[];
  toRows: (data: unknown) => ReadonlyArray<Record<string, unknown>>;
};

type TsvExport = {
  columns: readonly string[];
  rows: string;
};

function arr(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function obj(value: unknown): Record<string, unknown> {
  return value && typeof value === 'object'
    ? (value as Record<string, unknown>)
    : {};
}

function scalar(value: unknown): unknown {
  if (Array.isArray(value)) return value;
  if (value && typeof value === 'object') return value;
  return value ?? '';
}

function renderTsv(projection: TsvProjection, data: unknown): TsvExport {
  const rows = projection.toRows(data);
  return {
    columns: projection.columns,
    rows: tsvFormat(projection.columns, rows),
  };
}

function locationLineColumn(loc: Record<string, unknown>): {
  line: unknown;
  column: unknown;
} {
  const range = obj(loc.range);
  const start = obj(range.start);
  return {
    line: start.line ?? loc.line ?? '',
    column: start.character ?? start.column ?? loc.column ?? '',
  };
}

// ---------------------------------------------------------------------------
// githubSearchCode — flatten owner/repo groups into one row per match
// ---------------------------------------------------------------------------
export const githubSearchCodeColumns = [
  'id',
  'owner',
  'repo',
  'path',
  'value',
] as const;

export const githubSearchCodeProjection: TsvProjection = {
  columns: githubSearchCodeColumns,
  toRows: data => {
    const groups = arr((data as { results?: unknown }).results);
    const rows: Array<Record<string, unknown>> = [];
    for (const g of groups) {
      const group = obj(g);
      for (const m of arr(group.matches)) {
        const match = obj(m);
        rows.push({
          id: scalar(group.id),
          owner: scalar(group.owner),
          repo: scalar(group.repo),
          path: scalar(match.path),
          value: scalar(match.value),
        });
      }
    }
    return rows;
  },
};

export function githubSearchCodeToTsv(data: unknown): TsvExport {
  return renderTsv(githubSearchCodeProjection, data);
}

// ---------------------------------------------------------------------------
// githubGetFileContent — one row per file or downloaded directory entry
// ---------------------------------------------------------------------------
export const githubFetchContentColumns = [
  'id',
  'owner',
  'repo',
  'path',
  'content',
  'totalLines',
  'resolvedBranch',
  'isPartial',
  'startLine',
  'endLine',
  'lastModified',
  'lastModifiedBy',
  'warnings',
  'localPath',
  'fileCount',
  'totalSize',
  'size',
  'type',
  'cached',
] as const;

export const githubFetchContentProjection: TsvProjection = {
  columns: githubFetchContentColumns,
  toRows: data => {
    const groups = arr((data as { results?: unknown }).results);
    const rows: Array<Record<string, unknown>> = [];
    for (const g of groups) {
      const group = obj(g);
      for (const f of arr(group.files)) {
        const file = obj(f);
        rows.push({
          id: scalar(group.id),
          owner: scalar(group.owner),
          repo: scalar(group.repo),
          path: scalar(file.path),
          content: scalar(file.content),
          totalLines: scalar(file.totalLines),
          resolvedBranch: scalar(file.resolvedBranch),
          isPartial: scalar(file.isPartial),
          startLine: scalar(file.startLine),
          endLine: scalar(file.endLine),
          lastModified: scalar(file.lastModified),
          lastModifiedBy: scalar(file.lastModifiedBy),
          warnings: scalar(file.warnings),
          localPath: '',
          fileCount: '',
          totalSize: '',
          size: '',
          type: '',
          cached: '',
        });
      }
      for (const d of arr(group.directories)) {
        const dir = obj(d);
        const files = arr(dir.files);
        if (files.length === 0) {
          rows.push({
            id: scalar(group.id),
            owner: scalar(group.owner),
            repo: scalar(group.repo),
            path: scalar(dir.path),
            content: '',
            totalLines: '',
            resolvedBranch: scalar(dir.resolvedBranch),
            isPartial: '',
            startLine: '',
            endLine: '',
            lastModified: '',
            lastModifiedBy: '',
            warnings: '',
            localPath: scalar(dir.localPath),
            fileCount: scalar(dir.fileCount),
            totalSize: scalar(dir.totalSize),
            size: '',
            type: '',
            cached: scalar(dir.cached),
          });
          continue;
        }
        for (const nested of files) {
          const file = obj(nested);
          rows.push({
            id: scalar(group.id),
            owner: scalar(group.owner),
            repo: scalar(group.repo),
            path: scalar(file.path ?? dir.path),
            content: '',
            totalLines: '',
            resolvedBranch: scalar(dir.resolvedBranch),
            isPartial: '',
            startLine: '',
            endLine: '',
            lastModified: '',
            lastModifiedBy: '',
            warnings: '',
            localPath: scalar(dir.localPath),
            fileCount: scalar(dir.fileCount),
            totalSize: scalar(dir.totalSize),
            size: scalar(file.size),
            type: scalar(file.type),
            cached: scalar(dir.cached),
          });
        }
      }
    }
    return rows;
  },
};

export function githubFetchContentToTsv(data: unknown): TsvExport {
  return renderTsv(githubFetchContentProjection, data);
}

// ---------------------------------------------------------------------------
// githubSearchRepositories — one row per repository
// ---------------------------------------------------------------------------
export const githubSearchRepositoriesColumns = [
  'owner',
  'repo',
  'name',
  'fullName',
  'description',
  'url',
  'stars',
  'forks',
  'forksCount',
  'openIssuesCount',
  'language',
  'topics',
  'defaultBranch',
  'createdAt',
  'updatedAt',
  'pushedAt',
  'size',
  'archived',
  'private',
] as const;

export const githubSearchRepositoriesProjection: TsvProjection = {
  columns: githubSearchRepositoriesColumns,
  toRows: data => {
    const list = arr((data as { repositories?: unknown }).repositories);
    return list.map(r => {
      const row = obj(r);
      return {
        owner: scalar(row.owner),
        repo: scalar(row.repo),
        name: scalar(row.name),
        fullName: scalar(row.fullName ?? row.full_name),
        description: scalar(row.description),
        url: scalar(row.url ?? row.htmlUrl),
        stars: scalar(row.stars ?? row.stargazersCount),
        forks: scalar(row.forks),
        forksCount: scalar(row.forksCount),
        openIssuesCount: scalar(row.openIssuesCount),
        language: scalar(row.language),
        topics: scalar(row.topics),
        defaultBranch: scalar(row.defaultBranch),
        createdAt: scalar(row.createdAt),
        updatedAt: scalar(row.updatedAt),
        pushedAt: scalar(row.pushedAt),
        size: scalar(row.size),
        archived: scalar(row.archived),
        private: scalar(row.private),
      };
    });
  },
};

export function githubSearchRepositoriesToTsv(data: unknown): TsvExport {
  return renderTsv(githubSearchRepositoriesProjection, data);
}

// ---------------------------------------------------------------------------
// githubSearchPullRequests — one row per PR, including nested fields as JSON
// ---------------------------------------------------------------------------
export const githubSearchPullRequestsColumns = [
  'number',
  'state',
  'draft',
  'author',
  'title',
  'body',
  'createdAt',
  'updatedAt',
  'closedAt',
  'mergedAt',
  'additions',
  'deletions',
  'changedFilesCount',
  'url',
  'assignees',
  'labels',
  'sourceBranch',
  'targetBranch',
  'sourceSha',
  'targetSha',
  'commentsCount',
  'comments',
  'fileChanges',
] as const;

export const githubSearchPullRequestsProjection: TsvProjection = {
  columns: githubSearchPullRequestsColumns,
  toRows: data => {
    const list = arr((data as { pull_requests?: unknown }).pull_requests);
    return list.map(p => {
      const pr = obj(p);
      return {
        number: scalar(pr.number),
        state: scalar(pr.state),
        draft: scalar(pr.draft),
        author: scalar(pr.author),
        title: scalar(pr.title),
        body: scalar(pr.body),
        createdAt: scalar(pr.createdAt),
        updatedAt: scalar(pr.updatedAt),
        closedAt: scalar(pr.closedAt),
        mergedAt: scalar(pr.mergedAt),
        additions: scalar(pr.additions),
        deletions: scalar(pr.deletions),
        changedFilesCount: scalar(pr.changedFilesCount),
        url: scalar(pr.url),
        assignees: scalar(pr.assignees),
        labels: scalar(pr.labels),
        sourceBranch: scalar(pr.sourceBranch),
        targetBranch: scalar(pr.targetBranch),
        sourceSha: scalar(pr.sourceSha),
        targetSha: scalar(pr.targetSha),
        commentsCount: scalar(pr.commentsCount),
        comments: scalar(pr.comments),
        fileChanges: scalar(pr.fileChanges),
      };
    });
  },
};

export function githubSearchPullRequestsToTsv(data: unknown): TsvExport {
  return renderTsv(githubSearchPullRequestsProjection, data);
}

// ---------------------------------------------------------------------------
// githubViewRepoStructure — flatten { path -> { files, folders } } map
// ---------------------------------------------------------------------------
export const githubViewRepoStructureColumns = [
  'parent',
  'name',
  'type',
  'path',
  'size',
  'sha',
  'url',
] as const;

export const githubViewRepoStructureProjection: TsvProjection = {
  columns: githubViewRepoStructureColumns,
  toRows: data => {
    const tree = obj((data as { structure?: unknown }).structure);
    const rows: Array<Record<string, unknown>> = [];
    for (const [parent, node] of Object.entries(tree)) {
      const entry = obj(node);
      for (const rawFile of arr(entry.files)) {
        const file = obj(rawFile);
        const isObject = Object.keys(file).length > 0;
        const name = isObject ? file.name : rawFile;
        rows.push({
          parent,
          name: scalar(name),
          type: 'file',
          path: scalar(file.path),
          size: scalar(file.size),
          sha: scalar(file.sha),
          url: scalar(file.url),
        });
      }
      for (const rawFolder of arr(entry.folders)) {
        const folder = obj(rawFolder);
        const isObject = Object.keys(folder).length > 0;
        const name = isObject ? folder.name : rawFolder;
        rows.push({
          parent,
          name: scalar(name),
          type: 'dir',
          path: scalar(folder.path),
          size: scalar(folder.size),
          sha: scalar(folder.sha),
          url: scalar(folder.url),
        });
      }
    }
    return rows;
  },
};

export function githubViewRepoStructureToTsv(data: unknown): TsvExport {
  return renderTsv(githubViewRepoStructureProjection, data);
}

// ---------------------------------------------------------------------------
// githubCloneRepo — one row per local checkout
// ---------------------------------------------------------------------------
export const githubCloneRepoColumns = [
  'localPath',
  'resolvedBranch',
  'cached',
] as const;

export const githubCloneRepoProjection: TsvProjection = {
  columns: githubCloneRepoColumns,
  toRows: data => {
    const d = obj(data);
    if (typeof d.localPath !== 'string') return [];
    return [
      {
        localPath: scalar(d.localPath),
        resolvedBranch: scalar(d.resolvedBranch),
        cached: scalar(d.cached),
      },
    ];
  },
};

export function githubCloneRepoToTsv(data: unknown): TsvExport {
  return renderTsv(githubCloneRepoProjection, data);
}

// ---------------------------------------------------------------------------
// packageSearch — one row per package
// ---------------------------------------------------------------------------
export const packageSearchColumns = [
  'name',
  'version',
  'description',
  'owner',
  'repo',
  'repositoryUrl',
  'homepage',
  'weeklyDownloads',
  'lastPublished',
  'license',
  'keywords',
  'score',
  'searchScore',
] as const;

export const packageSearchProjection: TsvProjection = {
  columns: packageSearchColumns,
  toRows: data => {
    const list = arr((data as { packages?: unknown }).packages);
    return list.map(p => {
      const pkg = obj(p);
      return {
        name: scalar(pkg.name),
        version: scalar(pkg.version),
        description: scalar(pkg.description),
        owner: scalar(pkg.owner),
        repo: scalar(pkg.repo),
        repositoryUrl: scalar(pkg.repositoryUrl),
        homepage: scalar(pkg.homepage),
        weeklyDownloads: scalar(pkg.weeklyDownloads),
        lastPublished: scalar(pkg.lastPublished),
        license: scalar(pkg.license),
        keywords: scalar(pkg.keywords),
        score: scalar(pkg.score),
        searchScore: scalar(pkg.searchScore),
      };
    });
  },
};

export function packageSearchToTsv(data: unknown): TsvExport {
  return renderTsv(packageSearchProjection, data);
}

// ---------------------------------------------------------------------------
// localSearchCode (ripgrep) — flatten files[].matches[] into one row per hit
// ---------------------------------------------------------------------------
export const localSearchCodeColumns = [
  'path',
  'matchCount',
  'line',
  'column',
  'value',
] as const;

export const localSearchCodeProjection: TsvProjection = {
  columns: localSearchCodeColumns,
  toRows: data => {
    const files = arr((data as { files?: unknown }).files);
    const rows: Array<Record<string, unknown>> = [];
    for (const f of files) {
      const file = obj(f);
      const matches = arr(file.matches);
      if (matches.length === 0) {
        rows.push({
          path: scalar(file.path),
          matchCount: scalar(file.matchCount),
          line: '',
          column: '',
          value: '',
        });
        continue;
      }
      for (const m of matches) {
        const match = obj(m);
        rows.push({
          path: scalar(file.path),
          matchCount: scalar(file.matchCount),
          line: scalar(match.line),
          column: scalar(match.column),
          value: scalar(match.value),
        });
      }
    }
    return rows;
  },
};

export function localSearchCodeToTsv(data: unknown): TsvExport {
  return renderTsv(localSearchCodeProjection, data);
}

// ---------------------------------------------------------------------------
// localFindFiles — one row per file
// ---------------------------------------------------------------------------
export const localFindFilesColumns = [
  'path',
  'type',
  'size',
  'permissions',
  'modified',
  'accessed',
  'created',
] as const;

export const localFindFilesProjection: TsvProjection = {
  columns: localFindFilesColumns,
  toRows: data => {
    const files = arr((data as { files?: unknown }).files);
    return files.map(f => {
      const file = obj(f);
      return {
        path: scalar(file.path),
        type: scalar(file.type),
        size: scalar(file.size),
        permissions: scalar(file.permissions),
        modified: scalar(file.modified),
        accessed: scalar(file.accessed),
        created: scalar(file.created),
      };
    });
  },
};

export function localFindFilesToTsv(data: unknown): TsvExport {
  return renderTsv(localFindFilesProjection, data);
}

// ---------------------------------------------------------------------------
// localViewStructure — one row per entry
// ---------------------------------------------------------------------------
export const localViewStructureColumns = [
  'name',
  'path',
  'type',
  'size',
  'modified',
  'depth',
] as const;

export const localViewStructureProjection: TsvProjection = {
  columns: localViewStructureColumns,
  toRows: data => {
    const entries = arr((data as { entries?: unknown }).entries);
    return entries.map(e => {
      const entry = obj(e);
      return {
        name: scalar(entry.name),
        path: scalar(entry.path),
        type: scalar(entry.type),
        size: scalar(entry.size),
        modified: scalar(entry.modified),
        depth: scalar(entry.depth),
      };
    });
  },
};

export function localViewStructureToTsv(data: unknown): TsvExport {
  return renderTsv(localViewStructureProjection, data);
}

// ---------------------------------------------------------------------------
// localGetFileContent — single row per file slice
// ---------------------------------------------------------------------------
export const localFetchContentColumns = [
  'path',
  'content',
  'startLine',
  'endLine',
  'totalLines',
  'isPartial',
  'matchRanges',
] as const;

export const localFetchContentProjection: TsvProjection = {
  columns: localFetchContentColumns,
  toRows: data => {
    const d = obj(data);
    if (typeof d.content !== 'string' && typeof d.totalLines !== 'number') {
      return [];
    }
    return [
      {
        path: scalar(d.path),
        content: scalar(d.content),
        startLine: scalar(d.startLine),
        endLine: scalar(d.endLine),
        totalLines: scalar(d.totalLines),
        isPartial: scalar(d.isPartial),
        matchRanges: scalar(d.matchRanges),
      },
    ];
  },
};

export function localFetchContentToTsv(data: unknown): TsvExport {
  return renderTsv(localFetchContentProjection, data);
}

// ---------------------------------------------------------------------------
// LSP tools — flatten reference/definition/call locations
// ---------------------------------------------------------------------------
export const lspGotoDefinitionColumns = [
  'uri',
  'name',
  'kind',
  'line',
  'column',
  'content',
  'snippet',
] as const;

export const lspGotoDefinitionProjection: TsvProjection = {
  columns: lspGotoDefinitionColumns,
  toRows: data => {
    const defs = arr(
      (data as { definitions?: unknown; locations?: unknown }).definitions ??
        (data as { locations?: unknown }).locations
    );
    return defs.map(d => {
      const loc = obj(d);
      const pos = locationLineColumn(loc);
      return {
        uri: scalar(loc.uri),
        name: scalar(loc.name),
        kind: scalar(loc.kind),
        line: pos.line,
        column: pos.column,
        content: scalar(loc.content),
        snippet: scalar(loc.snippet),
      };
    });
  },
};

export function lspGotoDefinitionToTsv(data: unknown): TsvExport {
  return renderTsv(lspGotoDefinitionProjection, data);
}

export const lspFindReferencesColumns = [
  'uri',
  'name',
  'kind',
  'line',
  'column',
  'content',
  'snippet',
  'isDeclaration',
] as const;

export const lspFindReferencesProjection: TsvProjection = {
  columns: lspFindReferencesColumns,
  toRows: data => {
    const refs = arr(
      (data as { references?: unknown; locations?: unknown }).references ??
        (data as { locations?: unknown }).locations
    );
    return refs.map(r => {
      const loc = obj(r);
      const pos = locationLineColumn(loc);
      return {
        uri: scalar(loc.uri),
        name: scalar(loc.name),
        kind: scalar(loc.kind),
        line: pos.line,
        column: pos.column,
        content: scalar(loc.content),
        snippet: scalar(loc.snippet),
        isDeclaration: scalar(loc.isDeclaration),
      };
    });
  },
};

export function lspFindReferencesToTsv(data: unknown): TsvExport {
  return renderTsv(lspFindReferencesProjection, data);
}

export const lspCallHierarchyColumns = [
  'direction',
  'name',
  'kind',
  'uri',
  'line',
  'column',
  'fromRanges',
] as const;

export const lspCallHierarchyProjection: TsvProjection = {
  columns: lspCallHierarchyColumns,
  toRows: data => {
    const calls = arr((data as { calls?: unknown }).calls);
    return calls.map(c => {
      const call = obj(c);
      const node = obj(call.from ?? call.to ?? call.item);
      const pos = locationLineColumn(obj(node.range ? node : call));
      return {
        direction: scalar(call.direction),
        name: scalar(node.name),
        kind: scalar(node.kind),
        uri: scalar(node.uri),
        line: pos.line,
        column: pos.column,
        fromRanges: scalar(call.fromRanges),
      };
    });
  },
};

export function lspCallHierarchyToTsv(data: unknown): TsvExport {
  return renderTsv(lspCallHierarchyProjection, data);
}

export const TOOL_TSV_PROJECTIONS: Record<string, TsvProjection> = {
  // Remote
  [STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE]: githubSearchCodeProjection,
  [STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT]: githubFetchContentProjection,
  [STATIC_TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES]:
    githubSearchRepositoriesProjection,
  [STATIC_TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS]:
    githubSearchPullRequestsProjection,
  [STATIC_TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE]:
    githubViewRepoStructureProjection,
  [STATIC_TOOL_NAMES.GITHUB_CLONE_REPO]: githubCloneRepoProjection,
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
  return TOOL_TSV_PROJECTIONS[toolName];
}

export function exportToolDataToTsv(
  toolName: string,
  data: unknown
): TsvExport | undefined {
  const projection = getTsvProjection(toolName);
  return projection ? renderTsv(projection, data) : undefined;
}
