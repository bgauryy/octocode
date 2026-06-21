# Performance Rules

Core Web Vitals targets and optimization patterns.

## Core Web Vitals Targets

| Metric | Good | Needs improvement | Poor | What it measures |
|--------|------|-------------------|------|-----------------|
| **LCP** | <= 2.5s | 2.5-4.0s | > 4.0s | Loading - largest visible element |
| **INP** | <= 200ms | 200-500ms | > 500ms | Interactivity - worst input delay |
| **CLS** | <= 0.1 | 0.1-0.25 | > 0.25 | Stability - visual layout shifts |

Measure at the **75th percentile** of real user data (CrUX / RUM), not just lab tools.

## Loading Performance (LCP)

### Image optimization

```tsx
// Next.js Image — automatic sizing, format, lazy loading
import Image from "next/image";
<Image src="/hero.jpg" alt="..." width={1200} height={600} priority />

// Priority for above-the-fold (LCP candidate)
// Lazy for below-the-fold (default)
```

- Serve **WebP/AVIF** formats
- Provide `width` and `height` to prevent layout shifts
- Use `srcset` / `sizes` for responsive images
- Preload LCP image: `<link rel="preload" as="image" href="..." />`

### Font loading

```css
/* Optimal font loading — swap for fast text render */
@font-face {
  font-family: "Inter";
  src: url("/fonts/inter.woff2") format("woff2");
  font-display: swap;
  unicode-range: U+0000-00FF; /* Latin subset first */
}
```

- Use **WOFF2** format exclusively
- Subset fonts to needed character ranges
- Preload critical fonts: `<link rel="preload" as="font" type="font/woff2" href="..." crossorigin />`
- Consider **system font stack** to eliminate font loading entirely

### Critical rendering path

- Inline **critical CSS** for above-the-fold content
- Defer non-critical CSS with `media="print"` trick or dynamic injection
- Minimize render-blocking resources
- Use `<link rel="preconnect">` for third-party origins

## Interactivity (INP)

### Minimize main thread work

```tsx
// Break up long tasks
// Use React.startTransition for non-urgent updates
import { startTransition } from "react";

startTransition(() => {
  setSearchResults(filtered);
});

// Debounce expensive handlers
const handleInput = useDebouncedCallback((value) => {
  search(value);
}, 300);
```

### Server Components (React/Next.js)

```tsx
// Server Component — zero client JS for this component
async function ProductList() {
  const products = await getProducts();
  return <ul>{products.map(p => <li key={p.id}>{p.name}</li>)}</ul>;
}

// Client Component — only when browser APIs needed
"use client";
function AddToCartButton({ id }) {
  return <button onClick={() => addToCart(id)}>Add</button>;
}
```

Move as much as possible to Server Components to reduce client bundle.

### Code splitting

```tsx
// Route-level splitting (automatic in Next.js App Router)
// Component-level splitting for heavy widgets
const Chart = dynamic(() => import("./Chart"), {
  loading: () => <Skeleton className="h-64" />,
  ssr: false,
});
```

## Visual Stability (CLS)

### Reserve space for dynamic content

```tsx
// Images — always specify dimensions
<Image width={400} height={300} alt="..." />

// Embeds — use aspect-ratio
<div className="aspect-video">
  <iframe src="..." />
</div>

// Skeleton loaders — match final layout
<Skeleton className="h-10 w-full" />  {/* matches input height */}
```

### Avoid layout-shifting patterns

- No injecting content above existing content after load
- No dynamically resizing fonts after web font loads (use `font-display: swap` + `size-adjust`)
- No lazy-loaded ads/banners without reserved space
- Animations use `transform` and `opacity` only (compositor-only properties)

## Performance Budget

| Resource | Budget |
|----------|--------|
| Total page JS (compressed) | < 200 KB |
| First-party JS | < 100 KB |
| Total CSS | < 50 KB |
| LCP image | < 200 KB |
| Web fonts | < 100 KB total |
| Total page weight | < 1 MB |

### Resource hints

```html
<link rel="preconnect" href="https://fonts.googleapis.com" />
<link rel="dns-prefetch" href="https://api.example.com" />
<link rel="preload" as="image" href="/hero.webp" />
<link rel="prefetch" href="/next-page.js" />
```

## Monitoring

- **Field data**: Chrome UX Report (CrUX), `web-vitals` library, RUM analytics
- **Lab tools**: Lighthouse, Chrome DevTools Performance panel
- **Lighthouse cannot measure INP** — use TBT (Total Blocking Time) as a lab proxy
- **Automate**: Lighthouse CI in PR checks, CrUX dashboards for production

## References

- [Core Web Vitals — web.dev](https://web.dev/articles/vitals)
- [MDN Web Performance](https://developer.mozilla.org/en-US/docs/Learn_web_development/Extensions/Performance)
- [MDN Performance Best Practices](https://developer.mozilla.org/en-US/docs/Learn_web_development/Extensions/Performance/Best_practices)
