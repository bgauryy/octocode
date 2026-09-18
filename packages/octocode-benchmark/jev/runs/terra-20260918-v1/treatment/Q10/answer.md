# Q10 — Axios entry chain

The repository is [`axios/axios`](https://github.com/axios/axios); its retrieved language breakdown is dominated by **JavaScript**. In [`package.json`](https://github.com/axios/axios/blob/main/package.json), legacy `main` is `./dist/node/axios.cjs`. For modern Node CommonJS resolution, the root `exports` default `require` target is also `./dist/node/axios.cjs`; its default (ESM) target is `./index.js`. [`index.js`](https://github.com/axios/axios/blob/main/index.js) imports `./lib/axios.js`.

Thus the evidence shows CJS `main`/`exports.require` landing in the Node CJS distribution bundle, and the source ESM root entry landing at `lib/axios.js`. The bundle's own internal mapping was not fetched, so I do not claim a direct observed `dist/node/axios.cjs → lib/axios.js` import.
