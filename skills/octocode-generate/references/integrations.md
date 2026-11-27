# Integrations: analytics, monitoring, payments, auth, email and AI

Load when the app needs a hosted service: analytics, error monitoring, a database or backend platform, a headless store or CMS, payments, auth, email or AI. Databases, backend platforms, headless business backends, CMS and commerce are in [backends](integrations-backends.md). Offer the user the leading options in each category with their docs. Add only what the features need, and add tracking, which affects privacy, consent and cost, only after the user's yes ([intake](intake.md#when-to-ask-more)). Versions are from 2026-11-25. Before you write code, read the provider's docs page and its official example at the current release ([research](research.md#recipe), steps 1–4), then check the SDK set with `check-versions.mjs`.

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
| Cookieless, privacy-friendly page analytics | Plausible or Umami | `@plausible-analytics/tracker` (`plausible-tracker` is deprecated); `@umami/node` | https://plausible.io/docs, https://docs.umami.is/docs |
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

## Payments, auth, email and AI

| Need | Leading option | Watch | Docs |
|---|---|---|---|
| Payments and subscriptions | Stripe: `stripe` (server), `@stripe/stripe-js` (browser) | Verify webhook signatures; build with test-mode keys | https://docs.stripe.com |
| Marketplace or multi-party payouts (studios, sellers, instructors) | Stripe Connect | Onboard each payee as a connected account; decide who pays fees | https://docs.stripe.com/connect |
| Hosted auth | Clerk (`@clerk/nextjs`), or the auth of Firebase, Supabase, Wix members or Base44. Self-hosted: Better Auth ([libraries](libraries.md#auth)) | One auth source per app | https://clerk.com/docs |
| Transactional email | Resend (`resend`) | Verify the sending domain | https://resend.com/docs |
| AI features and agents | AI SDK, Mastra, MCP SDKs ([services](stacks-services.md#ai-apps-agents-and-mcp-servers)) | Keys server-side | https://ai-sdk.dev/docs, https://mastra.ai/docs, https://modelcontextprotocol.io |
