// Flow logic without a browser: definitions, parameter checks, placeholder expansion, the
// workspace store and secret scrubbing. Every expanded plan goes through the real plan validator.
import {
  chmodSync,
  existsSync,
  linkSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  unlinkSync,
  writeFileSync,
} from 'node:fs';
import { join, relative, resolve, sep } from 'node:path';
import { validatePlan } from '../cdp-checks/plan-validate.mjs';
import { errorMessage } from '../error-message.mjs';
import { builtinFlows } from './index.mjs';
import type {
  FlowDefinition,
  ParamDefinition,
  ParamType,
  ParamValue,
} from './flow-types.mjs';

type Json = Record<string, unknown>;
const NAME = /^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$/;
const PARAM = /^[A-Za-z][A-Za-z0-9_]*$/;
const PLACEHOLDER = /\{\{([A-Za-z][A-Za-z0-9_]*)\}\}/g;
const TYPES: ParamType[] = ['string', 'number', 'boolean'];
const SECRET_NAME = /pass(word)?|secret|token|api_?key|credential/i;
export const REDACTED = '[REDACTED]';

const isRecord = (value: unknown): value is Json =>
  Boolean(value) && typeof value === 'object' && !Array.isArray(value);

export const flowsDir = (workspace: string) =>
  join(workspace, '.octocode', 'octocode-chrome-devtools', 'flows');

/** A flow name that is safe as a file name: the same pattern the schema enforces. */
export function assertFlowName(name: unknown): asserts name is string {
  if (typeof name !== 'string' || !NAME.test(name))
    throw new Error(
      'flow name: letters, digits, . _ -; at most 64 characters, starting with a letter or digit'
    );
}

/** Placeholder names inside string values; a placeholder in an object key is refused. */
export function placeholders(node: unknown, found = new Set<string>()) {
  if (typeof node === 'string')
    for (const match of node.matchAll(PLACEHOLDER)) found.add(match[1]);
  else if (Array.isArray(node))
    for (const item of node) placeholders(item, found);
  else if (isRecord(node))
    for (const [key, value] of Object.entries(node)) {
      if (key.includes('{{'))
        throw new Error(
          `Placeholders are allowed only inside string values, not in the key "${key}"`
        );
      placeholders(value, found);
    }
  return found;
}

export function validateDefinition(
  def: unknown,
  label = 'flow'
): asserts def is FlowDefinition {
  if (!isRecord(def)) throw new Error(`${label} must be an object`);
  assertFlowName(def.name);
  if (typeof def.description !== 'string' || !def.description)
    throw new Error(`${label} needs a description`);
  if (!isRecord(def.params)) throw new Error(`${label} needs params`);
  for (const [key, raw] of Object.entries(def.params)) {
    if (!PARAM.test(key)) throw new Error(`Invalid param name ${key}`);
    if (!isRecord(raw) || !TYPES.includes(raw.type as ParamType))
      throw new Error(`Param ${key} needs a type: ${TYPES.join(', ')}`);
    if (typeof raw.description !== 'string')
      throw new Error(`Param ${key} needs a description`);
    if (raw.default !== undefined && typeof raw.default !== raw.type)
      throw new Error(`Param ${key} default must be a ${String(raw.type)}`);
    if (raw.secret && raw.default !== undefined)
      throw new Error(`Secret param ${key} cannot have a default`);
    if (raw.secret && raw.type !== 'string')
      throw new Error(`Secret param ${key} must be a string`);
    if (raw.required && raw.default !== undefined)
      throw new Error(`Param ${key} cannot be required and have a default`);
  }
  if (!isRecord(def.plan)) throw new Error(`${label} needs a plan object`);
  if (
    !Array.isArray(def.expects) ||
    !def.expects.every(text => typeof text === 'string')
  )
    throw new Error(`${label} needs expects: a list of texts`);
  const names = placeholders(def.plan);
  const params = def.params as Record<string, ParamDefinition>;
  if (def.repeat !== undefined) {
    const repeat = def.repeat;
    if (
      !isRecord(repeat) ||
      !Array.isArray(repeat.steps) ||
      typeof repeat.param !== 'string' ||
      params[repeat.param]?.type !== 'number' ||
      (repeat.offset !== undefined && !Number.isSafeInteger(repeat.offset))
    )
      throw new Error(
        'repeat needs steps, a number param and an integer offset'
      );
    placeholders(repeat.steps, names);
  }
  for (const name of names)
    if (!Object.hasOwn(params, name))
      throw new Error(`Placeholder {{${name}}} has no param definition`);
  // A stale definition fails here: its sample expansion must pass the real validator.
  const typed = def as FlowDefinition;
  validatePlan(expandFlow(typed, sampleParams(typed)).plan);
}

