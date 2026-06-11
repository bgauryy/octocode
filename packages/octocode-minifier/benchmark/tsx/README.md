# TSX (.tsx)

Source sample: `tsx/next-index.tsx`

Strategy: `conservative`

Agent rating: **7.2/10 (good)**

Artifacts:

- `raw/source.excerpt.txt`
- `minified/content-view.excerpt.txt`
- `minified/apply-minification.excerpt.txt`
- `minified/minify-content-sync.excerpt.txt`
- `minified/minify-content-async.excerpt.txt`
- `symbol/signatures.txt`

| Tool | Bytes | Cut | Time | Rating |
| --- | ---: | ---: | ---: | ---: |
| input | 285 | - | - | - |
| content-view | 284 | 0.4% | 0.108 ms | 6.8/10 |
| applyMinification | 284 | 0.4% | 3.252 ms | 6.8/10 |
| sync minify | 284 | 0.4% | 1.822 ms | 6.8/10 |
| async minify | 284 | 0.4% | 4.598 ms | 6.8/10 |
| symbols | 147 | 48.4% | 0.161 ms | 8/10 |

## Notes

- engine-backed or parser-backed path.

## Before Excerpt

```tsx
import Link from "next/link";
import Layout from "../components/Layout";

const IndexPage = () => (
  <Layout title="Home | Next.js + TypeScript Example">
    <h1>Hello Next.js 👋</h1>
    <p>
      <Link href="/about">About</Link>
    </p>
  </Layout>
);

export default IndexPage;

```

## Content-View Excerpt

```tsx
import Link from "next/link";
import Layout from "../components/Layout";

const IndexPage = () => (
  <Layout title="Home | Next.js + TypeScript Example">
    <h1>Hello Next.js 👋</h1>
    <p>
      <Link href="/about">About</Link>
    </p>
  </Layout>
);

export default IndexPage;
```

## Apply Minification Excerpt

```tsx
import Link from "next/link";
import Layout from "../components/Layout";

const IndexPage = () => (
  <Layout title="Home | Next.js + TypeScript Example">
    <h1>Hello Next.js 👋</h1>
    <p>
      <Link href="/about">About</Link>
    </p>
  </Layout>
);

export default IndexPage;
```

## Sync Minify Excerpt

```tsx
import Link from "next/link";
import Layout from "../components/Layout";

const IndexPage = () => (
  <Layout title="Home | Next.js + TypeScript Example">
    <h1>Hello Next.js 👋</h1>
    <p>
      <Link href="/about">About</Link>
    </p>
  </Layout>
);

export default IndexPage;
```

## Async Minify Excerpt

```tsx
import Link from "next/link";
import Layout from "../components/Layout";

const IndexPage = () => (
  <Layout title="Home | Next.js + TypeScript Example">
    <h1>Hello Next.js 👋</h1>
    <p>
      <Link href="/about">About</Link>
    </p>
  </Layout>
);

export default IndexPage;
```

## Symbols

```txt
 1| import Link from "next/link";
 2| import Layout from "../components/Layout";
 4| const IndexPage = () => (
11| );
13| export default IndexPage;
```
