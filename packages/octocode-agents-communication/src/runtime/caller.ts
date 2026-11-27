/** Tool-call attribution comes from the bound session, never from message payloads. */
import type { CommandResults } from './catalog-types.generated.js';
import { ValidationError } from './errors.js';
import { asObject, type JsonObject } from './validation.js';

export type Caller = Pick<CommandResults['binding'], 'id' | 'name' | 'vendor' | 'vendorSession'>;

export function callerIdentity(identity: Caller): Caller {
  return { id: identity.id, name: identity.name, vendor: identity.vendor,
    ...(identity.vendorSession === undefined ? {} : { vendorSession: identity.vendorSession }) };
}

/** Project the existing identity contract instead of maintaining a second schema. */
export function callerSchema(identitySchema: unknown): JsonObject {
  const properties = asObject(asObject(identitySchema, 'identity schema').properties, 'identity properties');
  return { type: 'object', properties: Object.fromEntries(['id', 'name', 'vendor', 'vendorSession'].map(key => [key, properties[key]!])),
    required: ['id', 'name', 'vendor'], additionalProperties: false,
    description: 'Your introduction on this call: copy id, name, vendor and known vendorSession from binding.caller. The bound runtime supplies it when omitted and rejects mismatches.' };
}

/** Validate supplied attribution; omission still resolves to the authoritative binding. */
export function boundCaller(value: unknown, identity: Caller): Caller {
  const actual = callerIdentity(identity);
  const supplied = asObject(value, 'tool arguments').caller;
  if (supplied !== undefined) for (const [field, expected] of Object.entries(asObject(supplied, 'caller'))) {
    if (actual[field as keyof Caller] !== expected) throw new ValidationError(`caller.${field} does not match the current binding`);
  }
  return actual;
}
