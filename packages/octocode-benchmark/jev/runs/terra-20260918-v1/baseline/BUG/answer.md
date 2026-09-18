# BUG — React issue #37655

## Verdict

The issue is an allegation, not runtime reproduction evidence. I did **not** run the reported app or downloaded React scripts, so there is no reproduction credit. However, its proposed mechanism is consistent with the current source and with the checked `v19.2.0` source ref: `mountHookTypesDev` appends to `hookTypesDev` without a replay-prefix truncation ([current](https://github.com/facebook/react/blob/71f725593739d2cb5866a282a1075d581831722f/packages/react-reconciler/src/ReactFiberHooks.js#L311-L323), [`v19.2.0`](https://github.com/facebook/react/blob/ae74234eae6ebd62f19190731278e20bc1c37d51/packages/react-reconciler/src/ReactFiberHooks.js#L309-L321)). I could not independently verify the reporter's package-release label `19.2.8`; the comparison source ref available here was `v19.2.0`.

## Trigger and mechanism

The reported trigger is two distinct `use(thenable)` suspensions in an initial mount, with only non-stateful hooks (such as `useContext`) between them. On replay, `renderWithHooksAgain` resets `currentHook`, `workInProgressHook`, and `hookTypesUpdateIndexDev` but preserves the debug hook-type array ([source](https://github.com/facebook/react/blob/71f725593739d2cb5866a282a1075d581831722f/packages/react-reconciler/src/ReactFiberHooks.js#L791-L855)). `useThenable` switches from the rerender dispatcher back to the mount dispatcher when no work-in-progress state hook remains ([source](https://github.com/facebook/react/blob/71f725593739d2cb5866a282a1075d581831722f/packages/react-reconciler/src/ReactFiberHooks.js#L1098-L1168)). Thus the replay can re-verify the context-hook prefix, then append the first post-`use` hook again. A second suspension/replay makes the duplicate visible; after completion, `finishRenderingHooks` commits `hookTypesDev` to `_debugHookTypes` ([source](https://github.com/facebook/react/blob/71f725593739d2cb5866a282a1075d581831722f/packages/react-reconciler/src/ReactFiberHooks.js#L636-L655)), causing a later update's dev order comparison to be offset.

This supports a **DEV diagnostic corruption**, not a production hook-chain corruption: the relevant recording and commit are inside `__DEV__` blocks. I did not verify the issue's broader DevTools or “only warning per component name” impact claims beyond the reporter's description.

## Suggested repair and tests

Do **not** truncate inside `mountHookTypesDev` on every append: a first mount has `hookTypesUpdateIndexDev === -1`, so that would discard earlier mount records. Instead, at the specific `useThenable` transition from the rerender dispatcher to `HooksDispatcherOnMountInDEV`, truncate the prior tentative suffix once, before selecting that mount dispatcher:

```js
if (__DEV__) {
  if (currentFiber === null || currentFiber.memoizedState === null) {
    if (hookTypesDev !== null) {
      hookTypesDev.length = hookTypesUpdateIndexDev + 1
    }
    ReactSharedInternals.H = HooksDispatcherOnMountInDEV
  } else {
    ReactSharedInternals.H = HooksDispatcherOnUpdateInDEV
  }
}
```

The guard retains exactly the prefix replayed by `updateHookTypesDev` and avoids altering ordinary mount recording. A regression should use the reconciler test renderer in DEV: render a `Suspense` component with two separately tracked thenables, three `useContext` calls before the first `use`, one between the two `use`s, then `useState` and an effect that schedules an update. Resolve both thenables and assert no hook-order warning on the update. Add controls for one `use`, the same thenable twice, a stateful hook between the uses, and a genuinely conditional hook (which must still warn). The plausible alternative—that the app truly called hooks conditionally—is weakened by this same fixed component call sequence, but only an executed regression proves it end-to-end.
