/**
 * F10 — Branded session HTML export ('/octocode-export').
 *
 * Wiring needed in src/index.ts (do NOT edit index.ts from this module):
 *
 *   import { registerExportCommand } from './tools/export-command.js';
 *   // inside the extension activate/setup function:
 *   registerExportCommand(pi);
 *
 * Takes a pi session HTML export (produced by pi's own `/export` command, an
 * injected exporter, or a best-effort deep import of pi's export-html module),
 * injects octocode branding (title suffix, inline-CSS badge, idempotency
 * marker) and writes the result to a '<basename>-octocode.html' sibling file.
 *
 * Export acquisition priority:
 *   1. explicit path argument: '/octocode-export <file.html>'
 *   2. injected deps.exporter (tests use this)
 *   3. dynamic deep import of pi's export-html module — best-effort only; the
 *      module is NOT exported from the package root and the deep specifier is
 *      normally NOT resolvable at runtime, so this is wrapped in try/catch.
 *   4. newest 'pi-session-*.html' file in cwd (pi's `/export` naming:
 *      `${APP_NAME}-session-<sessionBasename>.html`, APP_NAME default "pi").
 */

import fs from 'node:fs';
import path from 'node:path';
import { extensionTmpRoot } from '../extension-paths.js';
import type { PiCommandContext, PiInstance } from '../types.js';
import { atomicWriteUtf8 } from './file-state.js';
import { assertPathAllowed } from './path-guard.js';
import { createSessionArtifactContext } from './session-artifacts.js';

// ─── Branding ─────────────────────────────────────────────────────────────────

/** Idempotency marker — presence means the document is already branded. */
export const OCTOCODE_BRAND_MARKER = '<!-- octocode-branded -->';

const TITLE_SUFFIX = ' · octocode';

/** Small branded badge — inline CSS only, no external assets. */
const BADGE_HTML =
  '<div class="octocode-brand-badge" style="position:fixed;top:8px;right:8px;z-index:9999;' +
  'font:600 11px/1.6 -apple-system,BlinkMacSystemFont,\'Segoe UI\',sans-serif;' +
  'color:#e6edf3;background:#1f2937;border:1px solid #374151;border-radius:9999px;' +
  'padding:2px 10px;letter-spacing:.04em;opacity:.92;">octocode</div>';

/**
 * Inject octocode branding into a pi export HTML document.
 *
 * - Appends ' · octocode' to the <title> (when present).
 * - Inserts the idempotency marker after <head> (or with the badge when
 *   the document has no <head>).
 * - Inserts a small inline-CSS badge after <body>.
 * - Degrades on malformed/partial HTML: with no <head>/<body> the marker and
 *   badge are prepended at document start.
 *
 * Idempotent: calling on already-branded HTML returns it unchanged.
 */
export function brandExportHtml(html: string): string {
  if (html.includes(OCTOCODE_BRAND_MARKER)) return html;

  let out = html.replace(
    /(<title[^>]*>)([\s\S]*?)(<\/title>)/i,
    (_m, open: string, text: string, close: string) => `${open}${text}${TITLE_SUFFIX}${close}`,
  );

  const insertAfter = (source: string, tagMatch: RegExpExecArray, insertion: string): string => {
    const at = tagMatch.index + tagMatch[0].length;
    return source.slice(0, at) + '\n' + insertion + source.slice(at);
  };

  const head = /<head[^>]*>/i.exec(out);
  const body = /<body[^>]*>/i.exec(out);

  if (head && body) {
    // Insert the later occurrence first so the earlier index stays valid.
    out = insertAfter(out, body, BADGE_HTML);
    out = insertAfter(out, head, OCTOCODE_BRAND_MARKER);
  } else if (body) {
    out = insertAfter(out, body, OCTOCODE_BRAND_MARKER + '\n' + BADGE_HTML);
  } else if (head) {
    out = insertAfter(out, head, OCTOCODE_BRAND_MARKER) + '\n' + BADGE_HTML;
  } else {
    out = OCTOCODE_BRAND_MARKER + '\n' + BADGE_HTML + '\n' + out;
  }
  return out;
}

// ─── Export acquisition ──────────────────────────────────────────────────────

export interface ExportCommandDeps {
  /**
   * Injected exporter: produce a pi-export HTML file for the current session
   * and return its absolute path. Highest priority after an explicit path arg.
   */
  exporter?: (sessionFile: string | undefined, ctx: PiCommandContext) => Promise<string>;
  /**
   * Seam over the best-effort dynamic deep import of pi's export-html module.
   * Returns the module (with exportFromFile) or undefined when unresolvable.
   */
  importExporter?: () => Promise<
    { exportFromFile?: (inputPath: string, options?: unknown) => Promise<string> } | undefined
  >;
}

/** Pi names its own exports `pi-session-<basename>.html`; skip our outputs. */
const PI_EXPORT_PATTERN = /^pi-session-.+\.html$/i;

export async function defaultImportExporter(): Promise<
  { exportFromFile?: (inputPath: string, options?: unknown) => Promise<string> } | undefined
> {
  type ExporterModule = { exportFromFile?: (inputPath: string, options?: unknown) => Promise<string> };
  // pi's exports map exposes only "."/"./rpc-entry"/"./client", so the deep
  // subpath below throws ERR_PACKAGE_PATH_NOT_EXPORTED under standard Node
  // resolution. Try it first anyway (jiti-style hosts may allow it), then fall
  // back to resolving the package's exported main entry and importing the
  // module by file URL — file-URL imports are not subject to the exports map.
  const specifier = '@earendil-works/pi-coding-agent/dist/core/export-html/index.js';
  try {
    return (await import(specifier)) as ExporterModule;
  } catch {
    // Fall through to file-URL resolution.
  }
  try {
    const mainEntry = import.meta.resolve('@earendil-works/pi-coding-agent');
    const target = new URL('./core/export-html/index.js', mainEntry).href;
    return (await import(target)) as ExporterModule;
  } catch {
    return undefined;
  }
}

