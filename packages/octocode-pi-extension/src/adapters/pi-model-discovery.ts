import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { getOctocodeHome } from '@octocodeai/config';
import { capabilitySourcePaths } from '../contracts/capability-sources.js';
import type { ModelRegistry } from '@earendil-works/pi-coding-agent';

type ProviderConfigInput = Parameters<ModelRegistry['registerProvider']>[1];

type NativeProvider = NonNullable<ReturnType<ModelRegistry['getRegisteredNativeProvider']>>;
type RegisteredModel = NonNullable<ProviderConfigInput['models']>[number];
type ModelDefinition = Partial<RegisteredModel> & { id: string };
type ProviderDefinition = Omit<ProviderConfigInput, 'models' | 'oauth' | 'streamSimple' | 'refreshModels'> & {
  models?: ModelDefinition[];
  modelOverrides?: Record<string, Partial<RegisteredModel>>;
  compat?: RegisteredModel['compat'];
};

export interface PiModelDiscoveryOptions {
  readonly workspace: string;
  readonly octocodeHome?: string;
  readonly piAgentDir?: string;
  readonly homeDir?: string;
}

export interface PiModelSourceSummary {
  readonly id: string;
  readonly path: string;
  readonly scope: 'user' | 'workspace';
  readonly provider: string;
  readonly modelIds: readonly string[];
  readonly status: 'discovered' | 'active' | 'shadowed' | 'untrusted' | 'unavailable';
}

export interface PiModelDiscoveryError {
  readonly path: string;
  readonly code: 'parse' | 'unavailable' | 'registry-unavailable' | 'provider-registration' | 'shadowed';
  readonly message: string;
}

interface SourceDefinition extends PiModelSourceSummary { readonly definition: ProviderDefinition; }
interface ModelSources {
  readonly sources: PiModelSourceSummary[];
  readonly errors: PiModelDiscoveryError[];
  readonly definitions: SourceDefinition[];
  readonly revision: string;
}

