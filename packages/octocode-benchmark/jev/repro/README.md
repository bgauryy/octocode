# React #37655 runtime investigation

This parent-run verification is separate from the Terra comparison. The evaluated arms do not receive its results during research. It uses actual React DOM 19.2.8 in jsdom, with a development build, and preserves raw warnings plus the committed `_debugHookTypes` array.

```sh
npm ci --ignore-scripts --no-audit --no-fund
node reproduce.mjs
node build-candidate.mjs
JEV_REACT_CANDIDATE=1 node reproduce.mjs
```

The seven isolated processes cover two distinct thenables with context between them, a single thenable, reuse of one promise, a stateful hook between suspensions, adjacent thenables, a debug-value hook between suspensions, and an intentional hook-order violation. Assertions check warning behavior and rendered output. The double-context case also checks the exact context-entry count. Results are in baseline-results.json and candidate-results.json.

The experimental candidate makes the DEV mount recorder track the same cursor as the update recorder. It verifies an already-recorded slot through updateHookTypesDev and appends only after that prefix ends. It removes the duplicated context/debug-value entries in this matrix while preserving a genuine mismatch warning. Merely truncating at hookTypesUpdateIndexDev + 1 on every mount append is unsafe because the original mount recorder never advances that cursor.

## Compare the arm proposals

```sh
JEV_REACT_CANDIDATE=baseline-proposal node reproduce.mjs
JEV_REACT_CANDIDATE=treatment-interpretation node reproduce.mjs
```

The first command intentionally fails its warning assertion. Its baseline-proposal-results.json retains every case: the baseline answer's literal transition patch still warns on the original bug and introduces warnings in all four previously passing controls. Its code does not check that the dispatcher was actually the rerender dispatcher; the transition also occurs during ordinary mount use() calls, when its cursor assumption is invalid.

The treatment answer supplied prose rather than code. The parent implemented its “first newly-mounted hook” as an explicit once-per-replay flag, reset at the replay loop. That interpretation passes all seven checks, as recorded in treatment-interpretation-results.json. The parent supplied this missing implementation detail after the measured run; do not score it as runtime verification performed by Terra or proof that Jev caused an improvement. The treatment's pre-Jev decision already proposed guarded truncation.

build-candidate.mjs generates three ignored experimental bundles from exact known regions in the pinned distribution; installed dependencies remain unchanged. candidate-source.patch describes the cursor-aware candidate's equivalent upstream source edit. These are local candidates, not submitted or production-ready React fixes. Full reconciler, Strict Mode, hydration, render-phase update, aborted replay and DevTools regressions still require the upstream test suite.

Source anchors: [mount/update recorders](https://github.com/react/react/blob/71f725593739d2cb5866a282a1075d581831722f/packages/react-reconciler/src/ReactFiberHooks.js#L311), [replay reset](https://github.com/react/react/blob/71f725593739d2cb5866a282a1075d581831722f/packages/react-reconciler/src/ReactFiberHooks.js#L791), [dispatcher switch](https://github.com/react/react/blob/71f725593739d2cb5866a282a1075d581831722f/packages/react-reconciler/src/ReactFiberHooks.js#L1098), [reported issue](https://github.com/react/react/issues/37655). The same mount recorder was read at v19.2.8 (resolved ref 1dd4ecbdabf826f527fc9a58c05ea70375b7d170).