/** Find the newest pi-export HTML file in `dir`, or undefined. */
function findNewestPiExport(dir: string): string | undefined {
  let names: string[];
  try {
    names = fs.readdirSync(dir);
  } catch {
    return undefined;
  }
  let newest: { file: string; mtime: number } | undefined;
  for (const name of names) {
    if (!PI_EXPORT_PATTERN.test(name)) continue;
    if (/-octocode\.html$/i.test(name)) continue; // skip our own outputs
    const full = path.join(dir, name);
    try {
      const st = fs.statSync(full);
      if (!st.isFile()) continue;
      if (!newest || st.mtimeMs > newest.mtime) newest = { file: full, mtime: st.mtimeMs };
    } catch {
      // unreadable entry — skip
    }
  }
  return newest?.file;
}

const NOTHING_RESOLVABLE_MESSAGE =
  'octocode-export: no session export HTML found. Run pi\'s own /export first, ' +
  'then run /octocode-export <file.html> (or run /octocode-export with no ' +
  'argument from the directory containing the pi-session-*.html export).';

/**
 * Resolve the source pi-export HTML file path per the acquisition priority.
 * Throws with a user-actionable message when nothing is resolvable.
 */
async function resolveSourceHtmlPath(
  arg: string,
  ctx: PiCommandContext,
  deps: ExportCommandDeps,
): Promise<string> {
  const cwd = ctx?.cwd ?? process.cwd();

  // 1. Explicit path argument.
  if (arg) {
    const explicit = path.resolve(cwd, arg);
    // Guard the read too — the branded sibling write lands in the same
    // directory, so an out-of-bounds source is rejected up front.
    assertPathAllowed(explicit, cwd, 'octocode-export read');
    if (!fs.existsSync(explicit)) {
      throw new Error(
        `octocode-export: file not found: ${explicit}. Run /export <file.html> first, then /octocode-export <file.html>.`,
      );
    }
    return explicit;
  }

  const sessionFile = ctx?.sessionManager?.getSessionFile?.();

  // 2. Injected exporter.
  if (deps.exporter) {
    return await deps.exporter(sessionFile, ctx);
  }

  // 3. Best-effort dynamic deep import of pi's export-html module.
  if (sessionFile) {
    try {
      const mod = await (deps.importExporter ?? defaultImportExporter)();
      if (mod?.exportFromFile) {
        const exportTmp = path.join(extensionTmpRoot(), 'exports');
        fs.mkdirSync(exportTmp, { recursive: true, mode: 0o700 });
        const tmpOut = path.join(exportTmp, `pi-session-${path.basename(sessionFile).replace(/\.[^.]+$/, '')}.html`);
        const written = await mod.exportFromFile(sessionFile, { outputPath: tmpOut });
        if (typeof written === 'string' && fs.existsSync(written)) return written;
      }
    } catch {
      // Deep import / export failed — fall through to the file-scan fallback.
    }
  }

  // 4. Newest pi-export HTML file in cwd.
  const newest = findNewestPiExport(cwd);
  if (newest) return newest;

  throw new Error(NOTHING_RESOLVABLE_MESSAGE);
}

// ─── Command ─────────────────────────────────────────────────────────────────

/** Compute the branded sibling output path: '<basename>-octocode.html'. */
function brandedOutputPath(sourcePath: string): string {
  const dir = path.dirname(sourcePath);
  const base = path.basename(sourcePath).replace(/\.html?$/i, '');
  // Re-branding an already-branded file overwrites it in place.
  if (/-octocode$/i.test(base)) return path.join(dir, `${base}.html`);
  return path.join(dir, `${base}-octocode.html`);
}

/**
 * Register '/octocode-export [path]' — brand a pi session HTML export with
 * octocode styling and write it to a '<basename>-octocode.html' sibling file.
 */
export function registerExportCommand(pi: PiInstance, deps: ExportCommandDeps = {}): void {
  pi.registerCommand?.('octocode-export', {
    description: 'Brand a pi session HTML export with octocode styling (run /export first, or pass a path)',
    handler: async (args, ctx) => {
      const cwd = ctx?.cwd ?? process.cwd();
      try {
        const sourcePath = await resolveSourceHtmlPath((args ?? '').trim(), ctx, deps);
        const outputPath = brandedOutputPath(sourcePath);
        assertPathAllowed(outputPath, cwd, 'octocode-export write');

        const html = fs.readFileSync(sourcePath, 'utf8');
        const branded = brandExportHtml(html);
        await atomicWriteUtf8(outputPath, branded);
        // Register the exported file in the session artifact manifest (best-effort).
        if (ctx?.sessionManager) {
          try {
            const artifactCtx = createSessionArtifactContext({ cwd, sessionManager: ctx.sessionManager });
            artifactCtx.writeJson('export/latest-ref.json', { outputPath, writtenAt: new Date().toISOString() });
            artifactCtx.registerProducer('export', 'export/latest-ref.json');
          } catch { /* best-effort */ }
        }
        ctx?.ui?.notify?.(`octocode export written: ${outputPath}`, 'info');
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        ctx?.ui?.notify?.(message, 'error');
      }
    },
  });
}
