import { z } from 'zod';
import { CLI_REQUIRED, projectCliProperties } from './cli-contract.js';

type JsonSchema = Record<string, unknown>;

function objectBranch(
  output: JsonSchema,
  required: string[],
  overrides: Record<string, JsonSchema> = {},
): JsonSchema {
  const properties = output.properties as Record<string, JsonSchema> | undefined;
  return {
    type: 'object',
    properties: Object.fromEntries([...new Set([...required, ...Object.keys(overrides)])].flatMap(field => {
      const property = properties?.[field];
      if (!property && !overrides[field]) return [];
      const projected = { ...(property ?? {}), ...(overrides[field] ?? {}) };
      if (Object.hasOwn(overrides[field] ?? {}, 'const')) {
        delete projected.default;
        delete projected.enum;
      }
      return [[field, projected]];
    })),
    ...(required.length ? { required } : {}),
  };
}

const requireFields = (output: JsonSchema, ...fields: string[]): JsonSchema => objectBranch(output, fields);
const requireNonEmptyArray = (output: JsonSchema, field: string): JsonSchema => {
  const branch = objectBranch(output, [field], { [field]: { minItems: 1 } });
  const property = (branch.properties as Record<string, JsonSchema> | undefined)?.[field];
  if (property) delete property.default;
  return branch;
};

function constrainArray(output: JsonSchema, field: string, minItems: number, maxItems?: number): void {
  const properties = output.properties as Record<string, JsonSchema> | undefined;
  const property = properties?.[field];
  if (!property) return;
  delete property.default;
  property.minItems = minItems;
  if (maxItems !== undefined) property.maxItems = maxItems;
}

function replaceWithVariants(
  output: JsonSchema,
  variants: Array<{ required?: string[]; overrides: Record<string, JsonSchema> }>,
): void {
  const rootSchema = output.$schema;
  const variantSchemas = variants.map(({ required = [], overrides }) => {
    const variant = structuredClone(output);
    delete variant.$schema;
    const properties = variant.properties as Record<string, JsonSchema>;
    for (const [field, override] of Object.entries(overrides)) {
      properties[field] = { ...(properties[field] ?? {}), ...override };
      if (Object.hasOwn(override, 'const')) {
        delete properties[field]!.default;
        delete properties[field]!.enum;
      }
    }
    const baseRequired = Array.isArray(variant.required) ? variant.required as string[] : [];
    variant.required = [...new Set([...baseRequired, ...required])];
    return variant;
  });
  for (const key of Object.keys(output)) delete output[key];
  if (rootSchema !== undefined) output.$schema = rootSchema;
  output.oneOf = variantSchemas;
}

/** Preserve handler-only refinements that Zod cannot emit as JSON Schema. */
function applyCommandConstraints(output: JsonSchema, commandName: string): void {
  const allOf: JsonSchema[] = [];
  switch (commandName) {
    case 'task create':
      constrainArray(output, 'path', 1);
      break;
    case 'task claim':
      allOf.push({ anyOf: [
        {
          ...objectBranch(output, ['task_id'], { next: { const: false } }),
        },
        {
          ...objectBranch(output, ['plan_id', 'next'], { next: { const: true } }),
        },
      ] });
      break;
    case 'task depend':
      constrainArray(output, 'depends_on', 1);
      break;
    case 'work start':
      constrainArray(output, 'file', 1);
      allOf.push({ anyOf: [requireFields(output, 'run_id'), requireFields(output, 'rationale', 'test_plan')] });
      break;
    case 'work show':
      constrainArray(output, 'file', 1, 1);
      break;
    case 'lock release':
      allOf.push({ anyOf: [requireFields(output, 'run_id'), requireNonEmptyArray(output, 'target_file')] });
      break;
    case 'signal resolve':
      allOf.push({ oneOf: [
        requireNonEmptyArray(output, 'signal_id'),
        requireFields(output, 'thread_id'),
      ] });
      break;
    case 'verify mark':
      constrainArray(output, 'run_id', 1);
      allOf.push(
        { anyOf: [
          requireNonEmptyArray(output, 'run_id'),
          objectBranch(output, ['all_pending'], { all_pending: { const: true } }),
        ] },
        { anyOf: [
          objectBranch(output, ['status'], { status: { const: 'FAILED' } }),
          requireFields(output, 'message'),
        ] },
        { anyOf: [
          objectBranch(output, [], { adopt_verification: { const: false } }),
          objectBranch(output, ['adopt_verification', 'run_id'], {
            adopt_verification: { const: true },
            run_id: { minItems: 1, maxItems: 1 },
            all_pending: { const: false },
          }),
        ] },
      );
      break;
    case 'history recovery':
      replaceWithVariants(output, [
        { overrides: { action: { const: 'report' } } },
        { required: ['action', 'confirm'], overrides: { action: { const: 'reconcile' }, confirm: { const: 'reconcile' } } },
      ]);
      return;
    case 'history evidence':
      replaceWithVariants(output, [
        { overrides: { action: { const: 'report' } } },
        { required: ['action', 'confirm'], overrides: { action: { const: 'reclaim' }, confirm: { const: 'reclaim' } } },
      ]);
      return;
    case 'maintenance retention':
      {
        const report = objectBranch(output, [], { action: { const: 'report' } });
        (report.properties as Record<string, unknown>).confirm = false;
        allOf.push({ oneOf: [
          report,
        objectBranch(output, ['action', 'confirm'], {
          action: { const: 'apply' },
          confirm: { const: 'apply-retention' },
        }),
        ] });
      }
      break;
    case 'maintenance store-retire':
      {
        const report = objectBranch(output, [], { action: { const: 'report' } });
        (report.properties as Record<string, unknown>).confirm = false;
        (report.properties as Record<string, unknown>).report_file = false;
        allOf.push({ oneOf: [
          report,
          objectBranch(output, ['action', 'confirm', 'report_file'], {
            action: { const: 'apply' },
            confirm: { const: 'retire' },
          }),
        ] });
      }
      break;
  }
  if (allOf.length) output.allOf = [...(Array.isArray(output.allOf) ? output.allOf : []), ...allOf];
}

/** Exact command fields, shared by execution, discovery and host binding metadata. */
export function projectCommandInput(commandName: string, schema: z.ZodType): Record<string, unknown> {
  const output = structuredClone(z.toJSONSchema(schema)) as Record<string, unknown>;
  const properties = output.properties as Record<string, unknown> | undefined;
  const action = commandName.split(" ")[1];
  // History maintenance has a mode within its route; it is not the route's
  // command selector. Keep that explicit public field in discovery/validation.
  const selectsRoute = commandName !== 'history recovery'
    && commandName !== 'history evidence'
    && commandName !== 'maintenance retention'
    && commandName !== 'maintenance store-retire';
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
  applyCommandConstraints(output, commandName);
  return output;
}
