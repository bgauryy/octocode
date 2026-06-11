# HTML (.html)

Source sample: `html/vite-index.html`

Strategy: `aggressive`

Agent rating: **7.7/10 (good)**

Artifacts:

- `raw/source.excerpt.txt`
- `minified/content-view.excerpt.txt`
- `minified/apply-minification.excerpt.txt`
- `minified/minify-content-sync.excerpt.txt`
- `minified/minify-content-async.excerpt.txt`
- `symbol/signatures.txt`

| Tool | Bytes | Cut | Time | Rating |
| --- | ---: | ---: | ---: | ---: |
| input | 359 | - | - | - |
| content-view | 358 | 0.3% | 0.127 ms | 8.3/10 |
| applyMinification | 314 | 12.5% | 0.118 ms | 8.3/10 |
| sync minify | 314 | 12.5% | 0.068 ms | 8.3/10 |
| async minify | 305 | 15% | 2.963 ms | 8.3/10 |
| symbols | 257 | 28.4% | 0.359 ms | 6.5/10 |

## Notes

- engine-backed or parser-backed path.

## Before Excerpt

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <link rel="icon" type="image/svg+xml" href="/favicon.svg" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Vite + JS</title>
  </head>
  <body>
    <div id="app"></div>
    <script type="module" src="/src/main.js"></script>
  </body>
</html>

```

## Content-View Excerpt

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <link rel="icon" type="image/svg+xml" href="/favicon.svg" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Vite + JS</title>
  </head>
  <body>
    <div id="app"></div>
    <script type="module" src="/src/main.js"></script>
  </body>
</html>
```

## Apply Minification Excerpt

```html
<!doctype html><html lang="en"><head><meta charset="UTF-8" /><link rel="icon" type="image/svg+xml" href="/favicon.svg" /><meta name="viewport" content="width=device-width, initial-scale=1.0" /><title>Vite + JS</title></head><body><div id="app"></div><script type="module" src="/src/main.js"></script></body></html>
```

## Sync Minify Excerpt

```html
<!doctype html><html lang="en"><head><meta charset="UTF-8" /><link rel="icon" type="image/svg+xml" href="/favicon.svg" /><meta name="viewport" content="width=device-width, initial-scale=1.0" /><title>Vite + JS</title></head><body><div id="app"></div><script type="module" src="/src/main.js"></script></body></html>
```

## Async Minify Excerpt

```html
<!doctype html><html lang="en"><head><meta charset="UTF-8"><link rel="icon" type="image/svg+xml" href="/favicon.svg"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Vite + JS</title></head><body><div id="app"></div><script type="module" src="/src/main.js"></script></body></html>
```

## Symbols

```txt
 1| <!doctype html>
 5|     <link rel="icon" type="image/svg+xml" href="/favicon.svg" />
 6|     <meta name="viewport" content="width=device-width, initial-scale=1.0" />
10|     <div id="app"></div>
11|     <script type="module" src="/src/main.js"></script>
```
