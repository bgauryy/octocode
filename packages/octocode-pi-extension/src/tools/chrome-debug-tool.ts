/**
 * chromeDebug Pi tool — registration, schema, execute, and render.
 *
 * One tool that gives agents control over Chrome DevTools Protocol (CDP) through
 * a declarative scheme registry. A `raw` action exposes any Domain.method.
 * Screenshots/PDFs are written to <workspace>/.octocode/screenshots/.
 *
 * Mirrors the web-tool.ts + agent-tools.ts patterns:
 *   - in-process execution with AbortSignal
 *   - ctx.ui.setStatus() feedback
 *   - redaction at the return boundary
 *   - renderCall + renderResult for TUI
 */

import path from 'node:path';
import { connectToChrome, cleanupConnection, redactObject } from '../chrome-debug.js';
import { SCHEME_REGISTRY, SCHEMES, ACTIONS, STEALTH_SCRIPT } from '../chrome-debug-schemes.js';
import type { ChromeDebugParams, Scheme } from '../chrome-debug-schemes.js';
import type { ToolDefinition, ToolCallResult, PiTheme, PiContext } from '../types.js';
import type { registerUniqueTool } from './octocode-tools.js';
import { makeRenderer, truncateToWidth } from './render-helpers.js';

type TypeBoxBuilder = (typeof import('typebox'))['Type'];
type RegisterFn = typeof registerUniqueTool;

// ─── Status helpers ───────────────────────────────────────────────────────────

const STATUS_NAME = 'chrome-debug';

function setStatus(ctx: PiContext | undefined, msg: string | undefined): void {
  ctx?.ui?.setStatus?.(STATUS_NAME, msg);
}

// ─── Tool description ─────────────────────────────────────────────────────────

const DESCRIPTION = [
  'Chrome DevTools Protocol. All 28 schemes fully implemented:',
  'debug | network | console | dom | performance | screenshot | security | storage | intercept | automate | live-page | user-auth',
  'accessibility | workers | service-worker | websocket | supply-chain | consent | scrape | emulate | inject | monitor | login',
  'memory | css-coverage | js-coverage | full-audit | raw',
  '',
  'raw=ANY CDP Domain.Method (auto-enables domain) | stealth=bot-detection evasions | bypassCSP | scriptSource | xpath | depth',
  'url navigates first | port=9222 | launch=true | durationMs | selector | expression',
].join('\n');

// ─── Registration ─────────────────────────────────────────────────────────────

