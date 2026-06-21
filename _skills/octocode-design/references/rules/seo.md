# SEO Rules

Search engine optimization foundations for web applications.

## Metadata Template

Every page must define these metadata fields:

```tsx
// Next.js App Router metadata
export const metadata: Metadata = {
  title: "Page Title — Site Name",
  description: "Unique, specific description (150-160 chars). No keyword stuffing.",
  openGraph: {
    title: "Page Title — Site Name",
    description: "Same or tailored description for social sharing.",
    images: [{ url: "/og-image.png", width: 1200, height: 630 }],
    type: "website",
    siteName: "Site Name",
  },
  twitter: {
    card: "summary_large_image",
    title: "Page Title",
    description: "Description for Twitter cards.",
    images: ["/og-image.png"],
  },
  robots: {
    index: true,
    follow: true,
  },
  alternates: {
    canonical: "https://example.com/page",
  },
};
```

### Meta description rules
- Every page must have a **unique** `<meta name="description">`
- 150-160 characters, specific to page content
- No keyword stuffing — natural language that describes the page value
- Match user search intent

## Structured Data (JSON-LD)

Add structured data for rich search results:

```tsx
<script type="application/ld+json">
{JSON.stringify({
  "@context": "https://schema.org",
  "@type": "WebApplication",
  "name": "App Name",
  "description": "What the app does",
  "applicationCategory": "BusinessApplication",
  "operatingSystem": "Web",
  "offers": {
    "@type": "Offer",
    "price": "0",
    "priceCurrency": "USD"
  }
})}
</script>
```

Common types: `WebApplication`, `Product`, `Article`, `Organization`, `BreadcrumbList`, `FAQ`.

Validate with [Google Rich Results Test](https://search.google.com/test/rich-results).

## Technical SEO

### Sitemap

```xml
<!-- public/sitemap.xml or generated dynamically -->
<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url>
    <loc>https://example.com/</loc>
    <lastmod>2024-01-01</lastmod>
    <priority>1.0</priority>
  </url>
</urlset>
```

### Robots

```txt
# public/robots.txt
User-agent: *
Allow: /
Disallow: /api/
Disallow: /admin/
Sitemap: https://example.com/sitemap.xml
```

### Canonical URLs

Every page must specify `<link rel="canonical" href="..." />` to prevent duplicate content issues.

### Heading hierarchy

- Exactly one `<h1>` per page
- Headings in order: h1 -> h2 -> h3 (no skipping levels)
- Headings describe content structure, not styling

### Semantic HTML for crawlers

```html
<main>           <!-- Primary content -->
<article>        <!-- Self-contained content -->
<section>        <!-- Thematic grouping -->
<nav>            <!-- Navigation -->
<header/footer>  <!-- Page or section headers/footers -->
```

### Image SEO

```tsx
<Image
  src="/product.webp"
  alt="Red leather sofa with wooden legs"  // Descriptive, not "image1.jpg"
  width={800}
  height={600}
/>
```

- Meaningful `alt` text for all content images
- `alt=""` for decorative images only
- Descriptive filenames (`red-leather-sofa.webp` not `IMG_1234.webp`)

## Performance as SEO

Google uses Core Web Vitals as ranking signals:
- LCP <= 2.5s, INP <= 200ms, CLS <= 0.1
- See [Performance Rules](./performance.md) for optimization details

## Internationalization (i18n)

If multi-language:
```html
<html lang="en">
<link rel="alternate" hreflang="es" href="https://example.com/es/" />
<link rel="alternate" hreflang="en" href="https://example.com/" />
<link rel="alternate" hreflang="x-default" href="https://example.com/" />
```

## References

- [Chrome Lighthouse SEO](https://developer.chrome.com/docs/lighthouse/seo)
- [Google Structured Data](https://developers.google.com/search/docs/guides/mark-up-content)
- [Google Rich Results Test](https://search.google.com/test/rich-results)
- [Schema.org](https://schema.org)
