// The installed Chrome's live protocol schema: fetched from its loopback port, cached per browser
// version, and used to describe, search and validate any CDP method without a hand-written list.
import { mkdirSync, readFileSync, renameSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { cdpHttp, packageTmpDir } from './engine-io.mjs';
import {
  classify,
  navigationParamRefusal,
  parseWrappedCall,
  policyRefusal,
  twinEntries,
  wrappedCallRefusal,
  type CdpPolicy,
} from './cdp-policy.mjs';

type TypeSpec = {
  type?: string;
  $ref?: string;
  enum?: string[];
  items?: TypeSpec;
};
type Field = TypeSpec & {
  name: string;
  description?: string;
  optional?: boolean;
  experimental?: boolean;
  deprecated?: boolean;
};
type Command = {
  name: string;
  description?: string;
  parameters?: Field[];
  returns?: Field[];
  experimental?: boolean;
  deprecated?: boolean;
};
type Event = Omit<Command, 'returns'>;
type TypeDef = TypeSpec & {
  id: string;
  description?: string;
  properties?: Field[];
};
export type Domain = {
  domain: string;
  description?: string;
  experimental?: boolean;
  deprecated?: boolean;
  dependencies?: string[];
  types?: TypeDef[];
  commands?: Command[];
  events?: Event[];
};
export type Schema = { version?: unknown; domains: Domain[] };

export type MemberKind = 'method' | 'event' | 'type';
export type Param = {
  name: string;
  type: string;
  optional: boolean;
  enum?: string[];
  description?: string;
};
export type Described = {
  kind: MemberKind;
  name: string;
  domain: string;
  description: string;
  params: Param[];
  returns: Param[];
  experimental: boolean;
  deprecated: boolean;
  policy: CdpPolicy;
  docsUrl: string;
};
export type Validation =
  | {
      ok: true;
      /** Findings that never block: Chrome decides (e.g. a parameter newer than this schema). */
      warnings?: string[];
    }
  | {
      ok: false;
      errors: string[];
      expected?: Described;
      nearest?: string[];
    };

export const DOCS_ROOT = 'https://chromedevtools.github.io/devtools-protocol/';

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

// ---------- loading and caching ----------

/** Cache file stem: the browser version string made safe for a file name. */
export const sanitizeVersion = (version: string) =>
  version.replace(/[^A-Za-z0-9._-]+/g, '_').slice(0, 120) || 'unknown';

export const protocolCacheDir = () => join(packageTmpDir(), 'protocol-cache');

const isSchema = (value: unknown): value is Schema =>
  isRecord(value) && Array.isArray(value.domains);

const memo = new Map<string, Schema>();

export type LoadOptions = {
  cacheDir?: string;
  /** Reads a DevTools HTTP path as JSON; default is the loopback port. */
  fetchJson?: (path: string) => Promise<unknown>;
};

/**
 * The schema of the Chrome on this loopback port. The browser version selects a cache file, so a
 * Chrome update is picked up and an unchanged browser costs one small version request.
 */
export async function loadSchema(
  port: number | string,
  options: LoadOptions = {}
): Promise<Schema> {
  const number = Number(port);
  if (!Number.isInteger(number) || number < 1 || number > 65535)
    throw new Error(`Invalid port ${String(port)}`);
  const fetchJson = options.fetchJson ?? (path => cdpHttp(number, path));
  const version = await fetchJson('/json/version');
  const browser =
    isRecord(version) && typeof version.Browser === 'string'
      ? version.Browser
      : 'unknown';
  const dir = options.cacheDir ?? protocolCacheDir(),
    file = join(dir, `${sanitizeVersion(browser)}.json`),
    key = file;
  const cachedInMemory = memo.get(key);
  if (cachedInMemory) return cachedInMemory;
  try {
    const cached: unknown = JSON.parse(readFileSync(file, 'utf8'));
    if (isSchema(cached)) {
      memo.set(key, cached);
      return cached;
    }
  } catch {
    // no usable cache: fetch below
  }
  const fetched = await fetchJson('/json/protocol');
  if (!isSchema(fetched)) throw new Error('Chrome returned no protocol schema');
  try {
    mkdirSync(dir, { recursive: true, mode: 0o700 });
    const temporary = `${file}.${process.pid}.tmp`;
    writeFileSync(temporary, JSON.stringify(fetched), { mode: 0o600 });
    renameSync(temporary, file);
  } catch {
    // an unwritable cache only costs a refetch
  }
  memo.set(key, fetched);
  return fetched;
}

/** Forgets the in-process copies (tests; a long-lived process after a browser update). */
export const clearSchemaMemo = () => memo.clear();

/** The schema, or why it is unavailable: validation never blocks a call when Chrome cannot answer. */
export async function trySchema(
  port: number | string | undefined,
  options: LoadOptions = {}
): Promise<{ schema?: Schema; note?: string }> {
  if (port === undefined || port === '')
    return { note: 'protocol schema unavailable (no port)' };
  try {
    return { schema: await loadSchema(port, options) };
  } catch (error) {
    return {
      note: `protocol schema unavailable, params not validated: ${error instanceof Error ? error.message : String(error)}`,
    };
  }
}

// ---------- lookup ----------

const find = <T extends { name?: string; id?: string }>(
  list: T[] | undefined,
  name: string
) => list?.find(item => (item.name ?? item.id) === name);

export type Found =
  | { kind: 'domain'; domain: Domain }
  | { kind: 'method'; domain: Domain; entry: Command }
  | { kind: 'event'; domain: Domain; entry: Event }
  | { kind: 'type'; domain: Domain; entry: TypeDef };

/** `Domain` or `Domain.member`; a member is a method, else an event, else a type. */
export function lookupMember(schema: Schema, name: string): Found | undefined {
  const [domainName, member, extra] = name.split('.');
  if (!domainName || extra !== undefined) return undefined;
  const domain = schema.domains.find(d => d.domain === domainName);
  if (!domain) return undefined;
  if (member === undefined) return { kind: 'domain', domain };
  const method = find(domain.commands, member);
  if (method) return { kind: 'method', domain, entry: method };
  const event = find(domain.events, member);
  if (event) return { kind: 'event', domain, entry: event };
  const type = find(domain.types, member);
  return type ? { kind: 'type', domain, entry: type } : undefined;
}

// ---------- description ----------

function typeName(spec: TypeSpec, domain: string): string {
  if (spec.$ref)
    return spec.$ref.includes('.') ? spec.$ref : `${domain}.${spec.$ref}`;
  if (spec.type === 'array')
    return `array<${spec.items ? typeName(spec.items, domain) : 'any'}>`;
  return spec.type ?? 'any';
}

const param = (field: Field, domain: string): Param => ({
  name: field.name,
  type: typeName(field, domain),
  optional: field.optional === true,
  ...(field.enum ? { enum: field.enum } : {}),
  ...(field.description ? { description: field.description } : {}),
});

const anchor = { method: 'method', event: 'event', type: 'type' } as const;
export const docsUrl = (domain: string, kind: MemberKind, name: string) =>
  `${DOCS_ROOT}tot/${domain}/#${anchor[kind]}-${name}`;

/** A compact, self-contained description of one method, event or type. */
export function describeMember(
  schema: Schema,
  name: string
): Described | undefined {
  const found = lookupMember(schema, name);
  if (!found || found.kind === 'domain') return undefined;
  const { domain, entry, kind } = found;
  const flags = entry as { experimental?: boolean; deprecated?: boolean };
  const base = {
    kind,
    name: `${domain.domain}.${'id' in entry ? entry.id : entry.name}`,
    domain: domain.domain,
    description: entry.description ?? '',
    experimental: flags.experimental === true,
    deprecated: flags.deprecated === true,
  };
  const memberName = 'id' in entry ? entry.id : entry.name;
  const params =
    'properties' in entry
      ? (entry.properties ?? [])
      : 'parameters' in entry
        ? (entry.parameters ?? [])
        : [];
  return {
    ...base,
    params: params.map(f => param(f, domain.domain)),
    returns:
      'returns' in entry
        ? (entry.returns ?? []).map(f => param(f, domain.domain))
        : [],
    policy:
      kind === 'method' ? classify(base.name, twinEntries(schema)) : 'free',
    docsUrl: docsUrl(domain.domain, kind, memberName),
  };
}

/** Describes a command by its `Domain.method` name. */
export function describeMethod(
  schema: Schema,
  method: string
): Described | undefined {
  const described = describeMember(schema, method);
  return described?.kind === 'method' ? described : undefined;
}

const firstSentence = (text: string | undefined) => {
  const line = (text ?? '').replace(/\s+/g, ' ').trim();
  const end = line.search(/[.!?](\s|$)/);
  return end < 0 ? line : line.slice(0, end + 1);
};

/** What `<Domain>.enable` is in the installed Chrome: absent, bare (no required params) or needing params. */
export type EnableInfo = {
  available: boolean;
  /** True when `<Domain>.enable` can be sent without params. */
  bare: boolean;
  params: Param[];
  /** The schema's declared dependencies: domains whose types this one uses (not always needed to enable). */
  dependencies: string[];
  /** The events the domain starts emitting once enabled. */
  events: string[];
};

export function enableInfo(domain: Domain): EnableInfo {
  const command = (domain.commands ?? []).find(c => c.name === 'enable');
  const params = (command?.parameters ?? []).map(f => param(f, domain.domain));
  return {
    available: command !== undefined,
    bare: command !== undefined && params.every(p => p.optional),
    params,
    dependencies: domain.dependencies ?? [],
    events: (domain.events ?? []).map(e => e.name),
  };
}

export const docsHash = (domain: string) => `${DOCS_ROOT}#/${domain}`;

/** A short recipe for a domain: what to call in order (`Domain.method {key params}`) and what to watch for. */
export type Playbook = { summary: string; steps: string[]; notes?: string };

let playbooks: Record<string, Playbook> | undefined;
/** The playbook of a domain from cdp-playbooks.json (shipped beside this file); undefined when it has none. */
export function playbookOf(domain: string): Playbook | undefined {
  playbooks ??= JSON.parse(
    readFileSync(new URL('./cdp-playbooks.json', import.meta.url), 'utf8')
  ) as Record<string, Playbook>;
  return Object.hasOwn(playbooks, domain) ? playbooks[domain] : undefined;
}

/** A domain's methods and events as one-line entries with flags and policy. */
export function describeDomain(domain: Domain, schema?: Schema) {
  const twins = schema && twinEntries(schema);
  const line = (item: Command | Event, kind: MemberKind) => ({
    name: item.name,
    summary: firstSentence(item.description),
    flags: [
      ...(item.experimental ? ['experimental'] : []),
      ...(item.deprecated ? ['deprecated'] : []),
    ],
    ...(kind === 'method'
      ? { policy: classify(`${domain.domain}.${item.name}`, twins) }
      : {}),
  });
  return {
    kind: 'domain' as const,
    domain: domain.domain,
    description: firstSentence(domain.description),
    experimental: domain.experimental === true,
    deprecated: domain.deprecated === true,
    dependencies: domain.dependencies ?? [],
    methods: (domain.commands ?? []).map(c => line(c, 'method')),
    events: (domain.events ?? []).map(e => line(e, 'event')),
    types: (domain.types ?? []).map(t => t.id),
    enable: enableInfo(domain),
    startsEmitting: (domain.events ?? []).map(e => e.name),
    ...(playbookOf(domain.domain)
      ? { playbook: playbookOf(domain.domain) }
      : {}),
    docsUrl: `${DOCS_ROOT}tot/${domain.domain}/`,
    docsHash: docsHash(domain.domain),
  };
}

export type DomainRow = {
  name: string;
  commands: number;
  events: number;
  needsEnable: boolean;
  dependencies: string[];
  experimental: boolean;
};
export type IndexPage = {
  total: number;
  offset: number;
  domains: DomainRow[];
  nextOffset?: number;
};

/** One compact row per domain of the installed Chrome, paged like search (nothing is silently capped). */
export function domainIndex(
  schema: Schema,
  { offset = 0, limit = SEARCH_PAGE } = {}
): IndexPage {
  const rows = schema.domains.map(d => ({
    name: d.domain,
    commands: (d.commands ?? []).length,
    events: (d.events ?? []).length,
    needsEnable: (d.commands ?? []).some(c => c.name === 'enable'),
    dependencies: d.dependencies ?? [],
    experimental: d.experimental === true,
  }));
  const end = offset + limit;
  return {
    total: rows.length,
    offset,
    domains: rows.slice(offset, end),
    ...(end < rows.length ? { nextOffset: end } : {}),
  };
}

// ---------- search ----------

export type Hit = {
  name: string;
  kind: MemberKind | 'domain';
  summary: string;
  score: number;
  policy?: CdpPolicy;
};
export type SearchPage = {
  total: number;
  offset: number;
  hits: Hit[];
  /** Offset of the next page when more hits remain. */
  nextOffset?: number;
};
export const SEARCH_PAGE = 25;

/** Ranked case-insensitive search over names, parameter names and descriptions; every token must match. */
export function searchSchema(
  schema: Schema,
  keyword: string,
  { offset = 0, limit = SEARCH_PAGE } = {}
): SearchPage {
  const tokens = keyword.toLowerCase().split(/\s+/).filter(Boolean);
  const hits: Hit[] = [];
  const score = (
    name: string,
    description: string | undefined,
    params: Field[] | undefined
  ) => {
    const lowerName = name.toLowerCase(),
      member = lowerName.split('.').pop() ?? lowerName,
      text = (description ?? '').toLowerCase(),
      names = (params ?? []).map(p => p.name.toLowerCase());
    let total = 0;
    for (const token of tokens) {
      let best = 0;
      if (member === token) best = 100;
      else if (member.startsWith(token)) best = 70;
      else if (lowerName.includes(token)) best = 60;
      else if (names.some(n => n.includes(token))) best = 25;
      else if (text.includes(token)) best = 15;
      if (!best) return 0;
      total += best;
    }
    return total;
  };
  const push = (
    name: string,
    kind: Hit['kind'],
    entry: { description?: string },
    params: Field[] | undefined
  ) => {
    const value = tokens.length ? score(name, entry.description, params) : 0;
    if (value)
      hits.push({
        name,
        kind,
        summary: firstSentence(entry.description),
        score: value,
        ...(kind === 'method'
          ? { policy: classify(name, twinEntries(schema)) }
          : {}),
      });
  };
  for (const domain of schema.domains) {
    push(domain.domain, 'domain', domain, undefined);
    for (const c of domain.commands ?? [])
      push(`${domain.domain}.${c.name}`, 'method', c, c.parameters);
    for (const e of domain.events ?? [])
      push(`${domain.domain}.${e.name}`, 'event', e, e.parameters);
    for (const t of domain.types ?? [])
      push(`${domain.domain}.${t.id}`, 'type', t, t.properties);
  }
  hits.sort((a, b) => b.score - a.score || a.name.localeCompare(b.name));
  const end = offset + limit;
  return {
    total: hits.length,
    offset,
    hits: hits.slice(offset, end),
    ...(end < hits.length ? { nextOffset: end } : {}),
  };
}

// ---------- typo suggestions ----------

function editDistance(a: string, b: string) {
  let previous = Array.from({ length: b.length + 1 }, (_, i) => i);
  for (let i = 1; i <= a.length; i++) {
    const row = [i];
    for (let j = 1; j <= b.length; j++)
      row[j] = Math.min(
        previous[j] + 1,
        row[j - 1] + 1,
        previous[j - 1] + (a[i - 1] === b[j - 1] ? 0 : 1)
      );
    previous = row;
  }
  return previous[b.length];
}

/** Up to three method names closest to a mistyped one. */
export function nearestMethods(schema: Schema, method: string): string[] {
  const wanted = method.toLowerCase(),
    limit = Math.max(3, Math.ceil(wanted.length * 0.4));
  const scored: [string, number][] = [];
  for (const domain of schema.domains)
    for (const command of domain.commands ?? []) {
      const name = `${domain.domain}.${command.name}`;
      scored.push([name, editDistance(wanted, name.toLowerCase())]);
    }
  return scored
    .filter(([, distance]) => distance <= limit)
    .sort((a, b) => a[1] - b[1] || a[0].localeCompare(b[0]))
    .slice(0, 3)
    .map(([name]) => name);
}

/** Up to three domain names closest to a mistyped one (case-insensitive, prefixes count). */
export function nearestDomains(schema: Schema, name: string): string[] {
  const wanted = name.toLowerCase();
  const scored = schema.domains.map(
    ({ domain }) =>
      [
        domain,
        domain.toLowerCase().startsWith(wanted) ||
        wanted.startsWith(domain.toLowerCase())
          ? 0
          : editDistance(wanted, domain.toLowerCase()),
      ] as [string, number]
  );
  const limit = Math.max(3, Math.ceil(wanted.length * 0.4));
  return scored
    .filter(([, distance]) => distance <= limit)
    .sort((a, b) => a[1] - b[1] || a[0].localeCompare(b[0]))
    .slice(0, 3)
    .map(([domain]) => domain);
}

// ---------- validation ----------

/** A `$step` / `$event` reference resolves at run time, so its type cannot be checked up front. */
const isReference = (value: unknown) =>
  isRecord(value) && ('$step' in value || '$event' in value);

function resolveType(schema: Schema, domain: string, ref: string) {
  const [first, second] = ref.split('.');
  const owner = second === undefined ? domain : first,
    id = second ?? first;
  const def = schema.domains
    .find(d => d.domain === owner)
    ?.types?.find(t => t.id === id);
  return def ? { def, domain: owner } : undefined;
}

const describeValue = (value: unknown) =>
  value === null ? 'null' : Array.isArray(value) ? 'array' : typeof value;

function checkEnum(
  spec: TypeSpec,
  value: unknown,
  path: string,
  errors: string[]
) {
  if (spec.enum && !spec.enum.includes(value as string))
    errors.push(
      `${path}: ${JSON.stringify(value)} is not one of ${spec.enum.join(', ')}`
    );
}

function checkValue(
  schema: Schema,
  domain: string,
  spec: TypeSpec,
  value: unknown,
  path: string,
  errors: string[]
) {
  if (isReference(value)) return;
  if (spec.$ref) {
    const resolved = resolveType(schema, domain, spec.$ref);
    if (resolved)
      checkValue(schema, resolved.domain, resolved.def, value, path, errors);
    return;
  }
  switch (spec.type) {
    case undefined:
    case 'any':
      return;
    case 'integer':
      if (!Number.isInteger(value))
        errors.push(`${path}: expected integer, got ${describeValue(value)}`);
      return;
    case 'number':
      if (typeof value !== 'number' || !Number.isFinite(value))
        errors.push(`${path}: expected number, got ${describeValue(value)}`);
      return;
    case 'string':
      if (typeof value !== 'string')
        errors.push(`${path}: expected string, got ${describeValue(value)}`);
      else checkEnum(spec, value, path, errors);
      return;
    case 'boolean':
      if (typeof value !== 'boolean')
        errors.push(`${path}: expected boolean, got ${describeValue(value)}`);
      return;
    case 'array':
      if (!Array.isArray(value))
        errors.push(`${path}: expected array, got ${describeValue(value)}`);
      else if (spec.items)
        for (const [index, item] of value.entries())
          checkValue(
            schema,
            domain,
            spec.items,
            item,
            `${path}[${index}]`,
            errors
          );
      return;
    case 'object': {
      if (!isRecord(value)) {
        errors.push(`${path}: expected object, got ${describeValue(value)}`);
        return;
      }
      const properties = (spec as TypeDef).properties;
      if (!properties) return;
      for (const property of properties) {
        const at = `${path}.${property.name}`;
        if (value[property.name] === undefined) {
          if (!property.optional) errors.push(`${at}: required`);
        } else
          checkValue(
            schema,
            domain,
            property,
            value[property.name],
            at,
            errors
          );
      }
    }
  }
}

/** Checks `params` against the method's schema; invalid input comes back with the expected shape. */
export function validateParams(
  schema: Schema,
  method: string,
  params: unknown
): Validation {
  const expected = describeMethod(schema, method);
  if (!expected)
    return {
      ok: false,
      errors: [`Unknown method ${method} in the installed Chrome`],
      nearest: nearestMethods(schema, method),
    };
  if (!isRecord(params))
    return { ok: false, errors: ['params: expected object'], expected };
  const command = (
    lookupMember(schema, method) as Extract<Found, { kind: 'method' }>
  ).entry;
  const fields = command.parameters ?? [],
    errors: string[] = [],
    warnings: string[] = [];
  const domain = expected.domain;
  // A newer Chrome adds parameters and Chrome rejects a real unknown itself, so this never blocks.
  for (const key of Object.keys(params))
    if (!fields.some(f => f.name === key))
      warnings.push(
        `params.${key}: not a parameter of ${method} in the installed Chrome's schema; sent as is`
      );
  for (const field of fields) {
    const value = params[field.name],
      at = `params.${field.name}`;
    if (value === undefined) {
      if (!field.optional) errors.push(`${at}: required`);
    } else checkValue(schema, domain, field, value, at, errors);
  }
  if (errors.length) return { ok: false, errors, expected };
  return warnings.length ? { ok: true, warnings } : { ok: true };
}

/** The refusal text of invalid params, with the expected shape, docs link and typo suggestions. */
export function validationRefusal(
  method: string,
  result: Validation,
  wrappedIn?: string
) {
  if (result.ok) return undefined;
  const lines = [
    `[INVALID] ${method}${wrappedIn ? ` (wrapped in ${wrappedIn})` : ''}: ${result.errors.join('; ')}`,
  ];
  if (result.nearest?.length)
    lines.push(`Did you mean: ${result.nearest.join(', ')}`);
  if (result.expected) {
    const spec = result.expected.params
      .map(p => `${p.name}${p.optional ? '?' : ''}: ${p.type}`)
      .join(', ');
    lines.push(
      `Expected params: {${spec}}`,
      `Docs: ${result.expected.docsUrl}`
    );
  }
  return lines.join('\n');
}

/** A [WARNING] line when the method is experimental or deprecated; nothing otherwise. */
export function flagWarning(schema: Schema, method: string) {
  const described = describeMethod(schema, method);
  if (!described) return undefined;
  const flags = [
    ...(described.experimental ? ['experimental'] : []),
    ...(described.deprecated ? ['deprecated'] : []),
  ];
  return flags.length
    ? `[WARNING] ${method} is ${flags.join(' and ')} in the installed Chrome; it may change or fail`
    : undefined;
}

/**
 * The runtime gate of one raw call: policy first, then the live schema. A refusal is an Error message
 * to throw; warnings (experimental, deprecated, unknown parameter, schema unavailable) are printed and
 * never block. A `Target.sendMessageToTarget` is judged by the call it wraps, exactly as a direct call.
 */
export async function gateCdpCall(
  call: { method: string; params: unknown; acknowledge?: boolean },
  port: number | string | undefined,
  warn: (line: string) => void,
  options: LoadOptions = {}
): Promise<string | undefined> {
  const { schema, note } = await trySchema(port, options);
  // A deprecated twin inherits the policy of the method that replaced it, so the live schema is read first.
  const twins = schema ? twinEntries(schema) : undefined;
  const policy =
    policyRefusal(call.method, call.acknowledge, twins) ??
    navigationParamRefusal(call.method, call.params, call.acknowledge) ??
    wrappedCallRefusal(call.method, call.params, call.acknowledge, twins);
  if (policy) return policy;
  if (!schema) {
    if (note) warn(`[NOTE] ${note}`);
    return undefined;
  }
  const checked = (method: string, params: unknown, wrapped?: string) => {
    const result = validateParams(schema, method, params);
    if (!result.ok) return validationRefusal(method, result, wrapped);
    for (const line of result.warnings ?? []) warn(`[WARNING] ${line}`);
    const warning = flagWarning(schema, method);
    if (warning) warn(warning);
    return undefined;
  };
  const refusal = checked(call.method, call.params);
  if (refusal || call.method !== 'Target.sendMessageToTarget') return refusal;
  let inner = parseWrappedCall(call.params);
  // The policy gate above already refused a malformed or unacceptable wrapped call.
  while (inner.ok) {
    const problem = checked(inner.method, inner.params, call.method);
    if (problem || inner.method !== 'Target.sendMessageToTarget')
      return problem;
    inner = parseWrappedCall(inner.params);
  }
  return undefined;
}
