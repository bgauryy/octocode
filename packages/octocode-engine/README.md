# @octocodeai/octocode-engine

Compatibility package for the engine primitives now distributed by
[`@octocodeai/octocode-native/engine`](https://www.npmjs.com/package/@octocodeai/octocode-native).

Existing CommonJS, ESM, and TypeScript imports remain functional:

```js
const engine = require('@octocodeai/octocode-engine');
// New code:
const nativeEngine = require('@octocodeai/octocode-native/engine');
```

The package contains no Rust source or platform binaries. It depends on the exact
consolidated native version and re-exports its `./engine` entrypoint.
