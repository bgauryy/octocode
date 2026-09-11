import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ModelRegistry, ModelRuntime, VERSION } from '@earendil-works/pi-coding-agent';
import { createPiModelDiscovery, discoverPiModelSources } from '../src/adapters/pi-model-discovery.js';

const roots: string[] = [];
afterEach(() => { vi.unstubAllEnvs(); for (const root of roots.splice(0)) fs.rmSync(root, { recursive: true, force: true }); });
function fixture() {
  const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'pi-model-discovery-')));
  roots.push(root);
  const workspace = path.join(root, 'workspace');
  const octocodeHome = path.join(root, 'octocode');
  const piAgentDir = path.join(root, 'pi');
  for (const directory of [workspace, octocodeHome, piAgentDir, path.join(workspace, '.agents')]) fs.mkdirSync(directory, { recursive: true });
  const write = (file: string, providers: unknown) => fs.writeFileSync(file, JSON.stringify({ providers }));
  return { root, workspace, octocodeHome, piAgentDir, write };
}

describe('Pi model adapter', { concurrent: false }, () => {
  it('merges model IDs into real Pi 0.84.4, preserving native overrides, sibling models and active auth', async () => {
    expect(VERSION).toBe('0.84.4');
    vi.stubEnv('PI_OFFLINE', '1');
    const f = fixture();
    f.write(path.join(f.piAgentDir, 'models.json'), { openai: { models: [{ id: 'native-sibling', api: 'openai-responses', baseUrl: 'http://localhost:1' }], modelOverrides: { 'shared-fixture': { maxTokens: 777 } } } });
    f.write(path.join(f.octocodeHome, 'models.json'), { openai: { apiKey: '!touch SHOULD_NOT_EXECUTE', models: [{ id: 'shared-fixture', name: 'Global', contextWindow: 32000, maxTokens: 1000 }, { id: 'global-only' }] } });
    f.write(path.join(f.workspace, '.agents', 'models.json'), { openai: { models: [{ id: 'shared-fixture', name: 'Workspace', maxTokens: 2000 }] } });
    const runtime = await ModelRuntime.create({ authPath: path.join(f.piAgentDir, 'auth.json'), modelsPath: path.join(f.piAgentDir, 'models.json'), allowModelNetwork: false });
    await runtime.setRuntimeApiKey('openai', 'active-fixture-secret');
    const registry = new ModelRegistry(runtime);
    const originalCount = registry.getAll().filter(model => model.provider === 'openai').length;
    const adapter = createPiModelDiscovery(f);
    adapter.refresh(registry, { trusted: true });
    await registry.refresh({ allowNetwork: false });
    expect(registry.find('openai', 'shared-fixture')).toMatchObject({ name: 'Workspace', contextWindow: 32000, maxTokens: 777 });
    expect(registry.find('openai', 'native-sibling')).toBeDefined();
    expect(registry.find('openai', 'global-only')).toBeDefined();
    expect(registry.getAll().filter(model => model.provider === 'openai')).toHaveLength(originalCount + 2);
    expect(await registry.getApiKeyForProvider('openai')).toBe('active-fixture-secret');
    expect(JSON.stringify(adapter.snapshot())).not.toContain('SHOULD_NOT_EXECUTE');
    expect(JSON.stringify(adapter.snapshot())).not.toContain('active-fixture-secret');
    fs.rmSync(path.join(f.octocodeHome, 'models.json'));
    fs.rmSync(path.join(f.workspace, '.agents', 'models.json'));
    adapter.refresh(registry, { trusted: true });
    expect(registry.find('openai', 'global-only')).toBeUndefined();
    expect(registry.find('openai', 'native-sibling')).toBeDefined();
    expect(registry.getAll().filter(model => model.provider === 'openai')).toHaveLength(originalCount);
    adapter.dispose();
  });

  it('restores existing extension registrations and does not unregister another extension replacing its contribution', async () => {
    vi.stubEnv('PI_OFFLINE', '1');
    const f = fixture();
    const runtime = await ModelRuntime.create({ authPath: path.join(f.piAgentDir, 'auth.json'), modelsPath: null, allowModelNetwork: false });
    const registry = new ModelRegistry(runtime);
    registry.registerProvider('openai', { name: 'Existing extension', baseUrl: 'https://existing.example' });
    const original = registry.getRegisteredProviderConfig('openai');
    f.write(path.join(f.octocodeHome, 'models.json'), { openai: { models: [{ id: 'owned-fixture' }] } });
    const adapter = createPiModelDiscovery(f);
    adapter.refresh(registry, { trusted: true });
    adapter.dispose();
    expect(registry.getRegisteredProviderConfig('openai')).toEqual(original);
    const next = createPiModelDiscovery(f);
    next.refresh(registry, { trusted: true });
    registry.registerProvider('openai', { name: 'Later extension' });
    next.dispose();
    expect(registry.getRegisteredProviderConfig('openai')?.name).toBe('Later extension');
    expect(registry.find('openai', 'owned-fixture')).toBeDefined();
  });

  it('reports malformed, shadowed and unavailable entries without evaluating credentials or trusting workspace sources', () => {
    const f = fixture();
    f.write(path.join(f.octocodeHome, 'models.json'), { fixture: { apiKey: 'very-secret', api: 'openai-completions', baseUrl: 'https://secret:password@example.com', models: [{ id: 'same' }] } });
    f.write(path.join(f.workspace, '.agents', 'models.json'), { fixture: { models: [{ id: 'same' }] } });
    expect(discoverPiModelSources(f).sources.some(source => source.status === 'shadowed')).toBe(true);
    const adapter = createPiModelDiscovery(f);
    expect(adapter.refresh(undefined).errors.some(error => error.code === 'registry-unavailable')).toBe(true);
    expect(JSON.stringify(adapter.snapshot())).not.toContain('very-secret');
    fs.writeFileSync(path.join(f.octocodeHome, 'models.json'), '{"providers": "malformed-very-secret"');
    expect(discoverPiModelSources(f).errors[0]?.message).toBe('Invalid models JSON');
    adapter.dispose();
  });

  it('keeps credential commands inert and ignores untrusted workspace overrides in the real registry', async () => {
    vi.stubEnv('PI_OFFLINE', '1');
    const f = fixture();
    const marker = path.join(f.root, 'credential-command-ran');
    f.write(path.join(f.octocodeHome, 'models.json'), { openai: { apiKey: `!touch '${marker}'`, models: [{ id: 'reviewed-model', name: 'Global model' }] } });
    f.write(path.join(f.workspace, '.agents', 'models.json'), { openai: { models: [{ id: 'reviewed-model', name: 'Workspace model' }] } });
    const runtime = await ModelRuntime.create({ authPath: path.join(f.piAgentDir, 'auth.json'), modelsPath: null, allowModelNetwork: false });
    const registry = new ModelRegistry(runtime);
    const adapter = createPiModelDiscovery(f);
    adapter.refresh(registry, { trusted: false });
    await registry.refresh({ allowNetwork: false });
    expect(registry.find('openai', 'reviewed-model')?.name).toBe('Global model');
    expect(adapter.snapshot().sources.find(source => source.scope === 'user')?.status).toBe('active');
    expect(adapter.snapshot().sources.find(source => source.scope === 'workspace')?.status).toBe('untrusted');
    expect(fs.existsSync(marker)).toBe(false);
    adapter.dispose();
  });

  it('preserves native extension providers and exposes composition failures without retaining a broken provider', async () => {
    vi.stubEnv('PI_OFFLINE', '1');
    const f = fixture();
    const runtime = await ModelRuntime.create({ authPath: path.join(f.piAgentDir, 'auth.json'), modelsPath: null, allowModelNetwork: false });
    const registry = new ModelRegistry(runtime);
    const original = { ...registry.getProvider('openai')!, name: 'Native extension' };
    registry.registerProvider(original);
    f.write(path.join(f.octocodeHome, 'models.json'), { openai: { models: [{ id: 'native-added' }] }, 'broken-provider': { models: [{ id: 'no-api' }] } });
    const adapter = createPiModelDiscovery(f);
    const result = adapter.refresh(registry, { trusted: true });
    expect(registry.find('openai', 'native-added')).toBeDefined();
    expect(registry.getProvider('openai')?.auth).toBeDefined();
    expect(result.errors.some(error => error.code === 'provider-registration')).toBe(true);
    expect(registry.getRegisteredProviderIds()).not.toContain('broken-provider');
    adapter.dispose();
    expect(registry.getRegisteredNativeProvider('openai')).toBe(original);
    expect(registry.find('openai', 'native-added')).toBeUndefined();
  });
});
