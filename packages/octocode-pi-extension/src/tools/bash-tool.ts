/**
 * Octocode `bash` — same-name override of Pi's built-in bash.
 * Keeps full shell power for git/builds/sed, but blocks redirects / tee /
 * cp|mv destinations that escape Octocode path-guard roots.
 */
import { constants } from 'node:fs';
import { access } from 'node:fs/promises';
import { spawn } from 'node:child_process';
import path from 'node:path';
import type { TSchema, ToolCallResult, ToolDefinition, PiTheme } from '../types.js';
import { makeRenderer, truncateToWidth } from './render-helpers.js';
import { assertPathAllowed } from './path-guard.js';

type TypeBoxBuilder = (typeof import('typebox'))['Type'];

const DEFAULT_MAX_LINES = 2000;
const DEFAULT_MAX_BYTES = 50 * 1024;

/** Catastrophic patterns we refuse even when paths look local. */
const BLOCKED_COMMAND_PATTERNS: RegExp[] = [
  /\brm\s+(-[a-zA-Z]*f[a-zA-Z]*\s+|--force\s+)*\/\s*$/m,
  /\brm\s+(-[a-zA-Z]*rf[a-zA-Z]*|-[a-zA-Z]*fr[a-zA-Z]*)\s+\/(\s|$)/,
  /\bmkfs\b/,
  /\bdd\s+.*\bof=\/dev\//,
  />\s*\/dev\/sd[a-z]/,
];

/**
 * Extract likely write targets from a shell command for path-guard checks.
 * Best-effort — not a full shell parser. Misses are fail-open for non-redirect
 * commands; hits outside roots are blocked.
 */
