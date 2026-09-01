import { z } from 'zod';

type AnyZodObject = z.ZodObject<z.ZodRawShape>;
export type QueryShape = Record<string, z.ZodTypeAny>;
type ExtendableZodObject = AnyZodObject & {
  safeExtend?: (shape: QueryShape) => AnyZodObject;
};
type QuerySchemaOptions = {
  strict?: boolean;
  omit?: readonly string[];
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

function omitObjectSchema<T extends AnyZodObject>(
  schema: T,
  fields: readonly string[] | undefined
): T {
  if (!fields?.length) return schema;
  const omitted = new Set(fields);
  return z.object(
    Object.fromEntries(
      Object.entries(schema.shape).filter(([field]) => !omitted.has(field))
    )
  ) as unknown as T;
}

export function describeQuerySchema<T extends AnyZodObject>(
  coreSchema: T,
  overrides: QueryShape = {},
  options: QuerySchemaOptions = {}
): T {
  const base = omitObjectSchema(coreSchema, options.omit);
  const extended = extendObjectSchema(
    base,
    describeOverridesFromCore(base, overrides)
  );
  return extended.strict() as unknown as T;
}

export function createQueryShapeSchema<T extends AnyZodObject>(
  coreSchema: T,
  overrides: QueryShape = {},
  options: QuerySchemaOptions = {}
): AnyZodObject {
  const base = omitObjectSchema(coreSchema, options.omit);
  const schema = z.object({
    ...base.shape,
    ...describeOverridesFromCore(base, overrides),
  });
  return schema.strict();
}
