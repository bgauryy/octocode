// Flags from a Zod schema: one flag per property of an object root, typed by the
// property's unwrapped Zod type. A type no flag can hold stays one partial JSON flag
// and leaves a fidelity note.
import type { ZodType } from 'zod';
import { allocateFlag, notesOf } from './flag-names.js';
import type { Flag, ItemKind } from './spec.js';

interface ZodNode {
  description?: string;
  def: {
    type: string;
    innerType?: ZodNode;
    defaultValue?: unknown;
    checks?: readonly ZodCheck[];
    entries?: Record<string, unknown>;
    element?: ZodNode;
    shape?: Record<string, ZodNode>;
    options?: readonly ZodNode[];
    values?: readonly unknown[];
  };
}

interface ZodCheck {
  def?: { check?: string; format?: string };
  _zod?: { def?: { check?: string; format?: string } };
}

interface Unwrapped {
  node: ZodNode;
  required: boolean;
  hasDefault: boolean;
  defaultValue?: unknown;
  description?: string;
  notes: string[];
}

type FlagBase = Omit<Flag, 'kind'>;

/** Wrappers that change only optionality or the default; the flag reads the inner type. */
const WRAPPERS = new Set(['optional', 'default', 'prefault', 'readonly', 'catch']);

function isZodNode(value: unknown): value is ZodNode {
  return typeof value === 'object' && value !== null && 'def' in value;
}

function asNode(schema: ZodType): ZodNode {
  if (!isZodNode(schema)) throw new Error('Not a Zod schema');
  return schema;
}

function unwrapZod(schema: ZodNode): Unwrapped {
  let current = schema;
  let required = true;
  let hasDefault = false;
  let defaultValue: unknown;
  let description: string | undefined;
  const notes: string[] = [];
  const seen = new Set<ZodNode>();
  while (!seen.has(current)) {
    seen.add(current);
    if (description === undefined && current.description) description = current.description;
    const type = current.def.type;
    if (WRAPPERS.has(type)) {
      if (type === 'optional') required = false;
      if (type === 'default' || type === 'prefault') {
        required = false;
        hasDefault = true;
        const raw = current.def.defaultValue;
        defaultValue = typeof raw === 'function' ? raw() : raw;
      }
      const inner = current.def.innerType;
      if (!inner) break;
      current = inner;
      continue;
    }
    if (type === 'nullable') {
      notes.push('nullable');
      const inner = current.def.innerType;
      if (!inner) break;
      current = inner;
      continue;
    }
    break;
  }
  return { node: current, required, hasDefault, defaultValue, description, notes };
}

function checkInfo(check: ZodCheck): { check?: string; format?: string } | undefined {
  // Zod keeps a check's definition on `def` or on its internal `_zod` record.
  return check.def ?? check['_zod']?.def;
}

function isIntegerNode(node: ZodNode): boolean {
  return (node.def.checks ?? []).some(check => {
    const info = checkInfo(check);
    return info?.format === 'safeint' || info?.check === 'int';
  });
}

function literalValue(node: ZodNode): unknown {
  return node.def.values?.[0];
}

function enumFromUnion(node: ZodNode): unknown[] | undefined {
  const options = node.def.options;
  if (!options || options.length === 0) return undefined;
  const values: unknown[] = [];
  for (const option of options) {
    const unwrapped = unwrapZod(option);
    if (unwrapped.node.def.type !== 'literal') return undefined;
    values.push(literalValue(unwrapped.node));
  }
  return values;
}

function primitiveItem(node: ZodNode): ItemKind | undefined {
  const unwrapped = unwrapZod(node);
  if (unwrapped.notes.includes('nullable')) return undefined;
  const type = unwrapped.node.def.type;
  if (type === 'string') return 'string';
  if (type === 'boolean') return 'boolean';
  if (type === 'number') return isIntegerNode(unwrapped.node) ? 'integer' : 'number';
  return undefined;
}

/** The flag of each Zod type a flag can hold; `notes` collects what the flag loses. */
const ZOD_FLAGS = new Map<string, (base: FlagBase, node: ZodNode, notes: string[]) => Flag>([
  ['string', (base, node, notes) => {
    const infos = (node.def.checks ?? []).map(check => checkInfo(check));
    if (infos.some(info => info?.check === 'custom')) notes.push('refine');
    else if (infos.some(info => info !== undefined)) notes.push('constraint');
    return { ...base, kind: 'string' };
  }],
  ['number', (base, node) => ({ ...base, kind: isIntegerNode(node) ? 'integer' : 'number' })],
  ['boolean', base => {
    const presence = !base.required && base.hasDefault && base.defaultValue === false;
    return { ...base, kind: 'boolean', presence };
  }],
  ['enum', (base, node) => ({ ...base, kind: 'enum', enumValues: Object.values(node.def.entries ?? {}) })],
  ['literal', (base, node) => ({ ...base, kind: 'enum', enumValues: [literalValue(node)] })],
  ['union', (base, node, notes) => {
    const values = enumFromUnion(node);
    if (values) return { ...base, kind: 'enum', enumValues: values };
    notes.push('union');
    return { ...base, kind: 'json', partial: true };
  }],
  ['array', (base, node, notes) => {
    const item = node.def.element ? primitiveItem(node.def.element) : undefined;
    if (item) return { ...base, kind: 'array', itemKind: item };
    notes.push('array');
    return { ...base, kind: 'json', partial: true };
  }],
]);

function flagFromZod(property: string, schema: ZodNode, name: string): { flag: Flag; notes: string[] } {
  const unwrapped = unwrapZod(schema);
  const node = unwrapped.node;
  const type = node.def.type;
  const notes = [...unwrapped.notes];
  const base = {
    property,
    name,
    required: unwrapped.required,
    presence: false,
    partial: false,
    hasDefault: unwrapped.hasDefault,
    ...(unwrapped.description ? { description: unwrapped.description } : {}),
    ...(unwrapped.hasDefault ? { defaultValue: unwrapped.defaultValue } : {}),
  };
  if (notes.includes('nullable')) {
    return { flag: { ...base, kind: 'json', partial: true }, notes };
  }
  const build = ZOD_FLAGS.get(type);
  if (build) return { flag: build(base, node, notes), notes };
  notes.push(type || 'unknown');
  return { flag: { ...base, kind: 'json', partial: true }, notes };
}

export function flagsFromZod(schema: ZodType): { flags: Flag[]; notes: string[] } {
  const node = asNode(schema);
  const unwrapped = unwrapZod(node);
  if (unwrapped.node.def.type !== 'object' || !unwrapped.node.def.shape) {
    return {
      flags: [
        {
          property: 'input',
          name: 'input',
          kind: 'json',
          required: true,
          presence: false,
          partial: true,
          hasDefault: false,
          description: unwrapped.description,
        },
      ],
      notes: notesOf(['partial'], unwrapped.notes),
    };
  }
  const used = new Set<string>();
  const flags: Flag[] = [];
  const notes: string[] = [...unwrapped.notes];
  for (const [property, child] of Object.entries(unwrapped.node.def.shape)) {
    const built = flagFromZod(property, child, allocateFlag(property, used));
    flags.push(built.flag);
    notes.push(...built.notes);
  }
  if (flags.some(flag => flag.partial)) notes.push('partial');
  return { flags, notes: notesOf(notes) };
}

/** Whether a Zod root takes flags by property (an object) or one `input` value. */
export function zodRootIsObject(schema: ZodType): boolean {
  const unwrapped = unwrapZod(asNode(schema));
  return unwrapped.node.def.type === 'object' && Boolean(unwrapped.node.def.shape);
}