export function extractBashWriteTargets(command: string, cwd: string): string[] {
  const targets: string[] = [];
  const push = (raw: string) => {
    const cleaned = raw
      .trim()
      .replace(/^['"]|['"]$/g, '')
      .replace(/\\ /g, ' ');
    if (!cleaned || cleaned === '-' || cleaned.startsWith('&') || cleaned.startsWith('(')) return;
    // Skip /dev/null and process substitutions.
    if (cleaned === '/dev/null' || cleaned.startsWith('/dev/fd/')) return;
    targets.push(path.isAbsolute(cleaned) ? cleaned : path.resolve(cwd, cleaned));
  };

  // Redirects: > file, >> file, 2> file, &> file
  const redirectRe = /(?:^|[\s;|&])(?:\d*)?>>?\s*([^\s;|&<>]+)/g;
  let match: RegExpExecArray | null;
  while ((match = redirectRe.exec(command)) !== null) {
    push(match[1]!);
  }

  // tee [-a] file...
  const teeRe = /\btee\b(?:\s+-a)?\s+([^\n;|&]+)/g;
  while ((match = teeRe.exec(command)) !== null) {
    for (const part of match[1]!.trim().split(/\s+/)) {
      if (part.startsWith('-')) continue;
      push(part);
    }
  }

  // cp/mv ... dest (last non-flag arg) — only when dest looks like a path
  const copyRe = /\b(?:cp|mv|install)\b(?:\s+-[a-zA-Z]+|\s+--[^\s]+)*\s+(.+)$/gm;
  while ((match = copyRe.exec(command)) !== null) {
    const args = match[1]!.trim().split(/\s+/).filter((a) => !a.startsWith('-'));
    const dest = args[args.length - 1];
    if (dest) push(dest);
  }

  return [...new Set(targets)];
}

function assertBashCommandAllowed(command: string, cwd: string): void {
  for (const pattern of BLOCKED_COMMAND_PATTERNS) {
    if (pattern.test(command)) {
      throw new Error(
        `bash blocked: command matches a catastrophic pattern. ` +
          `Refusing to run. Rewrite the command to a safer form.`,
      );
    }
  }
  for (const target of extractBashWriteTargets(command, cwd)) {
    assertPathAllowed(target, cwd, 'bash write');
  }
}

function truncateOutput(text: string): string {
  const lines = text.split('\n');
  let out = text;
  if (lines.length > DEFAULT_MAX_LINES) {
    out = lines.slice(-DEFAULT_MAX_LINES).join('\n');
    out = `[truncated to last ${DEFAULT_MAX_LINES} lines]\n${out}`;
  }
  if (Buffer.byteLength(out, 'utf8') > DEFAULT_MAX_BYTES) {
    // Keep the tail.
    let end = out.length;
    while (end > 0 && Buffer.byteLength(out.slice(0, end), 'utf8') > DEFAULT_MAX_BYTES) {
      end = Math.floor(end * 0.9);
    }
    out = `${out.slice(Math.max(0, out.length - end))}\n[truncated to last ${DEFAULT_MAX_BYTES} bytes]`;
  }
  return out;
}

async function runBash(
  command: string,
  cwd: string,
  timeoutSec: number | undefined,
  signal?: AbortSignal,
): Promise<{ stdout: string; stderr: string; code: number | null }> {
  await access(cwd, constants.F_OK).catch(() => {
    throw new Error(`Working directory does not exist: ${cwd}\nCannot execute bash commands.`);
  });
  if (signal?.aborted) throw new Error('Operation aborted');

  const shell = process.env.SHELL || '/bin/bash';
  return await new Promise((resolve, reject) => {
    const child = spawn(shell, ['-lc', command], {
      cwd,
      env: process.env,
      detached: process.platform !== 'win32',
    });
    let stdout = '';
    let stderr = '';
    let settled = false;
    const timer =
      timeoutSec && timeoutSec > 0
        ? setTimeout(() => {
            try {
              child.kill('SIGTERM');
            } catch {
              /* ignore */
            }
            if (!settled) {
              settled = true;
              reject(new Error(`bash timed out after ${timeoutSec}s`));
            }
          }, timeoutSec * 1000)
        : null;

    const onAbort = () => {
      try {
        child.kill('SIGTERM');
      } catch {
        /* ignore */
      }
    };
    signal?.addEventListener('abort', onAbort, { once: true });

    child.stdout?.on('data', (chunk: Buffer) => {
      stdout += chunk.toString('utf8');
    });
    child.stderr?.on('data', (chunk: Buffer) => {
      stderr += chunk.toString('utf8');
    });
    child.on('error', (err) => {
      if (timer) clearTimeout(timer);
      signal?.removeEventListener('abort', onAbort);
      if (!settled) {
        settled = true;
        reject(err);
      }
    });
    child.on('close', (code) => {
      if (timer) clearTimeout(timer);
      signal?.removeEventListener('abort', onAbort);
      if (!settled) {
        settled = true;
        resolve({ stdout, stderr, code });
      }
    });
  });
}

export function registerBashTool(
  pi: { registerTool?(def: ToolDefinition): void },
  Type: TypeBoxBuilder,
): void {
  const parameters = Type.Object(
    {
      command: Type.String({ description: 'Bash command to execute' }),
      timeout: Type.Optional(
        Type.Integer({ description: 'Timeout in seconds (optional, no default timeout)' }),
      ),
    },
    { additionalProperties: false },
  ) as TSchema;

  pi.registerTool?.({
    name: 'bash',
    label: 'bash (Octocode)',
    description:
      'Octocode custom bash tool. Replaces Pi built-in bash with the same shell execution plus Octocode path-guard on redirect/tee/cp/mv write targets (cwd / home / OS temp / ALLOWED_PATHS) and a small blocklist of catastrophic commands. Prefer edit/write for ordinary file mutations; use bash for git, builds, tests, and bulk mechanical edits.',
    promptSnippet: 'Run shell commands with Octocode path-guard on write targets.',
    promptGuidelines: [
      'Octocode custom bash replaces Pi built-in bash; prefer edit/write for ordinary file creates and surgical edits.',
      'Use bash for git, builds, tests, package managers, and bulk mechanical edits (e.g. sed).',
      'Redirects (>, >>, tee) and cp/mv destinations must stay inside the working directory, home, OS temp, or ALLOWED_PATHS.',
      'Do not use bash to bypass the edit/write path-guard.',
    ],
    parameters,
    async execute(
      _toolCallId: string,
      params: Record<string, unknown>,
      signal?: AbortSignal,
      _onUpdate?: unknown,
      ctx?: { cwd?: string },
    ): Promise<ToolCallResult> {
      if (typeof params['command'] !== 'string' || params['command'].trim().length === 0) {
        throw new Error('Bash tool input is invalid. command must be a non-empty string.');
      }
      const command = params['command'];
      const timeout =
        typeof params['timeout'] === 'number' && Number.isFinite(params['timeout'])
          ? params['timeout']
          : undefined;
      const cwd = ctx?.cwd ?? process.cwd();
      assertBashCommandAllowed(command, cwd);
      if (signal?.aborted) throw new Error('Operation aborted');

      const { stdout, stderr, code } = await runBash(command, cwd, timeout, signal);
      const combined = [stdout, stderr].filter(Boolean).join('\n');
      const text = truncateOutput(combined || `(exit ${code ?? 'null'})`);
      const isError = code !== 0 && code !== null;
      return {
        content: [{ type: 'text', text }],
        isError,
        details: { code, stdout, stderr },
      };
    },
    renderCall(args: unknown, theme?: PiTheme) {
      const input = args && typeof args === 'object' ? (args as Record<string, unknown>) : {};
      const command = typeof input['command'] === 'string' ? input['command'] : '(missing command)';
      const title = theme?.fg('toolTitle', theme.bold('bash')) ?? 'bash';
      const suffix = theme?.fg('dim', command) ?? command;
      return makeRenderer((width) => [truncateToWidth(`${title} ${suffix}`, width)]);
    },
    renderResult(result: ToolCallResult, opts: { expanded?: boolean; isPartial?: boolean }, theme?: PiTheme) {
      if (opts.isPartial) {
        const prog = theme?.fg('warning', '… running') ?? '… running';
        return makeRenderer(() => [prog]);
      }
      if (!opts.expanded && !result.isError) {
        return makeRenderer(() => ['']);
      }
      const text = result.content.find((c) => c.type === 'text')?.text ?? '';
      const colored = result.isError ? (theme?.fg('error', text) ?? text) : text;
      return makeRenderer((width) =>
        colored.split('\n').map((line) => truncateToWidth(line, width)),
      );
    },
  });
}
