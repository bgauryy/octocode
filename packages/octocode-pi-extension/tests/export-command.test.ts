import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from 'vitest';
import {
  OCTOCODE_BRAND_MARKER,
  brandExportHtml,
  defaultImportExporter,
  registerExportCommand,
  type ExportCommandDeps,
} from '../src/tools/export-command.js';
import type { CommandDefinition, PiCommandContext, PiInstance } from '../src/types.js';

test('defaultImportExporter resolves pi export-html even though the exports map hides it', async () => {
  // pi's exports map only exposes "."/"./rpc-entry"/"./client", so a bare
  // package-subpath import of dist/core/export-html always throws
  // ERR_PACKAGE_PATH_NOT_EXPORTED. The exporter must resolve via the package's
  // exported main entry + a file-URL import instead of silently returning
  // undefined whenever the host pi package is actually installed.
  const mod = await defaultImportExporter();
  assert.ok(mod, 'exporter module must resolve from the installed pi package');
  assert.equal(typeof mod!.exportFromFile, 'function');
});

const FULL_HTML =
  '<!DOCTYPE html>\n<html>\n<head>\n<title>pi session</title>\n</head>\n' +
  '<body>\n<main>transcript</main>\n</body>\n</html>\n';

function countOccurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

interface Harness {
  handler: CommandDefinition;
  notifications: Array<{ message: string; level?: string }>;
  ctx: PiCommandContext;
  cwd: string;
}

function makeHarness(deps: ExportCommandDeps = {}, sessionFile?: string): Harness {
  const cwd = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-export-'));
  const commands = new Map<string, CommandDefinition>();
  const pi = {
    registerCommand: (name: string, def: CommandDefinition) => commands.set(name, def),
  } as unknown as PiInstance;
  registerExportCommand(pi, deps);
  const handler = commands.get('octocode-export');
  assert.ok(handler, 'command /octocode-export must be registered');

  const notifications: Array<{ message: string; level?: string }> = [];
  const ctx = {
    cwd,
    ui: { notify: (message: string, level?: string) => notifications.push({ message, level }) },
    sessionManager: { getSessionFile: () => sessionFile },
  } as unknown as PiCommandContext;
  return { handler: handler!, notifications, ctx, cwd };
}

// ─── brandExportHtml ─────────────────────────────────────────────────────────

test('brandExportHtml injects marker, title suffix, and badge exactly once (idempotent)', () => {
  const once = brandExportHtml(FULL_HTML);

  assert.equal(countOccurrences(once, OCTOCODE_BRAND_MARKER), 1);
  assert.match(once, /<title>pi session · octocode<\/title>/);
  assert.match(once, /octocode-brand-badge/);
  assert.match(once, /style="[^"]*position:fixed/);

  const twice = brandExportHtml(once);
  assert.equal(twice, once, 'double-branding must return the input unchanged');
  assert.equal(countOccurrences(twice, OCTOCODE_BRAND_MARKER), 1);
});

test('brandExportHtml degrades on head-less / body-less HTML fragments', () => {
  const fragment = '<p>just a fragment</p>';
  const branded = brandExportHtml(fragment);

  assert.ok(branded.startsWith(OCTOCODE_BRAND_MARKER), 'marker prepended at document start');
  assert.match(branded, /octocode-brand-badge/);
  assert.match(branded, /<p>just a fragment<\/p>/);
  assert.equal(brandExportHtml(branded), branded);

  // body but no head: marker + badge land inside the body.
  const bodyOnly = '<body><p>hi</p></body>';
  const brandedBody = brandExportHtml(bodyOnly);
  assert.equal(countOccurrences(brandedBody, OCTOCODE_BRAND_MARKER), 1);
  assert.ok(
    brandedBody.indexOf(OCTOCODE_BRAND_MARKER) > brandedBody.indexOf('<body>'),
    'marker inserted after <body>',
  );
  assert.match(brandedBody, /octocode-brand-badge/);
  assert.equal(brandExportHtml(brandedBody), brandedBody);
});

// ─── Command flow: injected exporter ─────────────────────────────────────────

