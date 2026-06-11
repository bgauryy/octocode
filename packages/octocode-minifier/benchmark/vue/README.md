# Vue (.vue)

Source sample: `vue/vite-app.vue`

Strategy: `aggressive`

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
| input | 119 | - | - | - |
| content-view | 118 | 0.8% | 0.033 ms | 7.5/10 |
| applyMinification | 112 | 5.9% | 0.036 ms | 7.5/10 |
| sync minify | 112 | 5.9% | 0.024 ms | 7.5/10 |
| async minify | 110 | 7.6% | 0.172 ms | 7.5/10 |
| symbols | 87 | 26.9% | 0.077 ms | 6.5/10 |

## Notes

- engine-backed or parser-backed path.

## Before Excerpt

```vue
<script setup>
import HelloWorld from './components/HelloWorld.vue'
</script>

<template>
  <HelloWorld />
</template>

```

## Content-View Excerpt

```vue
<script setup>
import HelloWorld from './components/HelloWorld.vue'
</script>

<template>
  <HelloWorld />
</template>
```

## Apply Minification Excerpt

```vue
<script setup> import HelloWorld from './components/HelloWorld.vue' </script><template><HelloWorld /></template>
```

## Sync Minify Excerpt

```vue
<script setup> import HelloWorld from './components/HelloWorld.vue' </script><template><HelloWorld /></template>
```

## Async Minify Excerpt

```vue
<script setup>import HelloWorld from"./components/HelloWorld.vue";</script><template><HelloWorld /></template>
```

## Symbols

```txt
1| <script setup>
2| import HelloWorld from './components/HelloWorld.vue'
5| <template>
```
