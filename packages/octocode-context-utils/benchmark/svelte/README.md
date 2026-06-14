# Svelte (.svelte)

Source sample: `svelte/vite-app.svelte`

Strategy: `aggressive`

| Tool | Bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| input | 2665 | - | - |
| content-view | 2664 | 0% | 0.111 ms |
| applyMinification | 2096 | 21.4% | 0.09 ms |
| sync minify | 2096 | 21.4% | 0.088 ms |
| async minify | 2096 | 21.4% | 0.102 ms |
| symbols | 343 | 87.1% | 2.242 ms |

## Notes

- engine-backed or parser-backed path.
- content-view kept original because the readable output was not shorter.

## Before Excerpt

```svelte
<script>
  import svelteLogo from './assets/svelte.svg'
  import viteLogo from './assets/vite.svg'
  import heroImg from './assets/hero.png'
  import Counter from './lib/Counter.svelte'
</script>

<section id="center">
  <div class="hero">
    <img src={heroImg} class="base" width="170" height="179" alt="" />
    <img src={svelteLogo} class="framework" alt="Svelte logo" />
    <img src={viteLogo} class="vite" alt="Vite logo" />
  </div>
  <div>
    <h1>Get started</h1>
    <p>Edit <code>src/App.svelte</code> and save to test <code>HMR</code></p>
  </div>
  <Counter />
</section>

<div class="ticks"></div>

<section id="next-steps">
  <div id="docs">
    <svg class="icon" role="presentation" aria-hidden="true">
      <use href="/icons.svg#documentation-icon"></use>
    </svg>
    <h2>Documentation</h2>
    <p>Your questions, answered</p>
    <ul>
      <li>
        <a href="https://vite.dev/" target="_blank" rel="noreferrer">
          <img class="logo" src={viteLogo} alt="" />
          Explore Vite
        </a>
      </li>
      <li>
        <a href="https://svelte.dev/" target="_blank" rel="noreferrer">
          <img class="button-icon" src={svelteLogo} alt="" />
          Learn more
        </a>


... [truncated 865 chars] ...

rel="noreferrer">
          <svg class="button-icon" role="presentation" aria-hidden="true">
            <use href="/icons.svg#x-icon"></use>
          </svg>
          X.com
        </a>
      </li>
      <li>
        <a href="https://bsky.app/profile/vite.dev" target="_blank" rel="noreferrer">
          <svg class="button-icon" role="presentation" aria-hidden="true">
            <use href="/icons.svg#bluesky-icon"></use>
          </svg>
          Bluesky
        </a>
      </li>
    </ul>
  </div>
</section>

<div class="ticks"></div>
<section id="spacer"></section>

```

## Content-View Excerpt

```svelte
<script>
  import svelteLogo from './assets/svelte.svg'
  import viteLogo from './assets/vite.svg'
  import heroImg from './assets/hero.png'
  import Counter from './lib/Counter.svelte'
</script>

<section id="center">
  <div class="hero">
    <img src={heroImg} class="base" width="170" height="179" alt="" />
    <img src={svelteLogo} class="framework" alt="Svelte logo" />
    <img src={viteLogo} class="vite" alt="Vite logo" />
  </div>
  <div>
    <h1>Get started</h1>
    <p>Edit <code>src/App.svelte</code> and save to test <code>HMR</code></p>
  </div>
  <Counter />
</section>

<div class="ticks"></div>

<section id="next-steps">
  <div id="docs">
    <svg class="icon" role="presentation" aria-hidden="true">
      <use href="/icons.svg#documentation-icon"></use>
    </svg>
    <h2>Documentation</h2>
    <p>Your questions, answered</p>
    <ul>
      <li>
        <a href="https://vite.dev/" target="_blank" rel="noreferrer">
          <img class="logo" src={viteLogo} alt="" />
          Explore Vite
        </a>
      </li>
      <li>
        <a href="https://svelte.dev/" target="_blank" rel="noreferrer">
          <img class="button-icon" src={svelteLogo} alt="" />
          Learn more
        </a>


... [truncated 864 chars] ...

 rel="noreferrer">
          <svg class="button-icon" role="presentation" aria-hidden="true">
            <use href="/icons.svg#x-icon"></use>
          </svg>
          X.com
        </a>
      </li>
      <li>
        <a href="https://bsky.app/profile/vite.dev" target="_blank" rel="noreferrer">
          <svg class="button-icon" role="presentation" aria-hidden="true">
            <use href="/icons.svg#bluesky-icon"></use>
          </svg>
          Bluesky
        </a>
      </li>
    </ul>
  </div>
</section>

<div class="ticks"></div>
<section id="spacer"></section>
```

## Apply Minification Excerpt

