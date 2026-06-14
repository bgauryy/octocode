# Vue (.vue)

Source sample: `vue/vite-app.vue`

Strategy: `aggressive`

| Tool | Bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| input | 119 | - | - |
| content-view | 118 | 0.8% | 0.015 ms |
| applyMinification | 111 | 6.7% | 0.007 ms |
| sync minify | 111 | 6.7% | 0.005 ms |
| async minify | 111 | 6.7% | 0.017 ms |
| symbols | 87 | 26.9% | 0.021 ms |

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
<script setup> import HelloWorld from './components/HelloWorld.vue'</script><template><HelloWorld /></template>
```

## Sync Minify Excerpt

```vue
<script setup> import HelloWorld from './components/HelloWorld.vue'</script><template><HelloWorld /></template>
```

## Async Minify Excerpt

```vue
<script setup> import HelloWorld from './components/HelloWorld.vue'</script><template><HelloWorld /></template>
```

## Symbols

```txt
1| <script setup>
2| import HelloWorld from './components/HelloWorld.vue'
5| <template>
```
