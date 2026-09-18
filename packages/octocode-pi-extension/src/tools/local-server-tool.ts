import fs from 'node:fs';
import path from 'node:path';
import { assertPathAllowed } from './path-guard.js';
import { resolveFilePath } from './file-state.js';
import {
  getLocalServerBaseUrl,
  isValidMountName,
  listLocalServerMounts,
  serveDirectory,
  stopLocalServer,
  unmount,
} from './local-server.js';
import { buildToolView } from './render-helpers.js';
import { buildQueryEnvelopeSchema, executeQueryBatch } from './query-envelope.js';
import type { ToolCallResult, ToolDefinition, PiContext, PiTheme } from '../types.js';
import { DIRECT_TOOL_DESCRIPTIONS, type registerUniqueTool } from './octocode-tools.js';
import { openLocalUrl, type LocalUrlOpenPreference, type LocalUrlOpenResult } from './local-url-opener.js';

import { z } from 'zod';
type RegisterFn = typeof registerUniqueTool;

interface LocalServerQuery {
  reasoning?: string;
  action: 'serve' | 'unmount' | 'status' | 'stop';
  name?: string;
  dir?: string;
  indexFile?: string;
  open?: boolean;
  browser?: LocalUrlOpenPreference;
}

interface LocalServerToolDependencies {
  openUrl?: (url: string, preference: LocalUrlOpenPreference) => Promise<LocalUrlOpenResult>;
}

function textResult(text: string, details: Record<string, unknown>): ToolCallResult {
  return { content: [{ type: 'text' as const, text }], details } as unknown as ToolCallResult;
}

function cleanMountName(name: unknown): string {
  return typeof name === 'string' ? name.trim() : '';
}

function preflightLocalServerQuery(query: Record<string, unknown>, cwd: string): void {
  const action = query['action'];
  if (action === 'status' || action === 'stop') return;
  if (action !== 'serve' && action !== 'unmount') throw new Error(`[localServer] unknown action: ${String(action)}`);
  const name = cleanMountName(query['name']);
  if (!name) throw new Error(`[localServer] ${action} requires a mount name.`);
  if (!isValidMountName(name)) throw new Error('[localServer] invalid mount name: use one safe URL path segment.');
  if (action === 'unmount') return;
  const input = typeof query['dir'] === 'string' ? query['dir'].trim() : '';
  if (!input) throw new Error('[localServer] serve requires dir.');
  const dir = resolveFilePath(input, cwd);
  assertPathAllowed(dir, cwd, 'localServer serve');
  if (!fs.statSync(dir).isDirectory()) throw new Error(`[localServer] not a directory: ${dir}`);
}

function renderStatus(): string {
  const baseUrl = getLocalServerBaseUrl();
  const mounts = listLocalServerMounts();
  if (!baseUrl) return '[localServer] stopped';
  if (mounts.length === 0) return `[localServer] running at ${baseUrl} (no mounts)`;
  return [
    `[localServer] running at ${baseUrl}`,
    ...mounts.map((m) => `- ${m.name}: ${baseUrl}${m.name}/ -> ${m.dir} (${m.indexFile})`),
  ].join('\n');
}

