import { readFileSync, existsSync } from 'node:fs';
import path from 'node:path';
import type { CLICommand } from '../types.js';
import { getBool, getString } from '../options.js';
import { resolveRef, isGithubRef, refLabel } from '../routing.js';
import { c, bold, dim } from '../../utils/colors.js';
import {
  minifyContent,
  extractSignatures,
  SIGNATURES_ONLY_HINT,
  SUPPORTED_SIGNATURE_EXTENSIONS,
} from '@octocodeai/octocode-minifier-utils';
import { executeDirectTool, type ContentResult } from 'octocode-mcp/public';

type MinifyMode = 'standard' | 'symbols' | 'none';
const VALID_MODES: MinifyMode[] = ['standard', 'symbols', 'none'];

// ── helpers ───────────────────────────────────────────────────────────────────

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes}B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)}KB`;
  return `${(bytes / 1024 / 1024).toFixed(2)}MB`;
}

function formatReduction(original: number, minified: number): string {
  if (original === 0) return '0%';
  return `${((1 - minified / original) * 100).toFixed(1)}%`;
}

interface FetchGithubOpts {
  branch?: string;
  matchString?: string;
  startLine?: number;
  endLine?: number;
  charLength?: number;
  charOffset?: number;
}

async function fetchGithubContent(
  owner: string,
  repo: string,
  subpath: string,
  opts: FetchGithubOpts = {}
): Promise<{ content: string; pagination?: Record<string, unknown> }> {
  const result = await executeDirectTool('githubGetFileContent', {
    queries: [
      {
        owner,
        repo,
        path: subpath || '.',
        branch: opts.branch,
        minify: 'none',
        matchString: opts.matchString,
        startLine: opts.startLine,
        endLine: opts.endLine,
        charLength: opts.charLength,
        charOffset: opts.charOffset,
        mainResearchGoal: 'Fetch file content',
        researchGoal: 'Fetch raw content for get command',
        reasoning: 'CLI get command',
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
      throw new Error(`Not found on GitHub: ${owner}/${repo}/${subpath}`);
    }
    throw new Error(`GitHub API error: ${errText}`);
  }

  const structured = result.structuredContent as ContentResult | undefined;
  const queryResult = structured?.results?.[0];
  const fileResult = (queryResult as Record<string, unknown> | undefined)
    ?.files as Array<Record<string, unknown>> | undefined;
  const firstFile = fileResult?.[0];
  const content = firstFile?.['content'] as string | undefined;
  if (!content) {
    throw new Error(`No content returned for ${owner}/${repo}/${subpath}`);
  }
  const pagination = firstFile?.['pagination'] as
    | Record<string, unknown>
    | undefined;
  return { content, pagination };
}

async function applyMode(
  raw: string,
  mode: MinifyMode,
  ext: string
): Promise<{ content: string; strategy: string }> {
  if (mode === 'none') return { content: raw, strategy: 'none' };

  if (mode === 'symbols') {
    // SUPPORTED_SIGNATURE_EXTENSIONS is an array in the compiled package
    const exts = SUPPORTED_SIGNATURE_EXTENSIONS as unknown as string[];
    const canExtract = Array.isArray(exts) ? exts.includes(ext) : false;
    if (canExtract) {
      const sig = extractSignatures(raw, ext);
      if (sig !== null) return { content: sig, strategy: 'symbols' };
    }
    // Fall back to standard when extension not supported for symbols
  }

  const result = await minifyContent(raw, ext);
  const minified = result as unknown as { content: string; strategy: string };
  return {
    content: minified.content,
    strategy: minified.strategy ?? 'standard',
  };
}

// ── command ───────────────────────────────────────────────────────────────────

export const getCommand: CLICommand = {
  name: 'get',
  description:
    'Fetch and minify file content — works for local paths and GitHub references',
  usage:
    'octocode get <path|github-ref> [--mode none|standard|symbols] [--type <ext>] [--branch <ref>] [--match-string <s>] [--start-line <n>] [--end-line <n>] [--page-size <n>] [--page <n>] [--stats] [--json]',
  options: [
    {
      name: 'mode',
      hasValue: true,
      description: 'Minification mode: standard (default) · symbols · none',
    },
    {
      name: 'type',
      hasValue: true,
      description:
        'Language hint — overrides auto-detection (e.g. ts, py, css)',
    },
    {
      name: 'branch',
      hasValue: true,
      description: 'Branch / ref for GitHub paths (overrides inline branch)',
    },
    {
      name: 'match-string',
      hasValue: true,
      description:
        'Return only sections matching this string — ALL occurrences returned',
    },
    {
      name: 'start-line',
      hasValue: true,
      description: 'First line to return — 1-based (GitHub only)',
    },
    {
      name: 'end-line',
      hasValue: true,
      description: 'Last line to return — 1-based (GitHub only)',
    },
    {
      name: 'page-size',
      hasValue: true,
      description: 'Characters per page for GitHub file reads',
    },
    {
      name: 'page',
      hasValue: true,
      description: 'GitHub file page number when pagination is available',
    },
    {
      name: 'stats',
      description: 'Print size-reduction statistics',
    },
    {
      name: 'json',
      description:
        'Output as JSON: { content, mode, strategy, pagination?, ... }',
    },
  ],
  handler: async args => {
    const { options } = args;
    const target = args.args[0] ?? '';
    const typeHint = getString(options, 'type');
    const branchOverride = getString(options, 'branch');
    const rawMode = getString(options, 'mode') || 'standard';
    const showStats = getBool(options, 'stats');
    const jsonOutput = getBool(options, 'json');
    const matchString = getString(options, 'match-string');
    const rawStartLine = getString(options, 'start-line');
    const rawEndLine = getString(options, 'end-line');
    const rawPageSize = getString(options, 'page-size');
    const rawPage = getString(options, 'page');
    const startLine = rawStartLine ? parseInt(rawStartLine, 10) : undefined;
    const endLine = rawEndLine ? parseInt(rawEndLine, 10) : undefined;
    const pageSize = rawPageSize ? parseInt(rawPageSize, 10) : undefined;
    const page = rawPage ? parseInt(rawPage, 10) : undefined;
    const charOffset =
      pageSize && page && page > 1 ? (page - 1) * pageSize : undefined;

    if (!VALID_MODES.includes(rawMode as MinifyMode)) {
      const err = `Unknown mode "${rawMode}". Valid: ${VALID_MODES.join(', ')}`;
      if (jsonOutput) {
        console.log(JSON.stringify({ success: false, error: err }));
      } else {
        console.error(`\n  ${c('red', '✗')} ${err}\n`);
      }
      process.exitCode = 1;
      return;
    }
    const mode = rawMode as MinifyMode;

    // ── source resolution ────────────────────────────────────────────────────
    let raw: string;
    let resolvedExt: string;
    let sourceLabel: string;
    let githubMeta: { owner: string; repo: string; path: string } | undefined;
    let paginationMeta: Record<string, unknown> | undefined;

    if (target) {
      const ref = resolveRef(target, branchOverride || undefined);

      if (isGithubRef(ref)) {
        sourceLabel = refLabel(ref);
        if (!jsonOutput) {
          process.stderr.write(`  ${dim(`Fetching ${sourceLabel} ...`)}\n`);
        }
        const fetchResult = await fetchGithubContent(
          ref.owner,
          ref.repo,
          ref.subpath,
          {
            branch: ref.branch,
            matchString: matchString || undefined,
            startLine,
            endLine,
            charLength: pageSize,
            charOffset,
          }
        ).catch((e: Error) => {
          if (jsonOutput) {
            console.log(JSON.stringify({ success: false, error: e.message }));
          } else {
            console.error(`\n  ${c('red', '✗')} ${e.message}\n`);
          }
          process.exitCode = 1;
          return null;
        });
        if (fetchResult === null) return;
        raw = fetchResult.content;
        paginationMeta = fetchResult.pagination;
        resolvedExt = typeHint || path.extname(ref.subpath).slice(1);
        githubMeta = { owner: ref.owner, repo: ref.repo, path: ref.subpath };
      } else {
        if (!existsSync(ref.path)) {
          const err = `File not found: ${ref.path}`;
          if (jsonOutput) {
            console.log(JSON.stringify({ success: false, error: err }));
          } else {
            console.error(`\n  ${c('red', '✗')} ${err}\n`);
          }
          process.exitCode = 1;
          return;
        }
        const stat = await import('node:fs').then(m => m.statSync(ref.path));
        if (stat.isDirectory()) {
          const err = `${ref.path} is a directory — use "octocode tree" for directory structure`;
          if (jsonOutput) {
            console.log(JSON.stringify({ success: false, error: err }));
          } else {
            console.error(`\n  ${c('red', '✗')} ${err}\n`);
          }
          process.exitCode = 1;
          return;
        }
        raw = readFileSync(ref.path, 'utf-8');
        resolvedExt = typeHint || path.extname(ref.path).slice(1);
        sourceLabel = ref.path;
      }
    } else {
      const err = 'Provide a file path or GitHub reference.';
      if (jsonOutput) {
        console.log(JSON.stringify({ success: false, error: err }));
      } else {
        console.error(`\n  ${c('red', '✗')} ${err}`);
        console.error(
          `\n  ${dim('Examples:')}\n` +
            `    octocode get src/utils.ts\n` +
            `    octocode get bgauryy/octocode-mcp/README.md\n` +
            `    octocode get "https://github.com/owner/repo/blob/main/file.ts"\n`
        );
      }
      process.exitCode = 1;
      return;
    }

    // ── minify ───────────────────────────────────────────────────────────────
    const originalSize = Buffer.byteLength(raw, 'utf-8');
    const { content, strategy } = await applyMode(raw, mode, resolvedExt);
    const minifiedSize = Buffer.byteLength(content, 'utf-8');

    // ── output ───────────────────────────────────────────────────────────────
    if (jsonOutput) {
      const out: Record<string, unknown> = {
        content,
        mode,
        strategy,
        originalSize,
        minifiedSize,
        reduction: formatReduction(originalSize, minifiedSize),
        failed: false,
      };
      if (githubMeta) out['github'] = githubMeta;
      if (paginationMeta) out['pagination'] = paginationMeta;
      console.log(JSON.stringify(out));
      return;
    }

    if (showStats) {
      const hint = mode === 'symbols' ? '' : ` (${strategy})`;
      const sigHint =
        mode === 'symbols' ? `\n  ${dim(SIGNATURES_ONLY_HINT)}` : '';
      const tag =
        minifiedSize < originalSize
          ? `${c('green', '✓')} Mode: ${bold(mode)}${hint}  |  ${formatBytes(originalSize)} → ${formatBytes(minifiedSize)}  ${bold(formatReduction(originalSize, minifiedSize))} smaller`
          : `${c('yellow', '·')} Mode: ${bold(mode)}${hint}  |  ${formatBytes(originalSize)} (no reduction)`;
      console.error(`  ${tag}${sigHint}`);
    }

    const lines = content.split('\n');
    const PREVIEW = 8;
    if (lines.length > PREVIEW && !showStats) {
      console.log(
        lines
          .slice(0, PREVIEW)
          .map(l => `  ${c('cyan', '│')} ${l}`)
          .join('\n')
      );
      console.error(`  ${dim(`│ … (${lines.length - PREVIEW} more lines)`)}`);
    } else {
      console.log(lines.map(l => `  ${c('cyan', '│')} ${l}`).join('\n'));
    }

    if (paginationMeta) {
      const { page: curPage, totalPages } = paginationMeta as {
        page?: number;
        totalPages?: number;
      };
      if (totalPages && totalPages > 1) {
        console.error(
          `\n  ${dim(`Page ${curPage ?? 1}/${totalPages} — use --page <n> --page-size ${pageSize ?? 'N'} to navigate`)}`
        );
      }
    }
  },
};
