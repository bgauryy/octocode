// Params generated from a live protocol schema: every required param with a typed value, enums take
// their first value, nested required properties recurse and `$ref` is resolved.
import { CDP_DOMAIN_HAZARDS } from '../../dist/engine/cdp-checks/cdp-domain-hazards.mjs';

const MAX_DEPTH = 6;

const resolveRef = (schema, domain, ref) => {
  const [first, second] = ref.split('.');
  const owner = second === undefined ? domain : first;
  const id = second ?? first;
  const def = schema.domains
    .find(d => d.domain === owner)
    ?.types?.find(t => t.id === id);
  return { def, domain: owner };
};

/** One value for a type spec (a parameter, array item, property or type definition). */
export function valueFor(schema, domain, spec, depth = 0) {
  if (spec.$ref) {
    const { def, domain: owner } = resolveRef(schema, domain, spec.$ref);
    return def ? valueFor(schema, owner, def, depth + 1) : 'x';
  }
  if (spec.enum) return spec.enum[0];
  switch (spec.type) {
    case 'integer':
      return 1;
    case 'number':
      return 1.5;
    case 'boolean':
      return true;
    case 'string':
      return 'x';
    case 'array':
      return depth >= MAX_DEPTH || !spec.items
        ? []
        : [valueFor(schema, domain, spec.items, depth + 1)];
    case 'object': {
      const value = {};
      if (depth >= MAX_DEPTH) return value;
      for (const property of spec.properties ?? [])
        if (!property.optional)
          value[property.name] = valueFor(schema, domain, property, depth + 1);
      return value;
    }
    default:
      return 'x';
  }
}

export const commandsOf = schema =>
  schema.domains.flatMap(domain =>
    (domain.commands ?? []).map(command => ({
      method: `${domain.domain}.${command.name}`,
      domain: domain.domain,
      command,
    }))
  );

/** Params with every required parameter filled. */
export function paramsFor(schema, { domain, command }) {
  const params = {};
  for (const field of command.parameters ?? [])
    if (!field.optional) params[field.name] = valueFor(schema, domain, field);
  return params;
}

/** The kind of value a spec holds once `$ref` is followed: string, number, boolean, array, object or any. */
export function kindOf(schema, domain, spec) {
  if (spec.$ref) {
    const { def, domain: owner } = resolveRef(schema, domain, spec.$ref);
    return def ? kindOf(schema, owner, def) : 'any';
  }
  if (spec.enum) return 'string';
  if (spec.type === 'integer' || spec.type === 'number') return 'number';
  return spec.type ?? 'any';
}

/** A value of another primitive kind than the spec, or undefined when the spec accepts anything. */
export function wrongValueFor(kind) {
  return {
    string: 123,
    number: 'x',
    boolean: 'x',
    array: 'x',
    object: 'x',
  }[kind];
}

// The methods the cdp-domain dump never sends: the one hazard list of the package.
const UNSAFE = new Set(CDP_DOMAIN_HAZARDS.map(hazard => hazard.method));
const READS =
  /^(get|list|query|read|describe|search|take|collect|resolve|request)|^(is|can)[A-Z]/;
const UNDOES = /^(stop|clear|unbind|cancel|find)/;

/**
 * One harmless method per domain: a read that needs no permission (`get...` first, then other read
 * verbs, then `disable`/`enable`, then an undo such as `clear`; a domain with only setters gets its
 * first command). The params are generated from the schema.
 */
export function harmlessPerDomain(schema, classify) {
  const picks = new Map();
  for (const domain of schema.domains) {
    const candidates = (domain.commands ?? [])
      .map(command => ({
        method: `${domain.domain}.${command.name}`,
        domain: domain.domain,
        command,
      }))
      .filter(entry => !UNSAFE.has(entry.method) && !entry.command.deprecated);
    // A domain whose commands all need the user's permission is exercised with the acknowledgement.
    const free = candidates.filter(entry => classify(entry.method) === 'free');
    const all = free.length
      ? free
      : candidates.map(entry => ({ ...entry, acknowledge: true }));
    const rank = entry => {
      const { name } = entry.command;
      const required = (entry.command.parameters ?? []).some(p => !p.optional);
      if (name.startsWith('get')) return required ? 1 : 0;
      if (READS.test(name)) return required ? 3 : 2;
      if (name === 'disable') return required ? 6 : 4;
      if (name === 'enable') return required ? 7 : 5;
      if (UNDOES.test(name)) return required ? 8.5 : 8;
      return 9;
    };
    const best = all.sort((a, b) => rank(a) - rank(b))[0];
    if (best) picks.set(domain.domain, best);
  }
  return picks;
}