test('/octocode-export uses the injected exporter and writes a branded sibling file', async () => {
  let exporterCalls = 0;
  const deps: ExportCommandDeps = {
    exporter: async (_sessionFile, ctx) => {
      exporterCalls += 1;
      const source = path.join(ctx.cwd!, 'pi-session-abc123.html');
      fs.writeFileSync(source, FULL_HTML, 'utf8');
      return source;
    },
  };
  const { handler, notifications, ctx, cwd } = makeHarness(deps, '/sessions/abc123.jsonl');

  await handler.handler('', ctx);

  assert.equal(exporterCalls, 1);
  const outPath = path.join(cwd, 'pi-session-abc123-octocode.html');
  assert.ok(fs.existsSync(outPath), `expected branded output at ${outPath}`);
  const written = fs.readFileSync(outPath, 'utf8');
  assert.equal(countOccurrences(written, OCTOCODE_BRAND_MARKER), 1);
  assert.match(written, /<title>pi session · octocode<\/title>/);
  assert.match(written, /octocode-brand-badge/);

  assert.equal(notifications.length, 1);
  assert.equal(notifications[0]!.level, 'info');
  assert.ok(notifications[0]!.message.includes(outPath), 'success notification carries the output path');
});

// ─── Command flow: explicit path argument ────────────────────────────────────

test('/octocode-export <path> brands an explicit fixture file', async () => {
  const { handler, notifications, ctx, cwd } = makeHarness();
  const fixture = path.join(cwd, 'my-export.html');
  fs.writeFileSync(fixture, FULL_HTML, 'utf8');

  await handler.handler(`  ${fixture}  `, ctx);

  const outPath = path.join(cwd, 'my-export-octocode.html');
  assert.ok(fs.existsSync(outPath));
  const written = fs.readFileSync(outPath, 'utf8');
  assert.equal(countOccurrences(written, OCTOCODE_BRAND_MARKER), 1);
  assert.match(written, /octocode-brand-badge/);
  // Source stays untouched.
  assert.equal(fs.readFileSync(fixture, 'utf8'), FULL_HTML);
  assert.equal(notifications[0]!.level, 'info');
});

// ─── Fallback scan ───────────────────────────────────────────────────────────

test('/octocode-export falls back to the newest pi-session-*.html in cwd', async () => {
  const { handler, notifications, ctx, cwd } = makeHarness({
    importExporter: async () => undefined, // deep import unavailable (the normal case)
  }, '/sessions/xyz.jsonl');

  const older = path.join(cwd, 'pi-session-older.html');
  const newer = path.join(cwd, 'pi-session-newer.html');
  fs.writeFileSync(older, FULL_HTML, 'utf8');
  fs.writeFileSync(newer, FULL_HTML, 'utf8');
  const now = Date.now();
  fs.utimesSync(older, new Date(now - 60_000), new Date(now - 60_000));
  fs.utimesSync(newer, new Date(now), new Date(now));

  await handler.handler('', ctx);

  assert.ok(fs.existsSync(path.join(cwd, 'pi-session-newer-octocode.html')));
  assert.ok(!fs.existsSync(path.join(cwd, 'pi-session-older-octocode.html')));
  assert.equal(notifications[0]!.level, 'info');
});

// ─── Error paths ─────────────────────────────────────────────────────────────

test('/octocode-export notifies a clear error when nothing is resolvable', async () => {
  const { handler, notifications, ctx } = makeHarness({
    importExporter: async () => undefined,
  });

  await handler.handler('', ctx);

  assert.equal(notifications.length, 1);
  assert.equal(notifications[0]!.level, 'error');
  assert.match(notifications[0]!.message, /run pi's own \/export first/i);
  assert.match(notifications[0]!.message, /\/octocode-export <file\.html>/);
});

test('/octocode-export rejects paths outside the allowed roots via the path guard', async () => {
  const { handler, notifications, ctx } = makeHarness();
  const denied = path.join(path.sep, 'octocode-denied-test-root', 'export.html');

  const savedAllowed = process.env['ALLOWED_PATHS'];
  delete process.env['ALLOWED_PATHS'];
  try {
    await handler.handler(denied, ctx);
  } finally {
    if (savedAllowed === undefined) delete process.env['ALLOWED_PATHS'];
    else process.env['ALLOWED_PATHS'] = savedAllowed;
  }

  assert.equal(notifications.length, 1);
  assert.equal(notifications[0]!.level, 'error');
  assert.match(notifications[0]!.message, /blocked/);
  assert.match(notifications[0]!.message, /outside the allowed roots/);
});
