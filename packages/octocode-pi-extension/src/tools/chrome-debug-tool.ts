import { truncateToWidth } from '../tui/width.js';
import { paint } from '../tui/palette.js';
/**
 * chromeDebug Pi tool — registration, schema, execute, and render.
 *
 * One tool that gives agents control over Chrome DevTools Protocol (CDP) through
 * a declarative scheme registry. A `raw` action exposes any Domain.method.
 * Screenshots are written to `$OCTOCODE_HOME/extension/sessions/<session-key>/browser/screenshots/`
 * (session-scoped). Session metadata lands at `browser/port-<N>/session.json` inside the
 * same session tree. A deterministic session identity is derived when the host does not provide one.
 *
 * Mirrors the web-tool.ts + agent-tools.ts patterns:
 *   - in-process execution with AbortSignal
 *   - renderer-managed status feedback
 *   - redaction at the return boundary
 *   - renderCall + renderResult for TUI
 */

import path from 'node:path';
import { connectToChrome, cleanupConnection, redactObject } from '../chrome-debug.js';
import { resolveSessionIdentity } from './session-artifacts.js';
import { connectionKey, getLiveConnection, cacheConnection, evictConnection } from '../chrome-connection-cache.js';
import { SCHEME_REGISTRY, SCHEMES, STEALTH_SCRIPT } from '../chrome-debug-schemes.js';
import type { ChromeDebugParams, Scheme } from '../chrome-debug-schemes.js';
import { CLI_STATUS_TEXT, cliStatusGlyph, cliStatusToken, cliToolTitle } from '../tui/cli-design.js';
import type { ToolDefinition, ToolCallResult, PiTheme, PiContext, RenderContext } from '../types.js';
import { appendImageLines } from './image-render.js';
import { DIRECT_TOOL_DESCRIPTIONS, type registerUniqueTool } from './octocode-tools.js';
import { buildQueryEnvelopeSchema, executeQueryBatch } from './query-envelope.js';
import { makeComponentRenderer } from './render-helpers.js';
import { setManagedStatus } from './runtime-renderer.js';

import { z } from 'zod';
type RegisterFn = typeof registerUniqueTool;

// ─── Status helpers ───────────────────────────────────────────────────────────

const STATUS_NAME = 'chrome-debug';

function setStatus(ctx: PiContext | undefined, msg: string | undefined): void {
  setManagedStatus(ctx, STATUS_NAME, msg);
}

// ─── Tool description ─────────────────────────────────────────────────────────

const DESCRIPTION = DIRECT_TOOL_DESCRIPTIONS.chromeDebug!;

// ─── Registration ─────────────────────────────────────────────────────────────

