import type { CLICommand } from '../types.js';
import { getBool, getString } from '../options.js';
import { c, bold, dim } from '../../utils/colors.js';
import { executeDirectTool } from '@octocodeai/octocode-tools-core/direct';

interface PRLabel {
  name?: string;
}

interface PRFileChange {
  path?: string;
  filename?: string; // fallback field name
  additions?: number;
  deletions?: number;
  patch?: string;
  status?: string;
}

interface PRComment {
  author?: string | { login?: string };
  body?: string;
  createdAt?: string;
  created_at?: string; // fallback field name
  path?: string;
  line?: number;
}

interface PRCommit {
  sha?: string;
  message?: string;
  author?: string | { login?: string };
}

interface PRReview {
  author?: string | { login?: string };
  state?: string;
  body?: string;
}

interface PRPagination {
  currentPage?: number;
  totalPages?: number;
  totalMatches?: number;
  hasMore?: boolean;
  page?: number;
}

interface PRItem {
  number?: number;
  title?: string;
  state?: string;
  draft?: boolean;
  author?: string | { login?: string };
  url?: string;
  body?: string;
  bodyPagination?: {
    charOffset?: number;
    charLength?: number;
    totalChars?: number;
    hasMore?: boolean;
  };
  createdAt?: string;
  updatedAt?: string;
  mergedAt?: string;
  closedAt?: string;
  targetBranch?: string; // base branch
  sourceBranch?: string; // head branch (may be absent)
  base?: string | { ref?: string }; // fallback field name
  head?: string | { ref?: string }; // fallback field name
  changedFilesCount?: number;
  changed_files?: number; // fallback field name
  additions?: number;
  deletions?: number;
  labels?: Array<string | PRLabel>;
  changedFiles?: PRFileChange[];
  file_changes?: PRFileChange[]; // fallback field name
  filePagination?: PRPagination;
  comment_details?: PRComment[];
  commit_details?: PRCommit[];
  reviews?: PRReview[];
}

interface PRSearchResult {
  results?: Array<{
    id?: string;
    data?: {
      pull_requests?: PRItem[];
      pagination?: PRPagination;
    };
    pull_requests?: PRItem[];
    pagination?: PRPagination;
  }>;
  pull_requests?: PRItem[];
  pagination?: PRPagination;
  total_count?: number;
}

interface PrTarget {
  owner: string;
  repo: string;
  prNumber?: number;
}