export function registerLocalServerTool(
  pi: {
    registerTool?(def: ToolDefinition): void;
    sendUserMessage?(message: string, options?: { deliverAs?: 'steer' | 'followUp' }): void | Promise<void>;
  },
  registeredToolNames: Set<string>,
  registerFn: RegisterFn,
  dependencies: LocalServerToolDependencies = {},
): void {
  const openUrl = dependencies.openUrl ?? ((url, preference) => openLocalUrl(url, { preference }));
  const parameters = buildQueryEnvelopeSchema(
    z.looseObject({
      action: z.enum(['serve', 'unmount', 'status', 'stop']).describe('serve|unmount|status|stop'),
      name: z.string().optional()
        .describe('Mount name: one safe URL path segment. Required for serve/unmount.'),
      dir: z.string().optional()
        .describe('Directory to serve for action:serve. Relative paths resolve against cwd.'),
      indexFile: z.string().optional()
        .describe('File served at the mount root for action:serve. Default index.html.'),
      open: z.boolean().optional()
        .describe('Open the page; only when user explicitly asks or approves. Always false by default.'),
      browser: z.enum(['auto', 'chrome', 'system', 'vscode', 'none']).optional()
        .describe('Browser for action:serve: auto→VS Code→Chrome→system.'),
    }),
  );
  registerFn(pi, registeredToolNames, {
    name: 'localServer',
    label: 'Local Server',
    description: DIRECT_TOOL_DESCRIPTIONS.localServer!,
    promptSnippet: 'Serve local static artifacts over a loopback-only, path-guarded local server.',
    promptGuidelines: [
      'Use for HTML/Markdown artifacts clearer in browser (plans, diagrams, reports).',
      'action:serve returns URL; open:true only on explicit user request or approval.',
      'Serve only authored/inspected dirs; never expose secrets or untrusted downloads.',
      'Unmount when no longer useful.',
    ],
    parameters,

    async execute(
      toolCallId: string,
      params: Record<string, unknown>,
      signal?: AbortSignal,
      onUpdate?: unknown,
      ctx?: PiContext,
    ): Promise<ToolCallResult> {
      const cwd = ctx?.cwd ?? process.cwd();
      return executeQueryBatch({
        toolCallId,
        raw: params,
        signal,
        onUpdate: typeof onUpdate === 'function' ? onUpdate as (update: ToolCallResult) => void : undefined,
        ctx,
        passthroughSingle: true,
        preflight: query => preflightLocalServerQuery(query, cwd),
        async execute(query) {
          preflightLocalServerQuery(query, cwd);
          const p = query as unknown as LocalServerQuery;

          if (p.action === 'status') {
            return textResult(renderStatus(), {
              action: p.action,
              baseUrl: getLocalServerBaseUrl(),
              mounts: listLocalServerMounts(),
            });
          }

          if (p.action === 'stop') {
            stopLocalServer();
            return textResult('[localServer] stopped', {
              action: p.action,
              baseUrl: undefined,
              mounts: [],
            });
          }

          const name = cleanMountName(p.name);
          if (!name) throw new Error(`[localServer] ${p.action} requires a mount name.`);

          if (p.action === 'unmount') {
            unmount(name);
            return textResult(`[localServer] unmounted ${name}`, {
              action: p.action,
              name,
              baseUrl: getLocalServerBaseUrl(),
              mounts: listLocalServerMounts(),
            });
          }

          if (p.action !== 'serve') {
            throw new Error(`[localServer] unknown action: ${String(p.action)}`);
          }

          const dirInput = typeof p.dir === 'string' ? p.dir.trim() : '';
          if (!dirInput) throw new Error('[localServer] serve requires dir.');

          const dir = resolveFilePath(dirInput, cwd);
          assertPathAllowed(dir, cwd, 'localServer serve');
          if (!fs.statSync(dir).isDirectory()) {
            throw new Error(`[localServer] not a directory: ${dir}`);
          }

          const indexFile =
            typeof p.indexFile === 'string' && p.indexFile.trim()
              ? path.basename(p.indexFile.trim())
              : 'index.html';
          const served = await serveDirectory(name, dir, {
            indexFile,
            onMessage: pi.sendUserMessage
              ? (message) => pi.sendUserMessage!(message, { deliverAs: 'followUp' })
              : undefined,
          });
          if (!served) {
            throw new Error('[localServer] could not mount directory (invalid name or server start failed).');
          }

          const interactive = Boolean(ctx?.hasUI && ctx.mode === 'tui');
          const shouldOpen = interactive && p.open === true;
          const preference = p.browser ?? 'auto';
          const opened = shouldOpen
            ? await openUrl(served.url, preference)
            : { ok: true, requested: preference, openedIn: 'none' as const };
          const openLine = shouldOpen
            ? opened.ok
              ? `Opened in ${opened.openedIn === 'vscode' ? 'VS Code' : opened.openedIn === 'chrome' ? 'Chrome' : 'the default browser'}.`
              : `Browser not opened: ${opened.message ?? 'unknown error'}`
            : interactive
              ? 'Browser remains closed; pass open:true only after explicit user approval.'
              : 'Browser not opened in headless mode.';

          return textResult(`[localServer] ${name}: ${served.url}\nServing ${dir} (${indexFile})\n${openLine}`, {
            action: p.action,
            name,
            dir,
            indexFile,
            url: served.url,
            baseUrl: getLocalServerBaseUrl(),
            mounts: listLocalServerMounts(),
            opened: shouldOpen && opened.ok && opened.openedIn !== 'none',
            openedIn: opened.openedIn,
            browserMessage: opened.message,
          });
        },
      });
    },

    renderCall(args: unknown, theme?: PiTheme) {
      const envelope = (args ?? {}) as Record<string, unknown>;
      const queries = Array.isArray(envelope['queries'])
        ? (envelope['queries'] as Record<string, unknown>[])
        : [];
      const p = (queries[0] ?? envelope) as unknown as LocalServerQuery;
      const suffix =
        p.action === 'serve'
          ? `${p.name ?? '?'} -> ${p.dir ?? '?'}`
          : p.name
            ? `${p.action} ${p.name}`
            : String(p.action ?? '');
      return buildToolView({
        name: 'localServer',
        state: 'request',
        segments: [
          { text: String(p.action ?? 'status'), token: 'bright' },
          ...(suffix && suffix !== p.action ? [{ text: suffix.replace(`${p.action} `, ''), token: p.action === 'serve' ? 'path' as const : 'dim' as const }] : []),
        ],
      }, theme);
    },

    renderResult(result: ToolCallResult, _opts: unknown, theme?: PiTheme) {
      const ok = !result.isError;
      const first =
        ((result.content.find((c) => c.type === 'text') as { text?: string } | undefined)?.text ?? '')
          .split('\n')
          .find(Boolean) ?? (ok ? 'localServer ok' : 'localServer failed');
      const details = result.details as Record<string, unknown> | undefined;
      const url = typeof details?.['url'] === 'string' ? details['url'] : '';
      return buildToolView({
        name: 'localServer',
        state: ok ? 'success' : 'error',
        segments: [
          ...(url ? [{ text: url, token: 'link' as const }] : []),
          { text: first, token: ok ? 'dim' : 'error' },
        ],
      }, theme);
    },
  } satisfies ToolDefinition);
}
