# Vue (.vue)

Source sample: `vue/vite-app.vue`

Strategy: `aggressive`

Agent rating: **7.2/10 (good)**

Agent understanding from minified output: **7.9/10 (good)**

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
| content-view | 118 | 0.8% | 0.046 ms | 7.5/10 |
| applyMinification | 112 | 5.9% | 0.026 ms | 7.5/10 |
| sync minify | 112 | 5.9% | 0.019 ms | 7.5/10 |
| async minify | 110 | 7.6% | 0.294 ms | 7.5/10 |
| symbols | 87 | 26.9% | 0.157 ms | 6.5/10 |

## Agent Understanding

Measured from `standard` minified output.

| Component | Score |
| --- | ---: |
| syntax anchors | 6.7/10 (2/3) |
| delimiter structure | 8/10 |
| output health | 10/10 |
| context budget | 6/10 |
| symbol context | 10/10 |
| signals passed | 6/6 |

## Agent Observation By Output Level

Ratings are computed from the actual raw, standard, minify, and symbol outputs
for this language sample.

| Level | Bytes | Cut | Agent observation | Syntax anchors | Structure |
| --- | ---: | ---: | ---: | ---: | ---: |
| none | 119 | 0% | 10/10 excellent | 6.7/10 | 8/10 |
| standard | 118 | 0.8% | 7.9/10 good | 6.7/10 | 8/10 |
| minify | 110 | 7.6% | 8/10 strong | 6.7/10 | 8/10 |
| symbols | 87 | 26.9% | 8.2/10 strong | 6.7/10 | 8/10 |

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