function record(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

const hash = (value: unknown): string => createHash('sha256').update(JSON.stringify(value)).digest('hex');

function validateModel(value: unknown, override = false): void {
  if (!record(value) || (!override && (typeof value['id'] !== 'string' || !value['id'].trim()))) throw new Error('Model definitions require a nonempty id');
  for (const field of ['name', 'api', 'baseUrl']) if (value[field] !== undefined && (typeof value[field] !== 'string' || !value[field].trim())) throw new Error(`Model ${field} must be a nonempty string`);
  for (const field of ['contextWindow', 'maxTokens']) if (value[field] !== undefined && (typeof value[field] !== 'number' || !Number.isFinite(value[field]) || value[field] <= 0)) throw new Error(`Model ${field} must be a positive number`);
  if (value['reasoning'] !== undefined && typeof value['reasoning'] !== 'boolean') throw new Error('Model reasoning must be boolean');
  if (value['input'] !== undefined && (!Array.isArray(value['input']) || value['input'].some(item => item !== 'text' && item !== 'image'))) throw new Error('Model input must contain text or image');
  if (value['headers'] !== undefined && (!record(value['headers']) || Object.values(value['headers']).some(item => typeof item !== 'string'))) throw new Error('Model headers must map strings to strings');
  for (const field of ['cost', 'compat', 'thinkingLevelMap', 'samplingParams']) if (value[field] !== undefined && !record(value[field])) throw new Error(`Model ${field} must be an object`);
  if (record(value['cost']) && Object.values(value['cost']).some(item => typeof item !== 'number' || !Number.isFinite(item) || item < 0)) throw new Error('Model costs must be finite nonnegative numbers');
}

function parseProviders(raw: string): Record<string, ProviderDefinition> {
  let input: unknown;
  try { input = JSON.parse(raw); } catch { throw new Error('Invalid models JSON'); }
  if (!record(input) || !record(input['providers'])) throw new Error('Models JSON requires a providers object');
  for (const [provider, definition] of Object.entries(input['providers'])) {
    if (!provider.trim() || !record(definition)) throw new Error('Provider definitions must be objects with nonempty names');
    for (const field of ['name', 'api', 'baseUrl', 'apiKey']) if (definition[field] !== undefined && typeof definition[field] !== 'string') throw new Error(`Provider ${field} must be a string`);
    if (definition['oauth'] !== undefined) throw new Error('Configure OAuth with Pi; declarative model sources preserve the active Pi authentication');
    if (definition['streamSimple'] !== undefined || definition['refreshModels'] !== undefined) throw new Error('Models JSON supports declarative provider configuration only');
    if (definition['headers'] !== undefined && (!record(definition['headers']) || Object.values(definition['headers']).some(value => typeof value !== 'string'))) throw new Error('Provider headers must map strings to strings');
    if (definition['compat'] !== undefined && !record(definition['compat'])) throw new Error('Provider compat must be an object');
    if (definition['authHeader'] !== undefined && typeof definition['authHeader'] !== 'boolean') throw new Error('Provider authHeader must be boolean');
    if (definition['models'] !== undefined && !Array.isArray(definition['models'])) throw new Error('Provider models must be an array');
    const ids = new Set<string>();
    for (const model of definition['models'] as unknown[] ?? []) {
      validateModel(model);
      const id = (model as ModelDefinition).id;
      if (ids.has(id)) throw new Error('A provider cannot define the same model id twice');
      ids.add(id);
    }
    if (definition['modelOverrides'] !== undefined) {
      if (!record(definition['modelOverrides'])) throw new Error('Provider modelOverrides must be an object');
      for (const override of Object.values(definition['modelOverrides'])) validateModel(override, true);
    }
  }
  return input['providers'] as Record<string, ProviderDefinition>;
}

function modelSources(options: PiModelDiscoveryOptions): ModelSources {
  const definitions: SourceDefinition[] = [];
  const errors: PiModelDiscoveryError[] = [];
  const workspace = path.resolve(options.workspace);
  const octocodeHome = path.resolve(options.octocodeHome ?? getOctocodeHome());
  for (const [sourcePath, scope] of [[path.join(octocodeHome, 'models.json'), 'user'], [path.join(workspace, '.agents', 'models.json'), 'workspace']] as const) {
    try {
      if (!fs.existsSync(sourcePath)) continue;
      const stat = fs.lstatSync(sourcePath);
      if (!stat.isFile() || stat.isSymbolicLink() || stat.size > 1_048_576) throw new Error('Models source must be a regular file smaller than 1 MiB');
      if (scope === 'workspace') {
        const relative = path.relative(fs.realpathSync(workspace), fs.realpathSync(sourcePath));
        if (relative.startsWith('..') || path.isAbsolute(relative)) throw new Error('Models source escapes the workspace');
      }
      for (const [provider, definition] of Object.entries(parseProviders(fs.readFileSync(sourcePath, 'utf8')))) {
        definitions.push({ id: `sha256:${hash([scope, sourcePath, provider])}`, path: sourcePath, scope, provider,
          modelIds: [...new Set([...(definition.models?.map(model => model.id) ?? []), ...Object.keys(definition.modelOverrides ?? {})])],
          status: 'discovered', definition });
      }
    } catch (error) {
      errors.push({ path: sourcePath, code: error instanceof Error && 'code' in error ? 'unavailable' : 'parse', message: error instanceof Error && !('code' in error) ? error.message : 'Models source is unavailable' });
    }
  }
  const sources = definitions.map(({ definition: _definition, ...source }) => {
    const workspaceDefinition = definitions.find(entry => entry.scope === 'workspace' && entry.provider === source.provider);
    return source.scope === 'user' && workspaceDefinition && source.modelIds.length > 0 && source.modelIds.every(id => workspaceDefinition.modelIds.includes(id))
      ? { ...source, status: 'shadowed' as const } : source;
  });
  return { sources, definitions, errors, revision: hash(definitions) };
}

/** A secret-free read-only projection. Credential expressions remain inert data. */
export function discoverPiModelSources(options: PiModelDiscoveryOptions) {
  const { sources, errors } = modelSources(options);
  return { sources, errors };
}

function mergeModel(base: Partial<RegisteredModel> | undefined, override: Partial<RegisteredModel>): Partial<RegisteredModel> {
  return { ...base, ...override,
    ...(base?.cost || override.cost ? { cost: { ...base?.cost, ...override.cost } as RegisteredModel['cost'] } : {}),
    ...(base?.compat || override.compat ? { compat: { ...base?.compat, ...override.compat } } : {}),
    ...(base?.headers || override.headers ? { headers: { ...base?.headers, ...override.headers } } : {}),
  };
}

function providerConfig(provider: string, definitions: SourceDefinition[], baseline: ReturnType<ModelRegistry['getAll']>, original?: ProviderConfigInput): ProviderConfigInput {
  let merged: ProviderDefinition = {};
  const declarations = new Map<string, ModelDefinition>();
  const overrides = new Map<string, Partial<RegisteredModel>>();
  for (const source of definitions.filter(entry => entry.provider === provider)) {
    merged = { ...merged, ...source.definition,
      headers: { ...merged.headers, ...source.definition.headers }, compat: { ...merged.compat, ...source.definition.compat } };
    for (const definition of source.definition.models ?? []) declarations.set(definition.id, mergeModel(declarations.get(definition.id), definition) as ModelDefinition);
    for (const [id, override] of Object.entries(source.definition.modelOverrides ?? {})) overrides.set(id, mergeModel(overrides.get(id), override));
  }
  const models = new Map<string, RegisteredModel>(baseline.filter(model => model.provider === provider).map(model => [model.id, { ...model, ...(original?.models?.find(entry => entry.id === model.id)?.headers ? { headers: original.models.find(entry => entry.id === model.id)!.headers } : {}) }]));
  const defaults = models.values().next().value as RegisteredModel | undefined;
  for (const [id, definition] of declarations) {
    const prior = models.get(id);
    models.set(id, {
      id, name: definition.name ?? prior?.name ?? id,
      api: definition.api ?? merged.api ?? prior?.api ?? defaults?.api,
      baseUrl: definition.baseUrl ?? merged.baseUrl ?? prior?.baseUrl ?? defaults?.baseUrl,
      reasoning: definition.reasoning ?? prior?.reasoning ?? false,
      input: definition.input ?? prior?.input ?? ['text'],
      cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, ...prior?.cost, ...definition.cost },
      contextWindow: definition.contextWindow ?? prior?.contextWindow ?? 128000,
      maxTokens: definition.maxTokens ?? prior?.maxTokens ?? 16384,
      ...(definition.thinkingLevelMap ?? prior?.thinkingLevelMap ? { thinkingLevelMap: definition.thinkingLevelMap ?? prior?.thinkingLevelMap } : {}),
      ...(definition.samplingParams ?? prior?.samplingParams ? { samplingParams: definition.samplingParams ?? prior?.samplingParams } : {}),
      ...(definition.headers ? { headers: definition.headers } : {}),
      compat: { ...prior?.compat, ...merged.compat, ...definition.compat },
    });
  }
  for (const [id, model] of models) {
    const override = overrides.get(id);
    const providerModel = { ...model, ...(merged.baseUrl && !declarations.get(id)?.baseUrl ? { baseUrl: merged.baseUrl } : {}), compat: { ...model.compat, ...merged.compat } };
    models.set(id, mergeModel(providerModel, override ?? {}) as RegisteredModel);
  }
  const { models: _models, modelOverrides: _overrides, compat: _compat, ...providerFields } = merged;
  return { ...original, ...providerFields, headers: { ...original?.headers, ...merged.headers }, models: [...models.values()] };
}

