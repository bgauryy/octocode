# Q6 — Express router trace

The layer-matching loop is not in the current Express repository's own implementation. Its root manifest depends on the separate [`router` package](https://github.com/expressjs/express/blob/9a34acf03cb818ff3f8bc40e44176e277a25cbb9/package.json#L49-L65) at `^2.2.0`, which leads to `pillarjs/router`.

In [`pillarjs/router/index.js`](https://github.com/pillarjs/router/blob/bda4af36c1e66811717b13421579c63029ea2877/index.js#L188-L288), `Router.prototype.handle` installs and calls the inner `next` function. `next` advances `idx` through `stack` in the `while` loop; each layer is checked by `matchLayer(layer, path)`. The per-layer path predicate is [`Layer.prototype.match`](https://github.com/pillarjs/router/blob/bda4af36c1e66811717b13421579c63029ea2877/lib/layer.js#L178-L218) in `lib/layer.js`, which runs its matchers and stores matched params/path.
