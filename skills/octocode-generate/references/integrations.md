# Integrations: analytics, monitoring, backends and platform SDKs

Load when the app needs a hosted service: analytics, error monitoring, a database or backend platform, a headless store or CMS, payments, auth, email or AI. Offer the user the leading options in each category with their docs. Add only what the features need, and ask before you add tracking, which affects privacy, consent and cost. Versions are from 2026-11-25. Before you write code, read the provider's docs page and its official example at the current release ([research](research.md#recipe), steps 1–4), then check the SDK set with `check-versions.mjs`.

Every integration needs the same four things:

- **Env.** Each key or ID goes in `.env.example`. Expose only public IDs to the client (`NEXT_PUBLIC_`, `VITE_`, `EXPO_PUBLIC_`); keep secrets server-side.
- **Seam.** One module wraps the SDK (`lib/analytics.ts`, `lib/db.ts`), so the rest of the code and the tests depend on your interface.
- **Off switch.** A missing env value disables the integration rather than breaking development and CI.
- **One minimal test.** Run it against an emulator or local server, or a fake of the seam, never the live service.

## Analytics

| Need | Leading option | SDK and setup | Docs |
|---|---|---|---|
| Traffic and marketing analytics | Google Analytics 4 | Next.js: `@next/third-parties`, `<GoogleAnalytics gaId="G-…" />` in the root layout, `sendGAEvent` for events. Other web apps: the gtag.js tag (`react-ga4` in React SPAs). Apps: Firebase Analytics. Server events: Measurement Protocol | https://developers.google.com/analytics/devguides/collection/ga4 |
| Product analytics, feature flags, session replay, experiments | PostHog | `posthog-js`, `posthog-node`, `posthog-react-native`; `posthog` (PyPI) | https://posthog.com/docs |
| Cookieless, privacy-friendly page analytics | Plausible or Umami | `@plausible-analytics/tracker` (`plausible-tracker` is deprecated); `@umami/node` | https://plausible.io/docs, https://umami.is/docs |
| Event analytics for growth teams | Mixpanel or Amplitude | `mixpanel-browser`; `@amplitude/analytics-browser` | https://docs.mixpanel.com, https://amplitude.com/docs |
| Sites hosted on Vercel | Vercel Web Analytics, Speed Insights | `@vercel/analytics`, `@vercel/speed-insights` | https://vercel.com/docs/analytics |
| Core Web Vitals to any backend | `web-vitals` | Send LCP, INP and CLS through the analytics seam | — |

GA4 needs a consent banner where the law requires one. On Expo, Firebase Analytics and Crashlytics need React Native Firebase, which is native code: it runs in a development build (`expo-dev-client`), not Expo Go. The Firebase JS SDK has neither for mobile (`expo/expo/docs/pages/guides/using-firebase.mdx`).

## Error monitoring and observability

| Need | Leading option | SDK and setup | Docs |
|---|---|---|---|
| Errors, traces, release health | Sentry | Next.js: `npx @sentry/wizard@latest -i nextjs` (it edits config files: run it after approval and review the diff). `@sentry/react`, `@sentry/node`, `@sentry/react-native`, `sentry-sdk` (Python) | https://docs.sentry.io |
| Vendor-neutral traces and metrics | OpenTelemetry | `@vercel/otel` (Next.js), `@opentelemetry/sdk-node`, `opentelemetry-sdk` (Python) | https://opentelemetry.io/docs |
| Mobile crashes | Crashlytics (React Native Firebase, FlutterFire) or Sentry | — | https://rnfirebase.io |

## Databases and backend platforms

| Service | Use when | SDK, local development and tests | Docs |
|---|---|---|---|
| Firebase (Firestore, Auth, Storage, Functions, Hosting) | A managed backend with realtime documents and auth, strong on mobile | Web `firebase` 13 (modular imports); server `firebase-admin` (Node.js, Python); Flutter: FlutterFire (`flutterfire configure`); React Native Firebase. Google Cloud client libraries reach Firestore from Java, Python, Node.js, Go, PHP, C# and Ruby. Tests: `firebase emulators:exec "<test command>"`; security rules with `@firebase/rules-unit-testing` | https://firebase.google.com/docs, https://firebase.google.com/docs/firestore/client/libraries, https://firebase.google.com/docs/emulator-suite |
| MongoDB (Atlas or self-hosted) | Document data with flexible schemas | Driver `mongodb` 7, or Mongoose 9 for models; Python: PyMongo with its async API (Motor is deprecated). Tests: `mongodb-memory-server`; development: the `mongo` image. Reuse one client per process | https://www.mongodb.com/docs/drivers/node/current/, https://mongoosejs.com/docs/, https://www.mongodb.com/docs/atlas/ |
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
| Shopify storefront | Commerce on Shopify | Hydrogen `@shopify/hydrogen` | https://shopify.dev/docs/storefronts/headless/hydrogen |

The `@wix/sdk` registry entry names no source repository: read current usage in the templates (wix/headless-templates `nextjs/commerce/middleware.ts`). Keep `WIX_CLIENT_ID` in env.

## Payments, auth, email and AI

| Need | Leading option | Watch | Docs |
|---|---|---|---|
| Payments and subscriptions | Stripe: `stripe` (server), `@stripe/stripe-js` (browser) | Verify webhook signatures; build with test-mode keys | https://docs.stripe.com |
| Marketplace or multi-party payouts (studios, sellers, instructors) | Stripe Connect | Onboard each payee as a connected account; decide who pays fees | https://docs.stripe.com/connect |
| Hosted auth | Clerk (`@clerk/nextjs`), or the auth of Firebase, Supabase, Wix members or Base44. Self-hosted: Better Auth ([libraries](libraries.md#auth)) | One auth source per app | https://clerk.com/docs |
| Transactional email | Resend (`resend`) | Verify the sending domain | https://resend.com/docs |
| AI features and agents | AI SDK, Mastra, MCP SDKs ([services](stacks-services.md#ai-apps-agents-and-mcp-servers)) | Keys server-side | https://ai-sdk.dev/docs, https://mastra.ai/docs, https://modelcontextprotocol.io |