/** One valid value per param (default, else a type-shaped sample), so every step is expanded. */
export function sampleParams(def: FlowDefinition) {
  const values: Record<string, ParamValue> = {};
  for (const [name, param] of Object.entries(def.params))
    values[name] =
      param.default ??
      (param.type === 'number'
        ? 2
        : param.type === 'boolean'
          ? true
          : /url/i.test(name)
            ? 'https://example.com/'
            : 'sample');
  return values;
}

/** Given params checked against the definition: unknown, mistyped and (unless allowed) missing are refused. */
export function resolveParams(
  def: FlowDefinition,
  given: unknown,
  allowMissing = false
) {
  const input = given === undefined ? {} : given;
  if (!isRecord(input)) throw new Error('params must be an object');
  for (const key of Object.keys(input))
    if (!Object.hasOwn(def.params, key))
      throw new Error(
        `Unknown param ${key} for flow ${def.name}; known: ${Object.keys(def.params).join(', ') || 'none'}`
      );
  const values: Record<string, ParamValue> = {},
    missing: string[] = [];
  for (const [name, param] of Object.entries(def.params)) {
    const value = input[name];
    if (value !== undefined) {
      if (typeof value !== param.type)
        throw new Error(`Param ${name} must be a ${param.type}`);
      values[name] = value as ParamValue;
    } else if (param.default !== undefined) values[name] = param.default;
    else if (param.required) missing.push(name);
  }
  if (missing.length && !allowMissing)
    throw new Error(`Missing required param(s): ${missing.join(', ')}`);
  return { values, missing };
}

const substitute = (
  node: unknown,
  values: Record<string, ParamValue>,
  redact: Set<string>
): unknown => {
  if (typeof node === 'string')
    return node.replace(PLACEHOLDER, (whole, name: string) =>
      redact.has(name)
        ? `[secret:${name}]`
        : Object.hasOwn(values, name)
          ? String(values[name])
          : whole
    );
  if (Array.isArray(node))
    return node.map(item => substitute(item, values, redact));
  if (isRecord(node))
    return Object.fromEntries(
      Object.entries(node).map(([key, value]) => [
        key,
        substitute(value, values, redact),
      ])
    );
  return node;
};

/** A step that names only absent optional params is dropped; a half-present step is an error. */
function usable(
  def: FlowDefinition,
  steps: unknown[],
  values: Record<string, ParamValue>,
  missing: string[]
) {
  const kept: unknown[] = [];
  steps.forEach((step, index) => {
    const names = [...placeholders(step)];
    const absent = names.filter(
      name =>
        !Object.hasOwn(values, name) &&
        !def.params[name].required &&
        !missing.includes(name)
    );
    if (names.length && absent.length === names.length) return;
    if (absent.length)
      throw new Error(
        `Step ${index + 1} needs ${absent.join(', ')} together with ${names.filter(name => !absent.includes(name)).join(', ')}`
      );
    kept.push(step);
  });
  return kept;
}

/**
 * The plan a flow runs for these params. `redact` shows secrets as [secret:name]; `missing`
 * required params stay as literal placeholders (for `show`).
 */
export function expandFlow(
  def: FlowDefinition,
  values: Record<string, ParamValue>,
  { redact = false, missing = [] as string[] } = {}
) {
  const secrets = new Set(
    redact
      ? Object.entries(def.params)
          .filter(([, param]) => param.secret)
          .map(([name]) => name)
      : []
  );
  const base = def.plan as { steps: unknown[] } & Json;
  const steps = usable(def, base.steps ?? [], values, missing);
  if (def.repeat) {
    const count =
      Number(values[def.repeat.param] ?? 0) + (def.repeat.offset ?? 0);
    if (!Number.isSafeInteger(count) || count < 0)
      throw new Error(
        `Param ${def.repeat.param} must give a whole number of repeats of at least 0`
      );
    const each = usable(def, def.repeat.steps, values, missing);
    for (let i = 0; i < count; i++) steps.push(...each);
  }
  const plan = substitute({ ...base, steps }, values, secrets) as Json;
  return { plan };
}

/** Text with every secret value (raw, JSON-escaped, URL-encoded) replaced. */
export function scrubText(text: string, secrets: string[]) {
  let out = text;
  for (const secret of secrets.filter(Boolean))
    for (const form of new Set([
      secret,
      JSON.stringify(secret).slice(1, -1),
      encodeURIComponent(secret),
    ]))
      out = out.split(form).join(REDACTED);
  return out;
}
export const secretValues = (
  def: FlowDefinition,
  values: Record<string, ParamValue>
) =>
  Object.entries(def.params)
    .filter(([, param]) => param.secret)
    .flatMap(([name]) =>
      values[name] === undefined ? [] : [String(values[name])]
    );

