// Pieces both flag builders share: kebab-case flag names that avoid the CLI's own
// flags, the sorted, de-duplicated fidelity-note list, and the flags both build alike.
import type { Flag } from './spec.js';

/** A flag before its kind is known. */
export type FlagBase = Omit<Flag, 'kind'>;

const RESERVED_FLAGS = new Set([
  'help',
  'h',
  'version',
  'json',
  'no-input',
  'no-color',
]);

export function notesOf(...groups: string[][]): string[] {
  const notes = [...new Set(groups.flat())];
  notes.sort();
  return notes;
}

function kebab(property: string): string {
  const value = property
    .replace(/([a-z0-9])([A-Z])/g, '$1-$2')
    .replace(/[_\s]+/g, '-')
    .replace(/[^A-Za-z0-9-]/g, '-')
    .replace(/-+/g, '-')
    .replace(/^-|-$/g, '')
    .toLowerCase();
  return value || 'option';
}

export function allocateFlag(property: string, used: Set<string>): string {
  let base = kebab(property);
  if (RESERVED_FLAGS.has(base)) base = `${base}-value`;
  let name = base;
  let index = 2;
  while (used.has(name) || RESERVED_FLAGS.has(name)) {
    name = `${base}-${index}`;
    index += 1;
  }
  used.add(name);
  return name;
}

/** A boolean flag; an optional one defaulting to false is a bare switch. */
export function booleanFlag(base: FlagBase): Flag {
  const presence = !base.required && base.hasDefault && base.defaultValue === false;
  return { ...base, kind: 'boolean', presence };
}

/** The one partial JSON flag of a root that is not an object. */
export function inputFlag(description?: string): Flag {
  return {
    property: 'input',
    name: 'input',
    kind: 'json',
    required: true,
    presence: false,
    partial: true,
    hasDefault: false,
    ...(description ? { description } : {}),
  };
}