export function registerChromeDebugTool(
  pi: { registerTool?(def: ToolDefinition): void },
  registeredToolNames: Set<string>,
  registerFn: RegisterFn,
  _notify?: (ctx: PiContext | undefined, message: string, level?: string) => void,
): void {
  registerFn(pi, registeredToolNames, {
    name: 'chromeDebug',
    label: 'Chrome DevTools',
    description: DESCRIPTION,
    promptSnippet: 'Connect to Chrome DevTools Protocol to debug, inspect, and control a live browser',
    promptGuidelines: [
      'Use smallest scheme; scheme:"debug" for combined pass when failure boundary is unknown.',
      'scheme:"raw" method:"Domain.Method" runs any CDP call; domain auto-enabled before call.',
      'launch:true starts fresh Chrome on port; each port has its own profile dir.',
      'Screenshots saved to private session browser/screenshots; use the returned artifact path.',
    ],
    parameters: (() => {
      const itemSchema = z.looseObject({
        scheme: z.enum([
          'debug','network','console','dom','performance','screenshot',
          'intercept','security','storage','automate','live-page','user-auth',
          'raw','memory','css-coverage','js-coverage','websocket',
          'service-worker','workers','accessibility','supply-chain','full-audit',
          'consent','scrape','login','emulate','inject','monitor',
        ]).describe('Evidence/action scheme; raw accepts any CDP Domain.method.'),
        action: z.enum(['observe','capture','navigate','interact','wait','breakpoint',
          'resume','screenshot','eval','list-targets','attach','cleanup','raw']).optional()
          .describe('Verb within the scheme. Most schemes default to observe.'),
        url: z.string().optional().describe('URL to navigate to before running the scheme recipe.'),
        selector: z.string().optional().describe('CSS selector for DOM-focused schemes.'),
        expression: z.string().optional().describe('JavaScript expression to evaluate (action: eval or live-page).'),
        interact: z.object({
          click: z.string().optional().describe('CSS selector to click.'),
          fill: z.object({
            selector: z.string().describe('CSS selector of the input.'),
            value: z.string().describe('Value to fill in.'),
          }).optional(),
          wait: z.string().optional().describe('Wait duration in ms before other interact steps.'),
        }).optional(),
        clip: z.object({
          x: z.number(), y: z.number(), width: z.number(), height: z.number(),
          scale: z.number().optional(),
        }).optional().describe('Clip region for screenshot.'),
        format: z.enum(['png','jpeg','webp','pdf']).optional().describe('Screenshot format. "pdf" uses Page.printToPDF.'),
        quality: z.number().int().min(0).max(100).optional().describe('JPEG quality (0-100).'),
        fullPage: z.boolean().optional().describe('Capture full page height (captureBeyondViewport).'),
        device: z.object({
          width: z.number().int(),
          height: z.number().int(),
          deviceScaleFactor: z.number(),
          mobile: z.boolean(),
          userAgent: z.string().optional(),
        }).optional().describe('Device metrics for emulation.'),
        throttle: z.object({
          offline: z.boolean().optional(),
          downloadThroughput: z.number().optional(),
          uploadThroughput: z.number().optional(),
          latency: z.number().optional(),
        }).optional(),
        durationMs: z.number().int().optional().describe('Observation window in ms for monitor/observe schemes. Default: 5000.'),
        timeoutMs: z.number().int().optional().describe('Per-call CDP timeout in ms. Default: 60000.'),
        port: z.number().int().optional().describe('Chrome remote debugging port. Default: 9222.'),
        targetId: z.string().optional().describe('Attach to a specific CDP target by ID.'),
        targetUrl: z.string().optional().describe('Attach to a target whose URL contains this substring.'),
        targetType: z.string().optional().describe('Attach to a target of this type (page, worker, …).'),
        newTab: z.string().optional().describe('Open a new tab at this URL.'),
        keepTab: z.boolean().optional().describe('Keep the target alive after the call. Default: true.'),
        launch: z.boolean().optional().describe('Launch Chrome on port; uses non-default --user-data-dir (Chrome ≥136).'),
        headless: z.boolean().optional().describe('Launch Chrome headless. Default: false (visible).'),
        stealth: z.boolean().optional().describe('Inject stealth evasions before navigation.'),
        bypassCSP: z.boolean().optional().describe('Bypass CSP before injection; required for scheme:"inject" on CSP-protected sites.'),
        scriptSource: z.string().optional().describe('JS for scheme:"inject"/"raw". Runs before page scripts.'),
        scriptFile: z.string().optional().describe('Abs .mjs path; its *SCRIPT export (or full text) is injected.'),
        depth: z.number().int().optional().describe('scheme:"scrape" results (default 50) or scheme:"accessibility" AX depth (default -1=full).'),
        xpath: z.string().optional().describe('XPath expression for scheme:"scrape". Evaluated alongside selector.'),
        cleanup: z.boolean().optional().describe('Close tabs opened by this call and, if the tool launched Chrome, terminate it.'),
        method: z.string().optional().describe('CDP Domain.method for scheme:"raw". Example: "Network.getCookies".'),
        params: z.record(z.string(), z.unknown()).optional().describe('CDP params object for scheme:"raw".'),
        sessionId: z.string().optional().describe('Route to a worker/iframe CDP session.'),
      });
      return buildQueryEnvelopeSchema(itemSchema, {
        reasoningDescription: 'Concise reason this Chrome DevTools Protocol operation is necessary.',
      });
    })(),

    async execute(
      toolCallId: string,
      rawParams: Record<string, unknown>,
      signal?: AbortSignal,
      onUpdate?: unknown,
      ctx?: PiContext,
    ): Promise<ToolCallResult> {
      return executeQueryBatch({
        toolCallId,
        raw: rawParams,
        signal,
        onUpdate: typeof onUpdate === 'function' ? (onUpdate as (update: ToolCallResult) => void) : undefined,
        ctx,
        passthroughSingle: true,
        preflight(query) {
          if (!SCHEMES.includes(query['scheme'] as Scheme)) throw new Error(`Unknown scheme: "${query['scheme']}". Valid schemes: ${SCHEMES.join(', ')}`);
          if (query['scheme'] === 'raw' && (typeof query['method'] !== 'string' || !query['method'].trim())) throw new Error('scheme:"raw" requires method:"Domain.method"');
        },
        async execute(query, _index, _itemToolCallId, itemSignal, _itemOnUpdate, itemCtx) {
      const params = query as unknown as ChromeDebugParams;
      const scheme = params.scheme as Scheme;
      const action = params.action ?? 'observe';
      const port = params.port ?? 9222;
      const keepTab = params.keepTab !== false; // default true
      const workspaceCwd = itemCtx?.cwd;

      const schemeEntry = SCHEME_REGISTRY[scheme];
      if (!schemeEntry) {
        throw new Error(`Unknown scheme: "${scheme}". Valid schemes: ${SCHEMES.join(', ')}`);
      }

      setStatus(itemCtx, `⧗ chromeDebug · ${scheme}/${action} · connecting on :${port}`);

      // Reuse a live CDP connection across calls (keyed by port+target) so stateful
      // flows and injected state survive; a fresh tab (newTab) always connects anew.
      const cacheKey = connectionKey(port, params.targetId ?? params.targetUrl ?? params.targetType);
      const reusable = params.newTab ? undefined : getLiveConnection(cacheKey);
      let connection;
      let reused = false;
      if (reusable) {
        connection = reusable;
        reused = true;
      } else {
        try {
          const sessionKey = resolveSessionIdentity({
            cwd: workspaceCwd,
            sessionManager: itemCtx?.sessionManager,
          }).sessionKey;
          connection = await connectToChrome({
            port,
            targetId: params.targetId,
            targetUrl: params.targetUrl,
            targetType: params.targetType,
            newTab: params.newTab,
            launch: params.launch,
            headless: params.headless,
            timeoutMs: params.timeoutMs,
            signal: itemSignal,
            workspaceCwd,
            sessionKey,
          });
        } catch (err) {
          setStatus(itemCtx, undefined);
          throw new Error(`[CHROME_DEBUG_ERROR] ${(err as Error).message ?? String(err)}`);
        }
        // Cache non-ephemeral connections for reuse and so shutdown can close them
        // (includes newTab: it stays open under keepTab, so it must be tracked).
        if (keepTab && !params.cleanup) cacheConnection(cacheKey, port, connection);
      }

      const { session, version, metadata, screenshotDir } = connection;

      // Emit SESSION line
      const identity = metadata.identity;
      const sessionLine =
        `[SESSION] mode=${reused ? 'reused' : metadata.mode} browser=${version.Browser ?? 'unknown'} ` +
        `tab=${identity?.tabHost ?? '?'}${identity?.tabPath ?? ''} ` +
        `cookies=${(identity?.cookieNames ?? []).length} names`;

      setStatus(
        itemCtx,
        `⧗ chromeDebug · ${scheme}/${action} · target ${session.targetInfo.id.slice(0, 8)}`,
      );

      // Inject stealth evasions before navigation when requested
      if (params.stealth) {
        try {
          await session.send('Page.enable', {});
          await session.send('Page.addScriptToEvaluateOnNewDocument', { source: STEALTH_SCRIPT });
          setStatus(itemCtx, `⧗ chromeDebug · ${scheme}/${action} · stealth injected`);
        } catch {
          // Non-fatal — page may not need it
        }
      }

      let result;
      try {
        result = await schemeEntry.recipe({
          session,
          params,
          screenshotDir,
          signal: itemSignal,
          setStatus: (msg) => setStatus(itemCtx, `⧗ chromeDebug · ${msg}`),
        });
      } catch (err) {
        const e = err as Error;
        setStatus(itemCtx, undefined);

        if (!keepTab || params.cleanup) {
          await cleanupConnection(session, keepTab, params.cleanup === true).catch(() => undefined);
          evictConnection(cacheKey);
        }
        // keepTab: leave the connection cached/open so a retry can reuse it.

        throw new Error(`[CHROME_DEBUG_ERROR] ${e.message} | target: ${JSON.stringify(metadata.activeTarget)}`);
      }

      // Cleanup
      if (params.cleanup) {
        // Full cleanup: close tab/WS AND terminate a Chrome this tool launched.
        await cleanupConnection(session, false, true).catch(() => undefined);
        evictConnection(cacheKey);
      } else if (!keepTab) {
        await cleanupConnection(session, false).catch(() => undefined);
        evictConnection(cacheKey);
      } else {
        // keepTab (default): keep the CDP connection OPEN and cached so the next
        // call reuses the same session (stateful flows + no reconnect latency).
        // Leak safety: closeAllChromeConnections() runs on session_shutdown and
        // the cache is LRU-capped + prunes closed sessions.
      }

      setStatus(itemCtx, undefined);

      // Build final text — session line first, then evidence
      const allLines = [sessionLine, ...result.evidenceLines];
      const text = allLines.join('\n');

      // Redact the entire details object before returning
      const safeDetails = redactObject({
        scheme,
        action,
        port,
        browser: version.Browser,
        mode: metadata.mode,
        target: metadata.activeTarget,
        session: {
          identity: {
            mode: identity?.mode,
            browser: identity?.browser,
            userAgent: identity?.userAgent,
            tabHost: identity?.tabHost,
            tabPath: identity?.tabPath,
            cookieNames: identity?.cookieNames,
            // Never: userDataDir value, cookie values, auth tokens
          },
        },
        screenshotPath: (result.details as Record<string, unknown>)?.['screenshotPath'],
        ...result.details,
      });

      return {
        content: [{ type: 'text', text }],
        details: safeDetails,
      };
        }, // end inner execute
      }); // end executeQueryBatch
    },

    renderCall(args: unknown, theme?: PiTheme) {
      const envelope = (args ?? {}) as Record<string, unknown>;
      const queryList = Array.isArray(envelope['queries']) ? envelope['queries'] as Record<string, unknown>[] : [];
      const a = queryList[0] ?? {};
      const scheme = typeof a['scheme'] === 'string' ? a['scheme'] : '?';
      const action = typeof a['action'] === 'string' ? a['action'] : '';
      const port = typeof a['port'] === 'number' ? a['port'] : 9222;
      const url = typeof a['url'] === 'string' ? a['url'] : typeof a['targetUrl'] === 'string' ? a['targetUrl'] : '';
      const more = queryList.length > 1 ? paint(theme, 'dim', ` +${queryList.length - 1}`) : '';

      const nameStr = cliToolTitle(theme, 'chromeDebug', { bold: true });
      const schemeStr = paint(theme, 'link', scheme);
      const actionStr = action ? paint(theme, 'dim', `/${action}`) : '';
      const portStr = paint(theme, 'dim', ` :${port}`);
      const displayUrl = url.length > 50 ? `${url.slice(0, 47)}…` : url;
      const urlStr = url
        ? paint(theme, 'dim', ` · ${displayUrl}`)
        : '';

      const rawLine = `${nameStr} ${schemeStr}${actionStr}${portStr}${urlStr}${more}`;
      return makeComponentRenderer((_props, { width: w }) => [truncateToWidth(rawLine, w)], undefined);
    },

    renderResult(result: ToolCallResult, opts: { expanded?: boolean; isPartial?: boolean }, theme?: PiTheme, context?: RenderContext) {
      if (opts.isPartial) {
        const msg = paint(theme, 'brand', CLI_STATUS_TEXT.connectingChrome);
        return makeComponentRenderer((_props, { width: w }) => [truncateToWidth(msg, w)], undefined);
      }

      const ok = !result.isError;
      const icon = paint(theme, cliStatusToken(ok), cliStatusGlyph(ok));
      const nameStr = cliToolTitle(theme, 'chromeDebug');

      const det = result.details as Record<string, unknown> | null;
      const scheme = typeof det?.['scheme'] === 'string' ? det['scheme'] : '';
      const schemeStr = scheme ? paint(theme, 'dim', ` · ${scheme}`) : '';

      // Count [FINDING] lines
      const text = (result.content as Array<{ type: string; text: string }>)
        ?.find?.((p) => p.type === 'text')?.text ?? '';
      const findingCount = (text.match(/^\[FINDING\]/gm) ?? []).length;
      const screenshotPath = typeof det?.['screenshotPath'] === 'string' ? det['screenshotPath'] : '';

      let stat = '';
      if (findingCount > 0) {
        stat = paint(theme, 'count', ` · ${findingCount} finding${findingCount === 1 ? '' : 's'}`);
      } else if (screenshotPath) {
        const fname = path.basename(screenshotPath);
        stat = paint(theme, 'dim', ` · ${fname}`);
      }

      const header = `${icon} ${nameStr}${schemeStr}${stat}`;

      if (!opts.expanded) {
        // Errors have no evidence to expand into — show the first error line
        // inline instead of a misleading "expand for evidence" hint.
        const hint = !ok
          ? paint(theme, 'error', ` · ${text.split('\n').find(Boolean) ?? 'failed'}`)
          : paint(theme, 'dim', ' · expand for evidence');
        return makeComponentRenderer((_props, { width: w }) => [truncateToWidth(`${header}${hint}`, w)], undefined);
      }

      const allLines = text.split('\n');
      const lines = allLines.slice(0, 30);
      const omitted = allLines.length - lines.length;

      const base = makeComponentRenderer((_props, { width: w }) => [
        truncateToWidth(header, w),
        ...lines.map((l) =>
          truncateToWidth(
            !ok
              ? paint(theme, 'error', l)
              : l.startsWith('[FINDING]')
              ? paint(theme, 'warning', l)
              : l.startsWith('[ACTION]')
              ? paint(theme, 'link', l)
              : paint(theme, 'dim', l),
            w,
          ),
        ),
        ...(omitted > 0
          ? [truncateToWidth(paint(theme, 'muted', `… ${omitted} more lines`), w)]
          : []),
      ], undefined);

      // Inline screenshot in the expanded view. appendImageLines keeps the image
      // escape lines outside component width truncation (see image-render.ts).
      return screenshotPath
        ? appendImageLines(base, context, screenshotPath, theme)
        : base;
    },
  } satisfies ToolDefinition);
}
