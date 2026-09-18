import { afterEach, describe, expect, it } from 'vitest';
import { createWorkerMcpBroker, createWorkerBrokerClient, type WorkerMcpBroker } from '../src/tools/mcp/broker.js';
import type { CapabilitySnapshot } from '../src/contracts/capabilities.js';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawn } from 'node:child_process';
import { build } from 'esbuild';

const snapshot: CapabilitySnapshot = {
  schemaVersion: 1, revision: 'catalog-1', nativeTools: ['MCPTool', 'skill', 'bash'],
  skills: [{ id: 'skill-one', name: 'one', path: '/skills/one' }],
  mcpTools: [{ server: 'octocode', tool: 'localSearch' }, { server: 'private', tool: 'readSecret' }],
};
const brokers: WorkerMcpBroker[] = [];
afterEach(async () => { await Promise.all(brokers.splice(0).map(broker => broker.dispose())); });

describe('authenticated parent MCP broker', () => {
  it('uses a real local connection and dispatches only the authenticated worker grant', async () => {
    const calls: unknown[] = [];
    const broker = await createWorkerMcpBroker({ snapshot, dispatchMcp: async (params) => {
      calls.push(params);
      return { content: [{ type: 'text', text: 'allowed result with next.call continuation' }] };
    } });
    brokers.push(broker);
    const binding = broker.registerWorker('one', { nativeTools: ['MCPTool'], mcpTools: [{ server: 'octocode', tool: 'localSearch' }] });
    const client = createWorkerBrokerClient(binding);
    expect((await client.readCapabilities(true)).snapshot.mcpTools).toEqual([{ server: 'octocode', tool: 'localSearch' }]);
    expect((await client.dispatch({ action: 'call', server: 'octocode', tool: 'localSearch', args: { next: 2 } })).content[0]).toMatchObject({ text: expect.stringContaining('next.call') });
    await expect(client.dispatch({ action: 'call', server: 'private', tool: 'readSecret' })).rejects.toThrow(/not granted/);
    await expect(client.dispatch({ action: 'describe', server: 'private' })).rejects.toThrow(/not granted/);
    await expect(client.dispatch({ action: 'enable', server: 'private' })).rejects.toThrow(/parent-owned/);
    const listing = await client.dispatch({ action: 'describe' });
    expect(JSON.stringify(listing)).not.toContain('readSecret');
    const impostor = createWorkerBrokerClient({ ...binding, workerId: 'other' });
    await expect(impostor.readCapabilities()).rejects.toThrow(/authentication/i);
    expect(calls).toHaveLength(1);
  });

  it('revokes immediately, applies additions at the next turn, and fails stale grants closed', async () => {
    const broker = await createWorkerMcpBroker({ snapshot, dispatchMcp: async () => ({ content: [{ type: 'text', text: 'ok' }] }) });
    brokers.push(broker);
    const client = createWorkerBrokerClient(broker.registerWorker('one', { nativeTools: ['MCPTool'], mcpTools: [{ server: 'octocode', tool: 'localSearch' }] }));
    expect(() => broker.configureWorker('one', { snapshotRevision: 'stale', selection: { skills: ['skill-one'] } })).toThrow(/stale/);
    broker.configureWorker('one', { snapshotRevision: 'catalog-1', selection: { mcpTools: [], skills: ['skill-one'] } });
    await expect(client.dispatch({ action: 'call', server: 'octocode', tool: 'localSearch' })).rejects.toThrow(/not granted/);
    expect((await client.readCapabilities()).grant.skills).toEqual([]);
    expect((await client.readCapabilities(true)).grant.skills).toEqual(['skill-one']);
    broker.setSnapshot({ ...snapshot, revision: 'catalog-2', skills: [] });
    expect((await client.readCapabilities()).grant.skills).toEqual([]);
    broker.removeWorker('one');
    await expect(client.readCapabilities()).rejects.toThrow(/authentication/i);
  });

  it('pages the granted descriptors losslessly and keeps scoped continuations executable', async () => {
    const instructions = 'parent server guidance '.repeat(1200);
    const description = 'routing description '.repeat(1100);
    const catalog: CapabilitySnapshot = {
      ...snapshot,
      mcpTools: [
        { server: 'octocode', tool: 'localSearch', instructions, description, inputSchema: { type: 'object', properties: { secretSchemaMarker: { type: 'string' } } } },
        { server: 'octocode', tool: 'localFetch', instructions, description: 'Read exact source.' },
        { server: 'another', tool: 'allowedElsewhere', description: 'Outside the selected server.' },
        { server: 'private', tool: 'readSecret', description: 'Never granted.' },
      ],
    };
    const broker = await createWorkerMcpBroker({ snapshot: catalog, dispatchMcp: async () => { throw new Error('Catalog listing must not invoke a provider transport.'); } });
    brokers.push(broker);
    const client = createWorkerBrokerClient(broker.registerWorker('one', { nativeTools: ['MCPTool'], mcpTools: catalog.mcpTools.slice(0, 3).map(({ server, tool }) => ({ server, tool })) }));
    type Page = { partial: boolean; items: Array<{ kind: string; server: string; tool?: string; instructions?: string; description?: string }>; next?: { tool: string; params: { queries: Array<Record<string, unknown>> } }; diagnostic?: { code: string } };
    const readPage = async (query: Record<string, unknown>): Promise<Page> => {
      const result = await client.dispatch(query);
      const serialized = JSON.stringify(result);
      expect(serialized).not.toMatch(/readSecret|allowedElsewhere|inputSchema|secretSchemaMarker/);
      return JSON.parse((result.content[0] as { text: string }).text) as Page;
    };
    let page = await readPage({ action: 'list', server: 'octocode', limit: 1 });
    const initialNext = page.next!.params.queries[0]!;
    const values = new Map<string, string>();
    let pages = 0;
    for (;;) {
      for (const row of page.items) {
        const identity = `${row.server}/${row.tool ?? ''}`;
        values.set(identity, (values.get(identity) ?? '') + (row.kind === 'server' ? row.instructions ?? '' : row.description ?? ''));
      }
      if (!page.partial) break;
      expect(++pages).toBeLessThan(20);
      expect(page.next).toMatchObject({ tool: 'MCPTool', params: { queries: [{ action: 'list', server: 'octocode' }] } });
      page = await readPage(page.next!.params.queries[0]!);
    }
    expect(values).toEqual(new Map([['octocode/', instructions], ['octocode/localFetch', 'Read exact source.'], ['octocode/localSearch', description]]));
    broker.setSnapshot({ ...catalog, revision: 'catalog-2', mcpTools: catalog.mcpTools.map(item => item.server === 'octocode' ? { ...item, instructions: 'Changed instructions.' } : item) });
    const changed = await readPage(initialNext);
    expect(changed.diagnostic?.code).toBe('catalog-revision-changed');
    expect(changed.next).toMatchObject({ params: { queries: [{ action: 'list', server: 'octocode', offset: 0 }] } });
    await expect(client.dispatch({ action: 'list', server: 'private' })).rejects.toThrow(/not granted/);
  });

  it('refreshes parent enablement before accepting another worker call', async () => {
    let current = snapshot;
    let dispatched = 0;
    const broker = await createWorkerMcpBroker({ snapshot, refreshSnapshot: async () => current, dispatchMcp: async () => { dispatched += 1; return { content: [] }; } });
    brokers.push(broker);
    const client = createWorkerBrokerClient(broker.registerWorker('one', { nativeTools: ['MCPTool', 'skill'], skills: ['skill-one'], mcpTools: [{ server: 'octocode', tool: 'localSearch' }] }));
    await client.dispatch({ action: 'call', server: 'octocode', tool: 'localSearch' });
    current = { ...snapshot, revision: 'disabled', skills: [], mcpTools: [] };
    await expect(client.dispatch({ action: 'call', server: 'octocode', tool: 'localSearch' })).rejects.toThrow(/not granted/);
    expect((await client.readCapabilities()).snapshot.skills).toEqual([]);
    expect(dispatched).toBe(1);
  });

  it('runs the production capability client in a child process with private, filtered binding', async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-worker-client-'));
    try {
      const modulePath = path.join(root, 'worker-capabilities.mjs');
      await build({
        entryPoints: [path.resolve('src/tools/worker-capabilities.ts')], outfile: modulePath,
        bundle: true, platform: 'node', format: 'esm', logLevel: 'silent',
        // A native addon remains an installed runtime dependency, as in the
        // production tsc build. Resolve it from this fixture's real installation.
        plugins: [{ name: 'native-extension-dependency', setup(builder) {
          builder.onResolve({ filter: /^@octocodeai\/octocode-extension-rust$/ }, () => ({
            path: import.meta.resolve('@octocodeai/octocode-extension-rust'), external: true,
          }));
        } }],
      });
      const broker = await createWorkerMcpBroker({ snapshot, dispatchMcp: async () => ({ content: [{ type: 'text', text: 'parent connection reused' }] }) });
      brokers.push(broker);
      const binding = broker.registerWorker('child', { nativeTools: ['MCPTool', 'skill'], skills: ['skill-one'], mcpTools: [{ server: 'octocode', tool: 'localSearch' }] });
      const source = `
        const api = await import(${JSON.stringify(modulePath)});
        const view = await api.refreshCurrentWorkerCapabilities({beginTurn:true});
        const allowed = await api.dispatchWorkerMcpAction({action:'call',server:'octocode',tool:'localSearch'});
        let denied = false;
        try { await api.dispatchWorkerMcpAction({action:'call',server:'private',tool:'readSecret'}); } catch { denied = true; }
        let nativeDenied = false;
        try { api.assertCurrentWorkerNativeTool('bash'); } catch { nativeDenied = true; }
        process.stdout.write(JSON.stringify({workerId:view.grant.workerId,tools:view.snapshot.mcpTools,allowed,denied,nativeDenied,secretInEnvironment:process.env.OCTOCODE_WORKER_CAPABILITY_BINDING !== undefined}));
      `;
      const output = await new Promise<string>((resolve, reject) => {
        const child = spawn(process.execPath, ['--input-type=module', '-e', source], { env: { ...process.env, OCTOCODE_PI_SUBAGENT: '1', OCTOCODE_WORKER_CAPABILITY_BINDING: JSON.stringify(binding) }, stdio: ['ignore', 'pipe', 'pipe'] });
        let stdout = '';
        let stderr = '';
        child.stdout.on('data', chunk => { stdout += String(chunk); });
        child.stderr.on('data', chunk => { stderr += String(chunk); });
        child.once('error', reject);
        child.once('close', code => code === 0 ? resolve(stdout) : reject(new Error(`worker capability child failed: ${stderr}`)));
      });
      expect(JSON.parse(output)).toMatchObject({ workerId: 'child', tools: [{ server: 'octocode', tool: 'localSearch' }], denied: true, nativeDenied: true, secretInEnvironment: false });
      expect(output).not.toContain(binding.token);
      expect(output).not.toContain('readSecret');
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });
});
