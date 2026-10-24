// The generated CLI presentation, shared by the tests that read it, with typed
// accessors that narrow its JSON instead of asserting a shape.
import { buildCliPresentation } from '../scripts/tool-contract/presentation.ts';

export type Json = Record<string, unknown>;

function isJson(value: unknown): value is Json {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/** The object at `keys` under `value`; throws when a step is not an object. */
export function at(value: unknown, ...keys: string[]): Json {
  let node = value;
  for (const key of keys) {
    if (!isJson(node)) throw new Error(`no object above ${key}`);
    node = node[key];
  }
  if (!isJson(node)) throw new Error(`no object at ${keys.join('.')}`);
  return node;
}

/** The objects of a JSON array (none when `value` is not an array). */
export function list(value: unknown): Json[] {
  return Array.isArray(value) ? value.filter(isJson) : [];
}

/** The strings of a JSON array (none when `value` is not an array). */
export function strings(value: unknown): string[] {
  return Array.isArray(value)
    ? value.filter((item): item is string => typeof item === 'string')
    : [];
}

export const presentation = buildCliPresentation();
export const tools = list(presentation.tools);
export const tool = (name: string): Json => tools.find(entry => entry.name === name)!;
