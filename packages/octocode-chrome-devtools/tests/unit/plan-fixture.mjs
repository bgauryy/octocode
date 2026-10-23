// Shared fake CDP connection for executor plan tests: records sends, emits events and
// reads the artifacts a run writes into its temporary output directory.
import { mkdtempSync, writeFileSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

export const call = (method, params = {}, extra = {}) => ({
  op: 'cdp',
  method,
  params,
  ...extra,
});
export async function fixture(fn) {
  const dir = mkdtempSync(join(tmpdir(), 'octo-plan-')),
    handlers = new Map(),
    sent = [];
  const cdp = {
    outputDir: dir,
    targetInfo: { id: 'root', type: 'browser' },
    on(event, handler) {
      if (!handlers.has(event)) handlers.set(event, new Set());
      handlers.get(event).add(handler);
    },
    off(event, handler) {
      handlers.get(event)?.delete(handler);
    },
    send: async (method, params, sessionId) => {
      sent.push({ method, params, sessionId });
      return {};
    },
    protocol: async () => ({
      domains: [
        {
          domain: 'Runtime',
          commands: [
            { name: 'evaluate', returns: [{ $ref: 'Debugger.ScriptId' }] },
          ],
          types: [],
        },
        { domain: 'Debugger', types: [{ id: 'ScriptId', type: 'string' }] },
      ],
    }),
    saveArtifact(name, data) {
      const file = join(dir, name);
      writeFileSync(file, JSON.stringify(data));
      return { file };
    },
  };
  const emit = (event, params, meta = {}) => {
    for (const handler of handlers.get(event) ?? []) handler(params, meta);
  };
  const read = name => JSON.parse(readFileSync(join(dir, name), 'utf8'));
  const previous = process.exitCode;
  try {
    await fn({ cdp, dir, emit, sent, read });
  } finally {
    process.exitCode = previous;
    rmSync(dir, { recursive: true, force: true });
  }
}