type Registry = Pick<ModelRegistry, 'getAll' | 'registerProvider' | 'unregisterProvider' | 'getRegisteredProviderConfig' | 'getRegisteredNativeProvider'>;
function isRegistry(value: unknown): value is Registry {
  return record(value) && ['getAll', 'registerProvider', 'unregisterProvider', 'getRegisteredProviderConfig', 'getRegisteredNativeProvider'].every(key => typeof value[key] === 'function');
}

interface OwnedProvider {
  readonly original?: ProviderConfigInput;
  readonly originalNative?: NativeProvider;
  readonly applied?: ProviderConfigInput;
  readonly appliedNative?: NativeProvider;
}

export function createPiModelDiscovery(options: PiModelDiscoveryOptions) {
  let registry: Registry | undefined;
  let current = discoverPiModelSources(options);
  let signature = '';
  const owned = new Map<string, OwnedProvider>();
  const shadowed = new Set<string>();
  let registrationErrors: PiModelDiscoveryError[] = [];

  function release(provider: string, previous: OwnedProvider): void {
    if (!registry) return;
    if (registry.getRegisteredProviderConfig(provider) !== previous.applied || registry.getRegisteredNativeProvider(provider) !== previous.appliedNative) {
      shadowed.add(provider);
      return;
    }
    registry.unregisterProvider(provider);
    if (previous.originalNative) registry.registerProvider(previous.originalNative);
    else if (previous.original) registry.registerProvider(provider, previous.original);
  }

  function dispose(): void {
    for (const [provider, previous] of owned) release(provider, previous);
    owned.clear();
    signature = '';
  }

  function refresh(candidate: unknown, input: { trusted?: boolean } = {}) {
    const discovered = modelSources(options);
    current = { sources: discovered.sources, errors: [...discovered.errors] };
    if (!isRegistry(candidate)) {
      current.errors.push({ path: '', code: 'registry-unavailable', message: 'Pi model registry does not expose the supported registration APIs' });
      current.sources = current.sources.map(source => ({ ...source, status: 'unavailable' }));
      return snapshot();
    }
    if (registry && registry !== candidate) { dispose(); shadowed.clear(); }
    registry = candidate;
    const definitions = discovered.definitions.filter(source => source.scope !== 'workspace' || input.trusted === true);
    const nextSignature = hash([discovered.revision, input.trusted]);
    const ownershipChanged = [...owned].some(([provider, previous]) => registry!.getRegisteredProviderConfig(provider) !== previous.applied || registry!.getRegisteredNativeProvider(provider) !== previous.appliedNative);
    if (signature !== nextSignature || ownershipChanged) {
      dispose();
      registrationErrors = [];
      const baseline = registry.getAll();
      for (const provider of new Set(definitions.map(source => source.provider))) {
        if (shadowed.has(provider)) {
          registrationErrors.push({ path: '', code: 'shadowed', message: `Provider ${provider} is now owned by another Pi extension` });
          continue;
        }
        const original = registry.getRegisteredProviderConfig(provider);
        const originalNative = registry.getRegisteredNativeProvider(provider);
        try {
          const config = providerConfig(provider, definitions, baseline, original);
          if (originalNative) {
            const models = config.models ?? [];
            registry.registerProvider({ ...originalNative, getModels: () => models.map(model => ({ ...model, provider, api: model.api!, baseUrl: model.baseUrl! })) });
          } else registry.registerProvider(provider, config);
          const registeredIds = new Set(registry.getAll().filter(model => model.provider === provider).map(model => model.id));
          if ((config.models ?? []).some(model => !registeredIds.has(model.id))) throw new Error('Provider composition is unavailable');
          owned.set(provider, { original, originalNative, applied: registry.getRegisteredProviderConfig(provider), appliedNative: registry.getRegisteredNativeProvider(provider) });
        } catch {
          // Registration errors can contain configuration values; keep the UI diagnostic structural.
          registrationErrors.push({ path: definitions.find(source => source.provider === provider)?.path ?? '', code: 'provider-registration', message: `Provider ${provider} could not be registered; check its API, base URL, model fields and authentication configuration` });
          registry.unregisterProvider(provider);
          if (originalNative) registry.registerProvider(originalNative);
          else if (original) registry.registerProvider(provider, original);
        }
      }
      signature = nextSignature;
    }
    current.errors.push(...registrationErrors);
    current.sources = current.sources.map(source => ({ ...source, status: source.scope === 'workspace' && input.trusted !== true ? 'untrusted'
      : shadowed.has(source.provider) ? 'shadowed' : source.status === 'shadowed' && input.trusted === true ? 'shadowed' : owned.has(source.provider) ? 'active' : 'unavailable' }));
    return snapshot();
  }

  function snapshot() {
    return { sources: [...current.sources], errors: [...current.errors],
      models: registry?.getAll().map(model => ({ provider: model.provider, id: model.id, name: model.name, api: model.api, contextWindow: model.contextWindow, maxTokens: model.maxTokens })) ?? [],
      nativeModelsPath: capabilitySourcePaths(options.workspace, { homeDir: options.homeDir, env: { ...process.env, ...(options.piAgentDir ? { PI_CODING_AGENT_DIR: options.piAgentDir } : {}) } }).pi.modelsFile,
    };
  }
  return { refresh, snapshot, dispose };
}

export type PiModelDiscovery = ReturnType<typeof createPiModelDiscovery>;
