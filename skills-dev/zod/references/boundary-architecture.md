Load when choosing where a schema parses data or which layer owns it.

# Boundary Architecture — Where Zod Fits

## Overview

Zod belongs at **system boundaries** — the points where your application receives data it doesn't control. Parse once at the boundary, then pass typed data inward. Domain logic should never see `unknown`.

## Express — Route Handler vs Middleware

The examples use Express response methods. For Fastify, use its request/reply types and validation integration. Follow the framework's async error path, and use async Zod parsing when the schema contains asynchronous work.

### In the Route Handler (Recommended for Most Cases)

```typescript
app.post("/api/users", async (req, res) => {
  const result = CreateUserSchema.safeParse(req.body)
  if (!result.success) {
    return res.status(400).json({
      errors: z.flattenError(result.error),
    })
  }
  const user = await createUser(result.data)
  res.status(201).json(user)
})
```

### As Middleware (For Shared Validation Logic)

```typescript
function validate<T extends z.ZodType>(schema: T) {
  return (req: Request, res: Response, next: NextFunction) => {
    const result = schema.safeParse(req.body)
    if (!result.success) {
      return res.status(400).json({
        errors: z.flattenError(result.error),
      })
    }
    req.body = result.data // validated output; this does not refine a later handler's TS type
    next()
  }
}

app.post("/api/users", validate(CreateUserSchema), async (req, res) => {
  // Supply the parsed body type through Express route generics or a typed wrapper.
  const user = await createUser(req.body)
  res.status(201).json(user)
})
```

### When to Use Each

| Approach | Use When |
|----------|----------|
| Route handler | Schema is specific to one route, custom error responses needed |
| Middleware | Same schema/error format across many routes |

## tRPC — `.input()` Parsing

tRPC handles boundary parsing for you via `.input()`:

```typescript
export const userRouter = router({
  create: publicProcedure
    .input(CreateUserSchema) // tRPC owns the schema's parsing integration
    .mutation(async ({ input }) => {
      // input is fully typed — z.infer<typeof CreateUserSchema>
      return createUser(input)
    }),

  getById: publicProcedure
    .input(z.object({ id: z.string() }))
    .query(async ({ input }) => {
      return getUser(input.id)
    }),
})
```

Do not add duplicate parsing around a framework integration that already validates this boundary. Follow its configured error response contract.

## Next.js Server Actions

```typescript
"use server"

const CreatePostSchema = z.object({
  title: z.string().min(1),
  body: z.string().min(10),
})

export async function createPost(formData: FormData) {
  // Parse at the top of the action
  const result = CreatePostSchema.safeParse({
    title: formData.get("title"),
    body: formData.get("body"),
  })

  if (!result.success) {
    return { errors: z.flattenError(result.error).fieldErrors }
  }

  // Typed from here on
  await db.posts.create({ data: result.data })
  revalidatePath("/posts")
}
```

### Next.js Route Handlers

```typescript
export async function POST(request: Request) {
  const body = await request.json()
  const result = CreateUserSchema.safeParse(body)

  if (!result.success) {
    return Response.json(
      { errors: z.flattenError(result.error) },
      { status: 400 }
    )
  }

  const user = await createUser(result.data)
  return Response.json(user, { status: 201 })
}
```

## React Hook Form — zodResolver

The form library handles the boundary:

```typescript
import { useForm } from "react-hook-form"
import { zodResolver } from "@hookform/resolvers/zod"

const ProfileSchema = z.object({
  name: z.string().min(1),
  bio: z.string().max(500).optional(),
})

type ProfileForm = z.infer<typeof ProfileSchema>

function ProfileEditor() {
  const { register, handleSubmit, formState: { errors } } = useForm<ProfileForm>({
    resolver: zodResolver(ProfileSchema),
  })

  const onSubmit = (data: ProfileForm) => {
    // data is already validated — no safeParse needed
    updateProfile(data)
  }

  return (
    <form onSubmit={handleSubmit(onSubmit)}>
      <input {...register("name")} />
      {errors.name && <span>{errors.name.message}</span>}
    </form>
  )
}
```

## Environment Variables — Startup Parsing

Parse env vars through the application's config owner at startup. Fail fast if misconfigured; in Octocode that owner is `@octocodeai/config`, so do not add a second env loader. HTTP examples omit transport handling: malformed JSON, non-success upstream responses and network failures need their own error path before schema validation.

### Manual Approach

```typescript
// config/env.ts — parsed at import time
const EnvSchema = z.object({
  DATABASE_URL: z.url(),
  API_KEY: z.string().min(1),
  PORT: z.coerce.number().int().min(1).max(65535).default(3000),
  NODE_ENV: z.enum(["development", "production", "test"]).default("development"),
})

export const env = EnvSchema.parse(process.env)
// App crashes at startup if env is invalid — this is intentional
```

### With t3-env

```typescript
import { createEnv } from "@t3-oss/env-nextjs"

export const env = createEnv({
  server: {
    DATABASE_URL: z.url(),
    API_KEY: z.string().min(1),
  },
  client: {
    NEXT_PUBLIC_APP_URL: z.url(),
  },
  runtimeEnv: {
    DATABASE_URL: process.env.DATABASE_URL,
    API_KEY: process.env.API_KEY,
    NEXT_PUBLIC_APP_URL: process.env.NEXT_PUBLIC_APP_URL,
  },
})
```

## External API Responses

Always validate data coming from external services — their schemas can change without warning.

```typescript
const WeatherResponse = z.object({
  temperature: z.number(),
  humidity: z.number().min(0).max(100),
  conditions: z.string(),
})

async function getWeather(city: string) {
  const res = await fetch(`https://api.weather.example/v1/${city}`)
  const json = await res.json()

  // Parse at the boundary — don't trust external data
  const result = WeatherResponse.safeParse(json)
  if (!result.success) {
    logger.warn("weather_api_schema_mismatch", {
      schema: "WeatherResponse",
      fieldErrors: z.flattenError(result.error).fieldErrors,
    })
    throw new ExternalServiceError("Weather API returned unexpected shape")
  }

  return result.data // typed Weather
}
```

## Database Layer

Parse DB results when the schema might drift from the actual DB shape (e.g., after migrations, with untyped ORMs, or with raw SQL).

```typescript
const UserRow = z.object({
  id: z.string(),
  name: z.string(),
  email: z.string(),
  created_at: z.coerce.date(),
})

async function getUserById(id: string) {
  const row = await db.query("SELECT * FROM users WHERE id = $1", [id])
  // Validate if using raw SQL or untyped ORM
  return UserRow.parse(row)
}
```

If you use a fully typed ORM like Prisma or Drizzle that generates types from your schema, additional Zod parsing of DB results is usually unnecessary.

## Summary: Boundary Layer Checklist

| Boundary | Who Parses | Schema Location |
|----------|------------|-----------------|
| Express/Fastify route | Your middleware or handler | `api/[resource]/schemas.ts` |
| tRPC procedure | tRPC via `.input()` | Inline or co-located |
| Next.js Server Action | Top of action function | Co-located with action |
| React Hook Form | zodResolver | `features/[name]/form-schema.ts` |
| Env vars | Startup (parse, not safeParse) | `config/env.ts` |
| External API response | After fetch, before use | Co-located with API client |
| Database results | After query (if untyped) | Co-located with data access |
| Message queue | Top of consumer handler | Co-located with consumer |
