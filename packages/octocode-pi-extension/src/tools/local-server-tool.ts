import fs from 'node:fs';
import path from 'node:path';
import { assertPathAllowed } from './path-guard.js';
import { resolveFilePath } from './file-state.js';
import {
  getLocalServerBaseUrl,
  listLocalServerMounts,
  serveDirectory,
  stopLocalServer,
  unmount,
} from './local-server.js';
import { buildToolView } from './render-helpers.js';
import { buildQueryEnvelopeSchema, executeQueryBatch } from './query-envelope.js';
import type { ToolCallResult, ToolDefinition, PiContext, PiTheme } from '../types.js';
import type { registerUniqueTool } from './octocode-tools.js';
import { openLocalUrl, type LocalUrlOpenPreference, type LocalUrlOpenResult } from './local-url-opener.js';

type TypeBoxBuilder = (typeof import('typebox'))['Type'];
type RegisterFn = typeof registerUniqueTool;

interface LocalServerQuery {
  reasoning: string;
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
  Type: TypeBoxBuilder,
  registeredToolNames: Set<string>,
  registerFn: RegisterFn,
  dependencies: LocalServerToolDependencies = {},
): void {
  const openUrl = dependencies.openUrl ?? ((url, preference) => openLocalUrl(url, { preference }));
  const querySchema = Type.Object(
    {
      action: Type.Unsafe({
        type: 'string',
        enum: ['serve', 'unmount', 'status', 'stop'],
        description: 'serve|unmount|status|stop',
      }),
      name: Type.Optional(Type.String({
        description: 'Mount name: one safe URL path segment. Required for serve/unmount.',
      })),
      dir: Type.Optional(Type.String({
        description: 'Directory to serve for action:serve. Relative paths resolve against cwd.',
      })),
      indexFile: Type.Optional(Type.String({
        description: 'File served at the mount root for action:serve. Default index.html.',
      })),
      open: Type.Optional(Type.Boolean({
        description: 'Open the mounted page only after the user explicitly asks or approves. Defaults to false in every mode.',
      })),
      browser: Type.Optional(Type.Unsafe({
        type: 'string',
        enum: ['auto', 'chrome', 'system', 'vscode', 'none'],
        description: 'Browser target for action:serve. auto prefers VS Code when available, then Chrome, then the system opener.',
      })),
    },
    { additionalProperties: false },
  );

  const parameters = buildQueryEnvelopeSchema(Type, querySchema, {
    reasoningDescription: 'Concise reason this local server operation is necessary.',
  });

  registerFn(pi, registeredToolNames, {
    name: 'localServer',
    label: 'Local Server',
    description: [
      'Serve local, agent-authored static artifacts over a shared loopback-only HTTP server.',
      'Actions: serve (mount a directory), unmount (remove one mount), status (show base URL and mounts), stop (stop server and clear mounts).',
      'Use for HTML plan/design/report artifacts when a browser view helps. Serving never opens a browser by default; pass open:true only after the user explicitly asks or approves.',
      'Browser routing: VS Code integrated browser when the extension-host API is available, otherwise Chrome, then the platform default browser.',
      'Security: static files only, bound to 127.0.0.1, mount names are a single URL segment, and served directories must pass the Octocode path guard (cwd/home/tmp/ALLOWED_PATHS).',
      'Pass one or more queries[] entries; each requires reasoning and an action.',
    ].join('\n'),
    promptSnippet: 'Serve local static artifacts over a loopback-only, path-guarded local server.',
    promptGuidelines: [
      'Use localServer for generated HTML/Markdown artifacts that are clearer in a browser (plans, design diagrams, reports).',
      'In the interactive TUI, action:serve opens the page by default. Use open:false when the user only wants the URL.',
      'Serve only directories you authored or inspected; never expose secrets, home directories wholesale, or untrusted downloads.',
      'Unmount or stop surfaces when they are no longer useful.',
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
        async execute(query) {
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