export function registerChromeDebugTool(
  pi: { registerTool?(def: ToolDefinition): void },
  Type: TypeBoxBuilder,
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
      'Start with scheme:"debug" for a combined pass (exceptions + network errors + DOM + screenshot).',
      'scheme:"raw" method:"Domain.Method" runs ANY CDP call; the domain is auto-enabled before the call.',
      'Each scheme auto-adds Debugger.setSkipAllPauses + dialog guard — safe on any page.',
      'Pass launch:true to start a fresh Chrome on the given port; each port gets its own profile dir.',
      'Screenshots → <workspace>/.octocode/screenshots/. Set OCTOCODE_CDP_DEBUG=1 for cdp-events.jsonl log.',
    ],
    parameters: Type.Object({
      scheme: Type.Unsafe({
        type: 'string',
        enum: [...SCHEMES],
        description: 'Smart prebuilt debug need. Use "raw" for any CDP Domain.method not covered by a scheme.',
      }),
      action: Type.Optional(
        Type.Unsafe({
          type: 'string',
          enum: [...ACTIONS],
          description: 'Verb within the scheme. Most schemes default to observe.',
        }),
      ),
      // Navigation / target
      url: Type.Optional(
        Type.String({ description: 'URL to navigate to before running the scheme recipe.' }),
      ),
      selector: Type.Optional(
        Type.String({ description: 'CSS selector for DOM-focused schemes.' }),
      ),
      expression: Type.Optional(
        Type.String({ description: 'JavaScript expression to evaluate (action: eval or live-page).' }),
      ),
      interact: Type.Optional(
        Type.Object(
          {
            click: Type.Optional(Type.String({ description: 'CSS selector to click.' })),
            fill: Type.Optional(
              Type.Object(
                {
                  selector: Type.String({ description: 'CSS selector of the input.' }),
                  value: Type.String({ description: 'Value to fill in.' }),
                },
                { description: 'Fill an input field.' },
              ),
            ),
            wait: Type.Optional(Type.String({ description: 'Wait duration in ms before other interact steps.' })),
          },
          { description: 'Browser interaction steps (click, fill, wait).' },
        ),
      ),
      // Raw action
      method: Type.Optional(
        Type.String({ description: 'CDP Domain.method for scheme:"raw". Example: "Network.getCookies".' }),
      ),
      params: Type.Optional(
        Type.Unsafe({ type: 'object', additionalProperties: true, description: 'CDP params object for scheme:"raw". Example: {"urls":["https://example.com"]} for Network.getCookies, {"query":"button"} for DOM.performSearch.' }),
      ),
      sessionId: Type.Optional(
        Type.String({ description: 'Route to a worker/iframe CDP session.' }),
      ),
      // Screenshot
      format: Type.Optional(
        Type.Unsafe({
          type: 'string',
          enum: ['png', 'jpeg', 'webp', 'pdf'],
          description: 'Screenshot format. "pdf" uses Page.printToPDF.',
        }),
      ),
      quality: Type.Optional(
        Type.Integer({
          minimum: 0,
          maximum: 100,
          description: 'JPEG quality (0-100).',
        }),
      ),
      clip: Type.Optional(
        Type.Object(
          {
            x: Type.Unsafe({ type: 'number' }),
            y: Type.Unsafe({ type: 'number' }),
            width: Type.Unsafe({ type: 'number' }),
            height: Type.Unsafe({ type: 'number' }),
            scale: Type.Optional(Type.Unsafe({ type: 'number' })),
          },
          { description: 'Clip region for screenshot.' },
        ),
      ),
      fullPage: Type.Optional(
        Type.Boolean({ description: 'Capture full page height (captureBeyondViewport).' }),
      ),
      // Emulate
      device: Type.Optional(
        Type.Object(
          {
            width: Type.Integer(),
            height: Type.Integer(),
            deviceScaleFactor: Type.Unsafe({ type: 'number' }),
            mobile: Type.Boolean(),
            userAgent: Type.Optional(Type.String()),
          },
          { description: 'Device metrics for emulation.' },
        ),
      ),
      throttle: Type.Optional(
        Type.Object(
          {
            offline: Type.Optional(Type.Boolean()),
            downloadThroughput: Type.Optional(Type.Unsafe({ type: 'number' })),
            uploadThroughput: Type.Optional(Type.Unsafe({ type: 'number' })),
            latency: Type.Optional(Type.Unsafe({ type: 'number' })),
          },
          { description: 'Network throttle conditions.' },
        ),
      ),
      // Session / lifecycle
      durationMs: Type.Optional(
        Type.Integer({ description: 'Observation window in ms for monitor/observe schemes. Default: 5000.' }),
      ),
      timeoutMs: Type.Optional(
        Type.Integer({ description: 'Per-call CDP timeout in ms. Default: 60000.' }),
      ),
      port: Type.Optional(
        Type.Integer({
          default: 9222,
          description: 'Chrome remote debugging port. Default: 9222.',
        }),
      ),
      targetId: Type.Optional(
        Type.String({ description: 'Attach to a specific CDP target by ID.' }),
      ),
      targetUrl: Type.Optional(
        Type.String({ description: 'Attach to a target whose URL contains this substring.' }),
      ),
      targetType: Type.Optional(
        Type.String({ description: 'Attach to a target of this type (page, worker, …).' }),
      ),
      newTab: Type.Optional(
        Type.String({ description: 'Open a new tab at this URL.' }),
      ),
      keepTab: Type.Optional(
        Type.Boolean({ description: 'Keep the target alive after the call. Default: true.' }),
      ),
      launch: Type.Optional(
        Type.Boolean({
          description:
            'Launch Chrome if not already running on the port. ' +
            'Always uses a non-default --user-data-dir (Chrome ≥136 requirement).',
        }),
      ),
      headless: Type.Optional(
        Type.Boolean({ description: 'Launch Chrome headless. Default: false (visible).' }),
      ),
      stealth: Type.Optional(
        Type.Boolean({ description: 'Inject stealth evasions before navigation: patches navigator.webdriver, window.chrome, plugins, vendor, hardwareConcurrency, permissions, WebGL. Use on sites with bot detection.' }),
      ),
      bypassCSP: Type.Optional(
        Type.Boolean({ description: 'Bypass Content-Security-Policy before script injection. Required for scheme:"inject" on CSP-protected sites.' }),
      ),
      scriptSource: Type.Optional(
        Type.String({ description: 'JavaScript source to inject via scheme:"inject"/"raw" (Page.addScriptToEvaluateOnNewDocument). Runs before any page script. Preferred over nesting in params to avoid JSON escaping fragility.' }),
      ),
      scriptFile: Type.Optional(
        Type.String({ description: 'Absolute path to a local .mjs file whose exported *SCRIPT constant (or full text) is injected. Avoids inline string escaping. Example: "/abs/path/stealth-inject.mjs".' }),
      ),
      depth: Type.Optional(
        Type.Integer({ description: 'Max results to return for scheme:"scrape" (default 50) or AX tree depth for scheme:"accessibility" (default -1 = full).' }),
      ),
      xpath: Type.Optional(
        Type.String({ description: 'XPath expression for scheme:"scrape". Evaluated alongside selector.' }),
      ),
      cleanup: Type.Optional(
        Type.Boolean({
          description: 'Close tabs opened by this call and, if the tool launched Chrome, terminate it.',
        }),
      ),
    }),

    async execute(
      _toolCallId: string,
      rawParams: Record<string, unknown>,
      signal?: AbortSignal,
      _onUpdate?: unknown,
      ctx?: PiContext,
    ): Promise<ToolCallResult> {
      const params = rawParams as unknown as ChromeDebugParams;
      const scheme = params.scheme as Scheme;
      const action = params.action ?? 'observe';
      const port = params.port ?? 9222;
      const keepTab = params.keepTab !== false; // default true
      const workspaceCwd = ctx?.cwd;

      const schemeEntry = SCHEME_REGISTRY[scheme];
      if (!schemeEntry) {
        throw new Error(`Unknown scheme: "${scheme}". Valid schemes: ${SCHEMES.join(', ')}`);
      }

      setStatus(ctx, `⧗ chromeDebug · ${scheme}/${action} · connecting on :${port}`);

      let connection;
      try {
        connection = await connectToChrome({
          port,
          targetId: params.targetId,
          targetUrl: params.targetUrl,
          targetType: params.targetType,
          newTab: params.newTab,
          launch: params.launch,
          headless: params.headless,
          timeoutMs: params.timeoutMs,
          signal,
          workspaceCwd,
        });
      } catch (err) {
        setStatus(ctx, undefined);
        throw new Error(`[CHROME_DEBUG_ERROR] ${(err as Error).message ?? String(err)}`);
      }

      const { session, version, metadata, screenshotDir } = connection;

      // Emit SESSION line
      const identity = metadata.identity;
      const sessionLine =
        `[SESSION] mode=${metadata.mode} browser=${version.Browser ?? 'unknown'} ` +
        `tab=${identity?.tabHost ?? '?'}${identity?.tabPath ?? ''} ` +
        `cookies=${(identity?.cookieNames ?? []).length} names`;

      setStatus(
        ctx,
        `⧗ chromeDebug · ${scheme}/${action} · target ${session.targetInfo.id.slice(0, 8)}`,
      );

      // Inject stealth evasions before navigation when requested
      if (params.stealth) {
        try {
          await session.send('Page.enable', {});
          await session.send('Page.addScriptToEvaluateOnNewDocument', { source: STEALTH_SCRIPT });
          setStatus(ctx, `⧗ chromeDebug · ${scheme}/${action} · stealth injected`);
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
          signal,
          setStatus: (msg) => setStatus(ctx, `⧗ chromeDebug · ${msg}`),
        });
      } catch (err) {
        const e = err as Error;
        setStatus(ctx, undefined);

        if (!keepTab || params.cleanup) {
          await cleanupConnection(session, keepTab, params.cleanup === true).catch(() => undefined);
        } else {
          session.close();
        }

        throw new Error(`[CHROME_DEBUG_ERROR] ${e.message} | target: ${JSON.stringify(metadata.activeTarget)}`);
      }

      // Cleanup
      if (params.cleanup) {
        // Full cleanup: close tab/WS AND terminate a Chrome this tool launched.
        await cleanupConnection(session, false, true).catch(() => undefined);
      } else if (!keepTab) {
        await cleanupConnection(session, false).catch(() => undefined);
      } else {
        // keepTab: just close the WS, leave the tab open
        session.close();
      }

      setStatus(ctx, undefined);

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
    },

    renderCall(args: unknown, theme?: PiTheme) {
      const a = (args ?? {}) as Record<string, unknown>;
      const scheme = typeof a['scheme'] === 'string' ? a['scheme'] : '?';
      const action = typeof a['action'] === 'string' ? a['action'] : '';
      const port = typeof a['port'] === 'number' ? a['port'] : 9222;
      const url = typeof a['url'] === 'string' ? a['url'] : typeof a['targetUrl'] === 'string' ? a['targetUrl'] : '';

      const nameStr = theme?.fg('toolTitle', theme.bold('chromeDebug')) ?? 'chromeDebug';
      const schemeStr = theme?.fg('accent', scheme) ?? scheme;
      const actionStr = action ? (theme?.fg('dim', `/${action}`) ?? `/${action}`) : '';
      const portStr = theme?.fg('dim', ` :${port}`) ?? ` :${port}`;
      const urlStr = url
        ? (theme?.fg('dim', ` · ${url.length > 50 ? url.slice(0, 47) + '…' : url}`) ?? ` · ${url}`)
        : '';

      const rawLine = `${nameStr} ${schemeStr}${actionStr}${portStr}${urlStr}`;
      return makeRenderer((w) => [truncateToWidth(rawLine, w)]);
    },

    renderResult(result: ToolCallResult, opts: { expanded?: boolean; isPartial?: boolean }, theme?: PiTheme) {
      if (opts.isPartial) {
        const msg = theme?.fg('warning', '⧗ Connecting to Chrome…') ?? '⧗ Connecting…';
        return makeRenderer((w) => [truncateToWidth(msg, w)]);
      }

      const ok = !result.isError;
      const icon = theme?.fg(ok ? 'success' : 'error', ok ? '✓' : '✗') ?? (ok ? '✓' : '✗');
      const nameStr = theme?.fg('toolTitle', 'chromeDebug') ?? 'chromeDebug';

      const det = result.details as Record<string, unknown> | null;
      const scheme = typeof det?.['scheme'] === 'string' ? det['scheme'] : '';
      const schemeStr = scheme ? (theme?.fg('dim', ` · ${scheme}`) ?? ` · ${scheme}`) : '';

      // Count [FINDING] lines
      const text = (result.content as Array<{ type: string; text: string }>)
        ?.find?.((p) => p.type === 'text')?.text ?? '';
      const findingCount = (text.match(/^\[FINDING\]/gm) ?? []).length;
      const screenshotPath = typeof det?.['screenshotPath'] === 'string' ? det['screenshotPath'] : '';

      let stat = '';
      if (findingCount > 0) {
        stat = theme?.fg('warning', ` · ${findingCount} finding${findingCount === 1 ? '' : 's'}`) ?? ` · ${findingCount} finding(s)`;
      } else if (screenshotPath) {
        const fname = path.basename(screenshotPath);
        stat = theme?.fg('dim', ` · ${fname}`) ?? ` · ${fname}`;
      }

      const header = `${icon} ${nameStr}${schemeStr}${stat}`;

      if (!opts.expanded) {
        const hint = theme?.fg('dim', ' · expand for evidence') ?? ' · expand for evidence';
        return makeRenderer((w) => [truncateToWidth(`${header}${hint}`, w)]);
      }

      const allLines = text.split('\n');
      const lines = allLines.slice(0, 30);
      const omitted = allLines.length - lines.length;

      return makeRenderer((w) => [
        truncateToWidth(header, w),
        ...lines.map((l) =>
          truncateToWidth(
            l.startsWith('[FINDING]')
              ? (theme?.fg('warning', l) ?? l)
              : l.startsWith('[ACTION]')
              ? (theme?.fg('accent', l) ?? l)
              : l.startsWith('[SESSION]')
              ? (theme?.fg('dim', l) ?? l)
              : (theme?.fg('dim', l) ?? l),
            w,
          ),
        ),
        ...(omitted > 0
          ? [truncateToWidth(theme?.fg('muted', `… ${omitted} more lines`) ?? `… ${omitted} more lines`, w)]
          : []),
      ]);
    },
  } satisfies ToolDefinition);
}
