Load when the app needs a database, a backend platform (Firebase, MongoDB, Supabase, Convex, Base44, Replit), a headless business backend, a CMS or a storefront. Read [integrations](integrations.md) first: it holds the checks to make before writing code and the four things every integration needs.

# Integrations: databases, backend platforms, CMS and commerce

## Databases and backend platforms

| Service | Use when | SDK, local development and tests | Docs |
|---|---|---|---|
| Firebase (Firestore, Auth, Storage, Functions, Hosting) | A managed backend with realtime documents and auth, strong on mobile | Web `firebase` 13 (modular imports); server `firebase-admin` (Node.js, Python); Flutter: FlutterFire (`flutterfire configure`); React Native Firebase. Google Cloud client libraries reach Firestore from Java, Python, Node.js, Go, PHP, C# and Ruby. Tests: `firebase emulators:exec "<test command>"`; security rules with `@firebase/rules-unit-testing` | https://firebase.google.com/docs, https://firebase.google.com/docs/firestore/client/libraries, https://firebase.google.com/docs/emulator-suite |
| MongoDB (Atlas or self-hosted) | Document data with flexible schemas | Driver `mongodb` 7, or Mongoose 9 for models; Python: PyMongo with its async API (Motor is deprecated since 2026-05-14). Tests: `mongodb-memory-server`; development: the `mongo` image. Reuse one client per process | https://www.mongodb.com/docs/drivers/node/current/, https://mongoosejs.com/docs/, https://www.mongodb.com/docs/atlas/ |
| Supabase (Postgres, auth, storage, realtime) | SQL with a hosted backend | `@supabase/supabase-js`, `@supabase/ssr`; local stack `supabase start`. Row-level security is the authorization layer | https://supabase.com/docs |
| Convex | A reactive database with TypeScript server functions | `convex`; `npx convex dev` | https://docs.convex.dev |
| Base44 | The app's backend is a Base44 app (entities, auth, AI agents, functions, integrations, analytics, payments) | `npm install @base44/sdk`, then `createClient({ appId })` (the ID is in the Base44 editor URL); `base44.entities.<Name>.list()`, `base44.auth.loginViaEmailPassword(...)`. Base44-generated apps already ship a configured client | https://docs.base44.com/developers/references/sdk/getting-started/overview |
| Replit App Storage | File storage for an app that runs on Replit | `@replit/object-storage` (JavaScript), `replit-object-storage` (Python): `new Client()`; methods return a result value to check, rather than throwing | https://docs.replit.com/features/sdks/object-storage-javascript-sdk |
| Postgres through an ORM | You run your own database | Drizzle or Prisma ([libraries](libraries.md#data)); Postgres container or PGlite in tests | — |

## Headless business backends, CMS and commerce

| Platform | Use when | SDK and starting point | Docs |
|---|---|---|---|
| Wix Headless | A custom frontend over a full business backend: stores, bookings, events, CMS, members, payments. Free to start | `@wix/sdk` plus a module package per feature (`@wix/stores`, `@wix/ecom`, `@wix/data`, `@wix/bookings`, `@wix/members`, `@wix/blog`, `@wix/redirects` for hosted checkout and login). `createClient({ modules, auth: OAuthStrategy({ clientId }) })`, with visitor tokens from `auth.generateVisitorTokens()` in middleware. Official templates live in wix/headless-templates under `nextjs/`, `astro/` and `react-native/` (for example `nextjs/commerce`, `nextjs/classes-subscriptions`, `astro/scheduler`): list the current set with `ghStructure` `{"operation":"tree","owner":"wix","repo":"headless-templates","maxDepth":2}` | https://www.wix.com/headless, https://dev.wix.com/docs/go-headless |
| Content CMS | Editors manage content; developers own the frontend | Sanity `@sanity/client`; Contentful `contentful` | https://www.sanity.io/docs, https://www.contentful.com/developers/docs/ |
| Shopify storefront | Commerce on Shopify | Hydrogen `@shopify/hydrogen` | https://shopify.dev/docs/storefronts/hydrogen |

The `@wix/sdk` registry entry names no source repository: read current usage in the templates (wix/headless-templates `nextjs/commerce/middleware.ts`). Keep `WIX_CLIENT_ID` in env.