```svelte
<script> import svelteLogo from './assets/svelte.svg' import viteLogo from './assets/vite.svg' import heroImg from './assets/hero.png' import Counter from './lib/Counter.svelte'</script><section id="center"><div class="hero"><img src={heroImg}class="base" width="170" height="179" alt="" /><img src={svelteLogo}class="framework" alt="Svelte logo" /><img src={viteLogo}class="vite" alt="Vite logo" /></div><div><h1>Get started</h1><p>Edit<code>src/App.svelte</code> and save to test<code>HMR</code></p></div><Counter /></section><div class="ticks"></div><section id="next-steps"><div id="docs"><svg class="icon" role="presentation" aria-hidden="true"><use href="/icons.svg#documentation-icon"></use></svg><h2>Documentation</h2><p>Your questions,answered</p><ul><li><a href="https://vite.dev/" target="_blank" rel="noreferrer"><img class="logo" src={viteLogo}alt="" /> Explore Vite</a></li><li><a href="https://svelte.dev/" target="_blank" rel="noreferrer"><img class="button-icon" src={svelteLogo}alt="" /> Learn more</a></li></ul></div><div id="social"><svg class="icon" role="presentation" aria-hidden="true"><use href="/icons.svg#social-icon"></use></svg><h2>Connect with us</h2><p>Join the Vite community</p><ul><li><a hr

... [truncated 296 chars] ...

"presentation" aria-hidden="true"><use href="/icons.svg#discord-icon"></use></svg> Discord</a></li><li><a href="https://x.com/vite_js" target="_blank" rel="noreferrer"><svg class="button-icon" role="presentation" aria-hidden="true"><use href="/icons.svg#x-icon"></use></svg> X.com</a></li><li><a href="https://bsky.app/profile/vite.dev" target="_blank" rel="noreferrer"><svg class="button-icon" role="presentation" aria-hidden="true"><use href="/icons.svg#bluesky-icon"></use></svg> Bluesky</a></li></ul></div></section><div class="ticks"></div><section id="spacer"></section>
```

## Sync Minify Excerpt

```svelte
<script> import svelteLogo from './assets/svelte.svg' import viteLogo from './assets/vite.svg' import heroImg from './assets/hero.png' import Counter from './lib/Counter.svelte'</script><section id="center"><div class="hero"><img src={heroImg}class="base" width="170" height="179" alt="" /><img src={svelteLogo}class="framework" alt="Svelte logo" /><img src={viteLogo}class="vite" alt="Vite logo" /></div><div><h1>Get started</h1><p>Edit<code>src/App.svelte</code> and save to test<code>HMR</code></p></div><Counter /></section><div class="ticks"></div><section id="next-steps"><div id="docs"><svg class="icon" role="presentation" aria-hidden="true"><use href="/icons.svg#documentation-icon"></use></svg><h2>Documentation</h2><p>Your questions,answered</p><ul><li><a href="https://vite.dev/" target="_blank" rel="noreferrer"><img class="logo" src={viteLogo}alt="" /> Explore Vite</a></li><li><a href="https://svelte.dev/" target="_blank" rel="noreferrer"><img class="button-icon" src={svelteLogo}alt="" /> Learn more</a></li></ul></div><div id="social"><svg class="icon" role="presentation" aria-hidden="true"><use href="/icons.svg#social-icon"></use></svg><h2>Connect with us</h2><p>Join the Vite community</p><ul><li><a hr

... [truncated 296 chars] ...

"presentation" aria-hidden="true"><use href="/icons.svg#discord-icon"></use></svg> Discord</a></li><li><a href="https://x.com/vite_js" target="_blank" rel="noreferrer"><svg class="button-icon" role="presentation" aria-hidden="true"><use href="/icons.svg#x-icon"></use></svg> X.com</a></li><li><a href="https://bsky.app/profile/vite.dev" target="_blank" rel="noreferrer"><svg class="button-icon" role="presentation" aria-hidden="true"><use href="/icons.svg#bluesky-icon"></use></svg> Bluesky</a></li></ul></div></section><div class="ticks"></div><section id="spacer"></section>
```

## Async Minify Excerpt

```svelte
<script> import svelteLogo from './assets/svelte.svg' import viteLogo from './assets/vite.svg' import heroImg from './assets/hero.png' import Counter from './lib/Counter.svelte'</script><section id="center"><div class="hero"><img src={heroImg}class="base" width="170" height="179" alt="" /><img src={svelteLogo}class="framework" alt="Svelte logo" /><img src={viteLogo}class="vite" alt="Vite logo" /></div><div><h1>Get started</h1><p>Edit<code>src/App.svelte</code> and save to test<code>HMR</code></p></div><Counter /></section><div class="ticks"></div><section id="next-steps"><div id="docs"><svg class="icon" role="presentation" aria-hidden="true"><use href="/icons.svg#documentation-icon"></use></svg><h2>Documentation</h2><p>Your questions,answered</p><ul><li><a href="https://vite.dev/" target="_blank" rel="noreferrer"><img class="logo" src={viteLogo}alt="" /> Explore Vite</a></li><li><a href="https://svelte.dev/" target="_blank" rel="noreferrer"><img class="button-icon" src={svelteLogo}alt="" /> Learn more</a></li></ul></div><div id="social"><svg class="icon" role="presentation" aria-hidden="true"><use href="/icons.svg#social-icon"></use></svg><h2>Connect with us</h2><p>Join the Vite community</p><ul><li><a hr

... [truncated 296 chars] ...

"presentation" aria-hidden="true"><use href="/icons.svg#discord-icon"></use></svg> Discord</a></li><li><a href="https://x.com/vite_js" target="_blank" rel="noreferrer"><svg class="button-icon" role="presentation" aria-hidden="true"><use href="/icons.svg#x-icon"></use></svg> X.com</a></li><li><a href="https://bsky.app/profile/vite.dev" target="_blank" rel="noreferrer"><svg class="button-icon" role="presentation" aria-hidden="true"><use href="/icons.svg#bluesky-icon"></use></svg> Bluesky</a></li></ul></div></section><div class="ticks"></div><section id="spacer"></section>
```

## Symbols

```txt
 1| <script>
 2|   import svelteLogo from './assets/svelte.svg'
 3|   import viteLogo from './assets/vite.svg'
 4|   import heroImg from './assets/hero.png'
 5|   import Counter from './lib/Counter.svelte'
 8| <section id="center">
23| <section id="next-steps">
24|   <div id="docs">
45|   <div id="social">
89| <section id="spacer"></section>
```
