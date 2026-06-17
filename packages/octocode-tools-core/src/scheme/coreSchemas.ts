import { z } from 'zod';

type AnyZodObject = z.ZodObject<z.ZodRawShape>;
export type QueryShape = Record<string, z.ZodTypeAny>;
type ExtendableZodObject = AnyZodObject & {
  safeExtend?: (shape: QueryShape) => AnyZodObject;
};

function copyDescription<T extends z.ZodTypeAny>(
  source: z.ZodTypeAny | undefined,
  target: T
): T {
  return source?.description && !target.description
    ? (target.describe(source.description) as T)
    : target;
}

function describeOverridesFromCore<T extends AnyZodObject>(
  coreSchema: T,
  overrides: QueryShape
): QueryShape {
  const describedOverrides: QueryShape = {};

  for (const [fieldName, fieldSchema] of Object.entries(overrides)) {
    describedOverrides[fieldName] = copyDescription(
      coreSchema.shape[fieldName] as z.ZodTypeAny | undefined,
      fieldSchema
    );
  }

  return describedOverrides;
}

function extendObjectSchema<T extends AnyZodObject>(
  schema: T,
  shape: QueryShape
): T {
  const extendableSchema = schema as ExtendableZodObject;
  const extendedSchema = extendableSchema.safeExtend
    ? extendableSchema.safeExtend(shape)
    : schema.extend(shape);

  return extendedSchema as unknown as T;
}

export function describeQuerySchema<T extends AnyZodObject>(
  coreSchema: T,
  overrides: QueryShape = {},
  options: { strict?: boolean } = {}
): T {
  const extended = extendObjectSchema(
    coreSchema,
    describeOverridesFromCore(coreSchema, overrides)
  );
  return (options.strict ? extended.strict() : extended) as unknown as T;
}

export function createQueryShapeSchema<T extends AnyZodObject>(
  coreSchema: T,
  overrides: QueryShape = {},
  options: { strict?: boolean } = {}
): AnyZodObject {
  // Strip unknown query keys instead of rejecting them — a legacy/removed/typo
  // field must never hard-fail the whole MCP call with a schema mismatch.
  const schema = z.object({
    ...coreSchema.shape,
    ...describeOverridesFromCore(coreSchema, overrides),
  });
  return options.strict ? schema.strict() : schema;
}