/** Validate the expanded plan; an error message never carries a secret. */
export function validateExpanded(plan: unknown, secrets: string[]) {
  try {
    return validatePlan(plan);
  } catch (error) {
    throw new Error(scrubText(errorMessage(error), secrets), { cause: error });
  }
}

/** The definition with secret defaults impossible and values never present: safe to print. */
export const describe = (
  def: FlowDefinition,
  source: 'built-in' | 'workspace'
) => ({
  name: def.name,
  description: def.description,
  source,
  params: def.params,
  expects: def.expects,
});

export type LoadedFlow = {
  def: FlowDefinition;
  source: 'built-in' | 'workspace';
};
export function loadFlows(workspace: string) {
  const flows: LoadedFlow[] = builtinFlows.map(def => ({
    def,
    source: 'built-in' as const,
  }));
  const invalid: { file: string; reason: string }[] = [];
  const dir = flowsDir(workspace);
  const files = existsSync(dir)
    ? readdirSync(dir)
        .filter(file => file.endsWith('.json'))
        .sort()
    : [];
  for (const file of files) {
    const name = file.slice(0, -'.json'.length);
    try {
      if (builtinFlows.some(def => def.name === name))
        throw new Error('name collides with a built-in flow');
      const def: unknown = JSON.parse(readFileSync(join(dir, file), 'utf8'));
      validateDefinition(def, file);
      if (def.name !== name) throw new Error('name does not match the file');
      flows.push({ def, source: 'workspace' });
    } catch (error) {
      invalid.push({ file, reason: errorMessage(error) });
    }
  }
  return { flows, invalid };
}

export function getFlow(workspace: string, name: string): LoadedFlow {
  assertFlowName(name);
  const builtin = builtinFlows.find(def => def.name === name);
  if (builtin) return { def: builtin, source: 'built-in' };
  const file = join(flowsDir(workspace), `${name}.json`);
  if (!existsSync(file))
    throw new Error(
      `Unknown flow ${name}; use flow list to see built-in and saved flows`
    );
  const def: unknown = JSON.parse(readFileSync(file, 'utf8'));
  validateDefinition(def, `${name}.json`);
  if (def.name !== name) throw new Error(`${name}.json names another flow`);
  return { def, source: 'workspace' };
}

/** A flow made from a plan: each {{placeholder}} becomes a required string param. */
export function flowFromPlan(name: string, plan: unknown): FlowDefinition {
  if (!isRecord(plan)) throw new Error('Plan must be an object');
  const names = [...placeholders(plan)].sort();
  const params = Object.fromEntries(
    names.map(param => [
      param,
      {
        type: 'string' as const,
        required: true,
        ...(SECRET_NAME.test(param) ? { secret: true } : {}),
        description: `Value for {{${param}}}`,
      },
    ])
  );
  const def: FlowDefinition = {
    name,
    description: `Saved plan ${name}`,
    params,
    plan,
    expects: [],
  };
  validateDefinition(def);
  return def;
}

/** Writes <workspace>/.octocode/octocode-chrome-devtools/flows/<name>.json (0600) without ever overwriting. */
export function saveFlow(workspace: string, name: string, plan: unknown) {
  assertFlowName(name);
  if (builtinFlows.some(def => def.name === name))
    throw new Error(`${name} is a built-in flow; choose another name`);
  const def = flowFromPlan(name, plan);
  const dir = flowsDir(workspace);
  const target = resolve(dir, `${name}.json`);
  if (
    relative(resolve(dir), target).startsWith('..') ||
    !target.startsWith(resolve(dir) + sep)
  )
    throw new Error('flow name escapes the flows folder');
  mkdirSync(dir, { recursive: true });
  const temp = join(dir, `.${name}.${process.pid}.tmp`);
  writeFileSync(temp, `${JSON.stringify(def, null, 2)}\n`, {
    mode: 0o600,
    flag: 'wx',
  });
  try {
    chmodSync(temp, 0o600);
    linkSync(temp, target);
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'EEXIST')
      throw new Error(
        `Flow ${name} already exists in ${relative(workspace, target)}; delete that file manually to replace it`,
        { cause: error }
      );
    throw error;
  } finally {
    unlinkSync(temp);
  }
  return { def, file: relative(workspace, target) };
}