function parsePrTarget(input: string, prOverride?: string): PrTarget | null {
  const urlMatch = input.match(/github\.com\/([^/]+)\/([^/]+)\/pull\/(\d+)/);
  if (urlMatch) {
    return {
      owner: urlMatch[1],
      repo: urlMatch[2],
      prNumber: parseInt(urlMatch[3], 10),
    };
  }

  const hashMatch = input.match(/^([^/]+)\/([^#/]+)#(\d+)$/);
  if (hashMatch) {
    return {
      owner: hashMatch[1],
      repo: hashMatch[2],
      prNumber: parseInt(hashMatch[3], 10),
    };
  }

  const repoMatch = input.match(/^([^/]+)\/([^/]+)$/);
  if (repoMatch) {
    const base = { owner: repoMatch[1], repo: repoMatch[2] };
    if (prOverride) {
      const n = parseInt(prOverride, 10);
      if (!isNaN(n)) return { ...base, prNumber: n };
    }
    return base;
  }

  return null;
}

function extractPRs(sc: PRSearchResult): {
  prs: PRItem[];
  total: number;
  pagination: PRPagination | undefined;
} {
  const firstResult = sc?.results?.[0];
  if (firstResult?.data?.pull_requests) {
    return {
      prs: firstResult.data.pull_requests,
      total:
        firstResult.data.pagination?.totalMatches ??
        firstResult.data.pull_requests.length,
      pagination: firstResult.data.pagination,
    };
  }
  if (firstResult?.pull_requests) {
    return {
      prs: firstResult.pull_requests,
      total:
        firstResult.pagination?.totalMatches ??
        firstResult.pull_requests.length,
      pagination: firstResult.pagination,
    };
  }
  if (sc?.pull_requests) {
    return {
      prs: sc.pull_requests,
      total: sc.total_count ?? sc.pull_requests.length,
      pagination: sc.pagination,
    };
  }
  return { prs: [], total: 0, pagination: undefined };
}

function authorName(a: string | { login?: string } | undefined): string {
  if (!a) return '?';
  if (typeof a === 'string') return a;
  return a.login ?? '?';
}

function branchRef(r: string | { ref?: string } | undefined): string {
  if (!r) return '?';
  if (typeof r === 'string') return r;
  return r.ref ?? '?';
}

function labelNames(labels: Array<string | PRLabel> | undefined): string[] {
  return (labels ?? []).map(l => (typeof l === 'string' ? l : (l.name ?? '')));
}

function relativeTime(dateStr: string | undefined): string {
  if (!dateStr) return '';
  const ms = Date.now() - new Date(dateStr).getTime();
  const mins = Math.floor(ms / 60000);
  if (mins < 2) return 'just now';
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `${days}d ago`;
  return new Date(dateStr).toISOString().slice(0, 10);
}

function stateBadge(pr: PRItem): string {
  if (pr.state === 'merged' || pr.mergedAt) return c('magenta', 'merged');
  if (pr.state === 'closed' || pr.closedAt) return c('red', 'closed');
  if (pr.draft) return c('yellow', 'draft');
  if (pr.state === 'open') return c('green', 'open');
  return pr.draft ? c('yellow', 'draft') : c('green', 'open');
}

function fileChangePath(f: PRFileChange): string {
  return f.path ?? f.filename ?? '';
}

function renderList(sc: PRSearchResult, limit: number): string {
  const { prs, total, pagination } = extractPRs(sc);
  if (prs.length === 0) return `  ${dim('No pull requests found.')}`;

  const lines: string[] = [];
  const shown = prs.slice(0, limit);

  for (const pr of shown) {
    const num = bold(`#${pr.number ?? '?'}`);
    const state = stateBadge(pr);
    const title = pr.title ?? '';
    const author = dim(`by ${authorName(pr.author)}`);
    const when = dim(relativeTime(pr.mergedAt ?? pr.updatedAt ?? pr.createdAt));
    const tags = labelNames(pr.labels);
    const labelStr = tags.length ? dim(` [${tags.join(', ')}]`) : '';
    lines.push(`  ${num}  ${state}  ${title}${labelStr}  ${author}  ${when}`);
  }

  if (total > shown.length) {
    lines.push(`\n  ${dim(`… ${total - shown.length} more`)}`);
  }
  if (pagination?.totalPages && pagination.totalPages > 1) {
    const cur = pagination.currentPage ?? pagination.page ?? 1;
    lines.push(
      `\n  ${dim(`Page ${cur}/${pagination.totalPages} — use --page <n> to navigate`)}`
    );
  }

  return lines.join('\n');
}

function renderDetail(sc: PRSearchResult): string {
  const { prs, pagination } = extractPRs(sc);
  const pr = prs[0];
  if (!pr) return `  ${dim('No PR data returned.')}`;

  const lines: string[] = [];

  lines.push(`  ${bold(`PR #${pr.number ?? '?'}`)}  ${pr.title ?? ''}`);

  const base = pr.targetBranch ?? branchRef(pr.base);
  const head = pr.sourceBranch ?? branchRef(pr.head);
  const metaParts = [
    stateBadge(pr),
    pr.draft ? dim('(draft)') : '',
    `${dim('by')} ${bold(authorName(pr.author))}`,
    base !== '?'
      ? `${dim('→')} ${base}${head !== '?' ? ` ${dim('←')} ${head}` : ''}`
      : '',
    dim(relativeTime(pr.mergedAt ?? pr.updatedAt ?? pr.createdAt)),
  ].filter(Boolean);
  lines.push(`  ${metaParts.join('  ')}`);

  const tags = labelNames(pr.labels);
  if (tags.length) lines.push(`  ${dim('Labels:')} ${tags.join(', ')}`);

  if (pr.url) lines.push(`  ${dim(pr.url)}`);

  const fileCount = pr.changedFilesCount ?? pr.changed_files;
  if (fileCount != null || pr.additions != null || pr.deletions != null) {
    const stats = [
      fileCount != null ? `${dim(String(fileCount))} files` : '',
      pr.additions != null ? c('green', `+${pr.additions}`) : '',
      pr.deletions != null ? c('red', `-${pr.deletions}`) : '',
    ]
      .filter(Boolean)
      .join('  ');
    lines.push(`  ${stats}`);
  }
  lines.push('');

  if (pr.body) {
    const bodyLines = pr.body.trim().split('\n');
    const MAX_BODY = 40;
    bodyLines.slice(0, MAX_BODY).forEach(l => lines.push(`  ${l}`));
    if (bodyLines.length > MAX_BODY) {
      lines.push(
        `  ${dim(`… (${bodyLines.length - MAX_BODY} more body lines)`)}`
      );
    }
    if (pr.bodyPagination?.hasMore) {
      lines.push(
        `  ${dim(`… body continues — use --match-string to target a section`)}`
      );
    }
    lines.push('');
  }

  const files = pr.changedFiles ?? pr.file_changes ?? [];
  if (files.length > 0) {
    const totalFiles = pr.changedFilesCount ?? pr.changed_files ?? files.length;
    lines.push(`  ${bold('Changed files')} ${dim(`(${totalFiles})`)}`);
    for (const f of files) {
      const add = f.additions != null ? c('green', `+${f.additions}`) : '';
      const del = f.deletions != null ? c('red', `-${f.deletions}`) : '';
      const status = f.status ? dim(` [${f.status}]`) : '';
      lines.push(
        `    ${c('cyan', fileChangePath(f))}  ${add}  ${del}${status}`
      );

      if (f.patch) {
        const patchLines = f.patch.split('\n');
        const MAX_DIFF = 15;
        patchLines.slice(0, MAX_DIFF).forEach(pl => {
          if (pl.startsWith('+')) lines.push(`      ${c('green', pl)}`);
          else if (pl.startsWith('-')) lines.push(`      ${c('red', pl)}`);
          else if (pl.startsWith('@@')) lines.push(`      ${c('cyan', pl)}`);
          else lines.push(`      ${dim(pl)}`);
        });
        if (patchLines.length > MAX_DIFF) {
          lines.push(
            `      ${dim(`… (${patchLines.length - MAX_DIFF} more diff lines)`)}`
          );
        }
      }
    }
    if (pr.filePagination?.hasMore) {
      lines.push(
        `    ${dim(`… more files — use --page <n> to navigate file pages`)}`
      );
    }
    lines.push('');
  }

  if (pr.comment_details && pr.comment_details.length > 0) {
    lines.push(
      `  ${bold('Comments')} ${dim(`(${pr.comment_details.length})`)}`
    );
    const MAX_COMMENTS = 8;
    for (const cm of pr.comment_details.slice(0, MAX_COMMENTS)) {
      const who = bold(authorName(cm.author));
      const when = dim(relativeTime(cm.createdAt ?? cm.created_at));
      const loc = cm.path
        ? dim(` @ ${cm.path}${cm.line != null ? `:${cm.line}` : ''}`)
        : '';
      lines.push(`    ${who}${loc}  ${when}`);
      const bodyLines = (cm.body ?? '').trim().split('\n').slice(0, 3);
      bodyLines.forEach(bl => lines.push(`    ${dim(bl.slice(0, 200))}`));
    }
    if (pr.comment_details.length > MAX_COMMENTS) {
      lines.push(
        `    ${dim(`… ${pr.comment_details.length - MAX_COMMENTS} more comments`)}`
      );
    }
    lines.push('');
  }

  if (pr.commit_details && pr.commit_details.length > 0) {
    lines.push(`  ${bold('Commits')} ${dim(`(${pr.commit_details.length})`)}`);
    const MAX_COMMITS = 15;
    for (const cm of pr.commit_details.slice(0, MAX_COMMITS)) {
      const sha = c('yellow', (cm.sha ?? '').slice(0, 7));
      const msg = (cm.message ?? '').split('\n')[0].slice(0, 100);
      const who = dim(authorName(cm.author));
      lines.push(`    ${sha}  ${msg}  ${who}`);
    }
    if (pr.commit_details.length > MAX_COMMITS) {
      lines.push(
        `    ${dim(`… ${pr.commit_details.length - MAX_COMMITS} more commits`)}`
      );
    }
    lines.push('');
  }

  if (pr.reviews && pr.reviews.length > 0) {
    lines.push(`  ${bold('Reviews')}`);
    for (const rv of pr.reviews) {
      const who = bold(authorName(rv.author));
      const state = rv.state ?? '';
      const stateStr =
        state === 'APPROVED'
          ? c('green', state)
          : state === 'CHANGES_REQUESTED'
            ? c('red', state)
            : dim(state);
      const bodySnip = (rv.body ?? '').trim().split('\n')[0].slice(0, 100);
      lines.push(
        `    ${who}  ${stateStr}${bodySnip ? `  ${dim(bodySnip)}` : ''}`
      );
    }
    lines.push('');
  }

  if (pagination?.totalPages && pagination.totalPages > 1) {
    const cur = pagination.currentPage ?? pagination.page ?? 1;
    lines.push(
      `  ${dim(`Page ${cur}/${pagination.totalPages} — use --page <n> to navigate`)}`
    );
  }

  return lines.join('\n');
}

interface ListOpts {
  query?: string;
  state?: string;
  author?: string;
  label?: string;
  base?: string;
  sort?: string;
  order?: string;
  draft?: boolean;
  created?: string;
  mergedAt?: string;
  page?: number;
  pageSize?: number;
}

async function fetchPRList(
  owner: string,
  repo: string,
  opts: ListOpts
): Promise<PRSearchResult> {
  const result = await executeDirectTool('ghSearchPRs', {
    queries: [
      {
        owner,
        repo,
        keywordsToSearch: opts.query ? [opts.query] : undefined,
        state: opts.state as 'open' | 'closed' | 'merged' | undefined,
        author: opts.author,
        label: opts.label,
        base: opts.base,
        sort: opts.sort as
          | 'created'
          | 'updated'
          | 'popularity'
          | 'long-running'
          | undefined,
        order: opts.order as 'asc' | 'desc' | undefined,
        draft: opts.draft,
        created: opts.created,
        'merged-at': opts.mergedAt,
        page: opts.page ?? 1,
        itemsPerPage: opts.pageSize,
        mainResearchGoal: 'List pull requests',
        researchGoal: `List PRs for ${owner}/${repo}`,
        reasoning: 'CLI pr command list mode',
      },
    ],
  });

  if (result.isError) {
    const errText =
      result.content[0]?.type === 'text' ? result.content[0].text : '';
    if (/401|403|auth/i.test(errText)) {
      throw new Error(
        `GitHub auth error: ${errText}. Set GITHUB_TOKEN, OCTOCODE_TOKEN, or GH_TOKEN.`
      );
    }
    throw new Error(`GitHub PR search error: ${errText}`);
  }

  return result.structuredContent as PRSearchResult;
}

interface ViewOpts {
  patches?: boolean;
  patchFile?: string;
  comments?: boolean;
  commits?: boolean;
  deep?: boolean;
  matchString?: string;
  charLength?: number;
  charOffset?: number;
  page?: number;
  pageSize?: number;
}

async function fetchPRDetail(
  owner: string,
  repo: string,
  prNumber: number,
  opts: ViewOpts
): Promise<PRSearchResult> {
  const content: Record<string, unknown> = {
    metadata: true,
    body: true,
    changedFiles: true,
  };

  if (opts.patchFile) {
    content['patches'] = { mode: 'selected', files: [opts.patchFile] };
  } else if (opts.patches || opts.deep) {
    content['patches'] = { mode: 'all' };
  }

  if (opts.comments || opts.deep) {
    content['comments'] = { discussion: true, reviewInline: true };
  }

  if (opts.commits || opts.deep) {
    content['commits'] = { list: true, includeFiles: true };
  }

  if (opts.deep) {
    content['reviews'] = true;
  }

  const result = await executeDirectTool('ghSearchPRs', {
    queries: [
      {
        owner,
        repo,
        prNumber,
        content,
        reviewMode: opts.deep ? 'full' : undefined,
        minify: 'standard',
        matchString: opts.matchString,
        charLength: opts.charLength,
        charOffset: opts.charOffset,
        filePage: opts.page,
        commentPage: opts.comments || opts.deep ? opts.page : undefined,
        commitPage: opts.commits || opts.deep ? opts.page : undefined,
        itemsPerPage: opts.pageSize,
        mainResearchGoal: 'View pull request details',
        researchGoal: `View PR #${prNumber} in ${owner}/${repo}`,
        reasoning: 'CLI pr command view mode',
      },
    ],
  });

  if (result.isError) {
    const errText =
      result.content[0]?.type === 'text' ? result.content[0].text : '';
    if (/401|403|auth/i.test(errText)) {
      throw new Error(
        `GitHub auth error: ${errText}. Set GITHUB_TOKEN, OCTOCODE_TOKEN, or GH_TOKEN.`
      );
    }
    if (/404|not found/i.test(errText)) {
      throw new Error(`PR #${prNumber} not found in ${owner}/${repo}`);
    }
    throw new Error(`GitHub PR fetch error: ${errText}`);
  }

  return result.structuredContent as PRSearchResult;
}

export const prCommand: CLICommand = {
  name: 'pr',
  description:
    'Search and view pull requests — list with filters or deep-dive a single PR',
  usage:
    'octocode pr <owner/repo[#N] | PR-URL> [--pr <n>] [--state open|closed|merged] [--patches] [--comments] [--commits] [--deep] [--json]',
  options: [
    {
      name: 'pr',
      hasValue: true,
      description: 'PR number to view (alternative to owner/repo#N)',
    },
    {
      name: 'query',
      hasValue: true,
      description: 'Keyword search within PR titles/bodies (list mode)',
    },
    {
      name: 'state',
      hasValue: true,
      description: 'Filter by state: open (default) · closed · merged',
    },
    {
      name: 'author',
      hasValue: true,
      description: 'Filter by PR author (list mode)',
    },
    {
      name: 'label',
      hasValue: true,
      description: 'Filter by label (list mode)',
    },
    {
      name: 'base',
      hasValue: true,
      description: 'Filter by base branch (list mode)',
    },
    {
      name: 'sort',
      hasValue: true,
      description:
        'Sort list results: created · updated · popularity · long-running (list mode)',
    },
    {
      name: 'order',
      hasValue: true,
      description: 'Sort direction: asc · desc (list mode, default: desc)',
    },
    {
      name: 'draft',
      description: 'Show only draft PRs (list mode)',
    },
    {
      name: 'created',
      hasValue: true,
      description: 'Filter by creation date, e.g. >2024-01-01 (list mode)',
    },
    {
      name: 'merged-at',
      hasValue: true,
      description: 'Filter by merge date, e.g. >2024-06-01 (list mode)',
    },
    {
      name: 'limit',
      hasValue: true,
      description: 'Max PRs to show in list mode (default: 10)',
    },
    {
      name: 'patches',
      description: 'Include unified diffs for all changed files (view mode)',
    },
    {
      name: 'file',
      hasValue: true,
      description: 'Show diff for a specific file path only (view mode)',
    },
    {
      name: 'comments',
      description: 'Include discussion and inline review comments (view mode)',
    },
    {
      name: 'commits',
      description: 'Include commit list (view mode)',
    },
    {
      name: 'deep',
      description:
        'Full deep-dive: patches + comments + commits + reviews (view mode)',
    },
    {
      name: 'match-string',
      hasValue: true,
      description:
        'Narrow PR content to section matching this string (view mode)',
    },
    {
      name: 'char-length',
      hasValue: true,
      description:
        'Cap PR body/diff size in chars — prevents token flood on large PRs (view mode)',
    },
    {
      name: 'char-offset',
      hasValue: true,
      description:
        'Continue reading PR body from this char offset for pagination (view mode)',
    },
    {
      name: 'page',
      hasValue: true,
      description: 'Page number for paginated results (default: 1)',
    },
    {
      name: 'page-size',
      hasValue: true,
      description: 'Results per page (default: server default)',
    },
    {
      name: 'json',
      description: 'Output raw JSON',
    },
  ],
  handler: async args => {
    const { options } = args;
    const input = args.args[0] ?? '';
    const prOverride = getString(options, 'pr');
    const jsonOutput = getBool(options, 'json');

    if (!input) {
      const err = 'Provide a GitHub repo or PR reference.';
      if (jsonOutput) {
        console.log(JSON.stringify({ success: false, error: err }));
      } else {
        console.error(`\n  ${c('red', '✗')} ${err}`);
        console.error(
          `\n  ${dim('Examples:')}\n` +
            `    octocode pr bgauryy/octocode-mcp\n` +
            `    octocode pr bgauryy/octocode-mcp --state merged --limit 5\n` +
            `    octocode pr bgauryy/octocode-mcp --query "fix auth"\n` +
            `    octocode pr bgauryy/octocode-mcp#142\n` +
            `    octocode pr bgauryy/octocode-mcp --pr 142 --patches\n` +
            `    octocode pr bgauryy/octocode-mcp --pr 142 --deep\n` +
            `    octocode pr https://github.com/bgauryy/octocode-mcp/pull/142\n`
        );
      }
      process.exitCode = 1;
      return;
    }

    const target = parsePrTarget(input, prOverride || undefined);
    if (!target) {
      const err = `Cannot parse PR reference: "${input}". Use owner/repo, owner/repo#N, or a GitHub PR URL.`;
      if (jsonOutput) {
        console.log(JSON.stringify({ success: false, error: err }));
      } else {
        console.error(`\n  ${c('red', '✗')} ${err}\n`);
      }
      process.exitCode = 1;
      return;
    }

    const isViewMode = target.prNumber !== undefined;

    const rawLimit = getString(options, 'limit');
    const limit = rawLimit ? parseInt(rawLimit, 10) : 10;
    const rawPage = getString(options, 'page');
    const rawPageSize = getString(options, 'page-size');
    const page = rawPage ? parseInt(rawPage, 10) : undefined;
    const pageSize = rawPageSize ? parseInt(rawPageSize, 10) : undefined;

    const modeLabel = isViewMode
      ? `PR #${target.prNumber} in ${target.owner}/${target.repo}`
      : `PRs in ${target.owner}/${target.repo}`;

    if (!jsonOutput) {
      process.stderr.write(`  ${dim(`Loading ${modeLabel} ...`)}\n`);
    }

    const rawCharLength = getString(options, 'char-length');
    const rawCharOffset = getString(options, 'char-offset');
    const charLength = rawCharLength ? parseInt(rawCharLength, 10) : undefined;
    const charOffset = rawCharOffset ? parseInt(rawCharOffset, 10) : undefined;

    try {
      if (isViewMode) {
        const viewOpts: ViewOpts = {
          patches: getBool(options, 'patches'),
          patchFile: getString(options, 'file') || undefined,
          comments: getBool(options, 'comments'),
          commits: getBool(options, 'commits'),
          deep: getBool(options, 'deep'),
          matchString: getString(options, 'match-string') || undefined,
          charLength,
          charOffset,
          page,
          pageSize,
        };
        const sc = await fetchPRDetail(
          target.owner,
          target.repo,
          target.prNumber!,
          viewOpts
        );
        if (jsonOutput) {
          console.log(JSON.stringify(sc, null, 2));
          return;
        }
        console.log('\n' + renderDetail(sc) + '\n');
      } else {
        const listOpts: ListOpts = {
          query: getString(options, 'query') || undefined,
          state: getString(options, 'state') || 'open',
          author: getString(options, 'author') || undefined,
          label: getString(options, 'label') || undefined,
          base: getString(options, 'base') || undefined,
          sort: getString(options, 'sort') || undefined,
          order: getString(options, 'order') || undefined,
          draft: getBool(options, 'draft') || undefined,
          created: getString(options, 'created') || undefined,
          mergedAt: getString(options, 'merged-at') || undefined,
          page,
          pageSize,
        };
        const sc = await fetchPRList(target.owner, target.repo, listOpts);
        if (jsonOutput) {
          console.log(JSON.stringify(sc, null, 2));
          return;
        }
        console.log('\n' + renderList(sc, limit) + '\n');
      }
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      if (jsonOutput) {
        console.log(JSON.stringify({ success: false, error: msg }));
      } else {
        console.error(`\n  ${c('red', '✗')} ${msg}\n`);
      }
      process.exitCode = 1;
    }
  },
};
