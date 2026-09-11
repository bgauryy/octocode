import { z } from 'zod';
import { CLI_REQUIRED, projectCliProperties } from './cli-contract.js';

/** Exact command fields, shared by execution, discovery and host binding metadata. */
export function projectCommandInput(commandName: string, schema: z.ZodType): Record<string, unknown> {
  const output = structuredClone(z.toJSONSchema(schema)) as Record<string, unknown>;
  const properties = output.properties as Record<string, unknown> | undefined;
  const action = commandName.split(" ")[1];
  // History maintenance has a mode within its route; it is not the route's
  // command selector. Keep that explicit public field in discovery/validation.
  const selectsRoute = commandName !== 'history recovery' && commandName !== 'history evidence';
  if (properties && action && properties.action && selectsRoute) delete properties.action;
  let canonicalNames: Record<string, string> = {};
  if (properties) {
    canonicalNames = projectCliProperties(properties, commandName);
  }
  const existingRequired = Array.isArray(output.required)
    ? (output.required as string[])
      .filter((field) => field !== "action" || !selectsRoute)
      .map((field) => canonicalNames[field] ?? field)
      .filter((field) => properties?.[field] && !Object.hasOwn(properties[field] as object, "default"))
    : [];
  const required = [...new Set([...existingRequired, ...(CLI_REQUIRED[commandName] ?? [])])];
  if (required.length > 0) output.required = required;
  else delete output.required;
  return output;
}
