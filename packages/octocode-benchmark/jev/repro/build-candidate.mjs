import { readFileSync, writeFileSync } from 'node:fs';
import assert from 'node:assert/strict';

// Generate a local experimental bundle; leave installed packages untouched.
const original = readFileSync(new URL('./node_modules/react-dom/cjs/react-dom-client.development.js', import.meta.url), 'utf8');
const before = `    function mountHookTypesDev() {
      var hookName = currentHookNameInDev;
      null === hookTypesDev
        ? (hookTypesDev = [hookName])
        : hookTypesDev.push(hookName);
    }`;
const after = `    function mountHookTypesDev() {
      var hookName = currentHookNameInDev;
      if (null === hookTypesDev) {
        hookTypesDev = [hookName];
        hookTypesUpdateIndexDev = 0;
      } else if (hookTypesUpdateIndexDev + 1 < hookTypesDev.length) {
        updateHookTypesDev();
      } else {
        hookTypesUpdateIndexDev++;
        hookTypesDev.push(hookName);
      }
    }`;
assert.equal(original.split(before).length, 2, 'Expected exactly one known React 19.2.8 mount recorder.');
writeFileSync(new URL('./patched-client.cjs', import.meta.url), original.replace(before, after));
console.log('Generated experimental patched-client.cjs from the pinned React 19.2.8 bundle.');

// Literal implementation of the completed baseline arm's proposed transition edit.
const transitionBefore = `      null ===
        (null === workInProgressHook
          ? index.memoizedState
          : workInProgressHook.next) &&
        ((index = index.alternate),
        (ReactSharedInternals.H =
          null !== index && null !== index.memoizedState
            ? HooksDispatcherOnUpdateInDEV
            : HooksDispatcherOnMountInDEV));`;
const transitionAfter = `      if (null === (null === workInProgressHook ? index.memoizedState : workInProgressHook.next)) {
        index = index.alternate;
        if (null === index || null === index.memoizedState) {
          if (null !== hookTypesDev) hookTypesDev.length = hookTypesUpdateIndexDev + 1;
          ReactSharedInternals.H = HooksDispatcherOnMountInDEV;
        } else {
          ReactSharedInternals.H = HooksDispatcherOnUpdateInDEV;
        }
      }`;
assert.equal(original.split(transitionBefore).length, 2, 'Expected exactly one known dispatcher transition.');
writeFileSync(new URL('./baseline-proposal.cjs', import.meta.url), original.replace(transitionBefore, transitionAfter));
console.log('Generated baseline-proposal.cjs to test the baseline arm solution literally.');

// The treatment supplied prose, not code. This parent-authored interpretation
// makes its phrase "first newly-mounted hook" explicitly once per replay pass.
const treatmentMount = `    var didTrimReplayHookTypes = false;
    function mountHookTypesDev() {
      var hookName = currentHookNameInDev;
      if (null === hookTypesDev) {
        hookTypesDev = [hookName];
      } else {
        if (hookTypesUpdateIndexDev >= 0 && !didTrimReplayHookTypes) {
          hookTypesDev.length = hookTypesUpdateIndexDev + 1;
          didTrimReplayHookTypes = true;
        }
        hookTypesDev.push(hookName);
      }
    }`;
const resetBefore = `        hookTypesUpdateIndexDev = -1;
        ReactSharedInternals.H = HooksDispatcherOnRerenderInDEV;`;
const resetAfter = `        hookTypesUpdateIndexDev = -1;
        didTrimReplayHookTypes = false;
        ReactSharedInternals.H = HooksDispatcherOnRerenderInDEV;`;
assert.equal(original.split(resetBefore).length, 2, 'Expected exactly one rerender reset.');
writeFileSync(new URL('./treatment-interpretation.cjs', import.meta.url), original.replace(before, treatmentMount).replace(resetBefore, resetAfter));
console.log('Generated parent-authored treatment-interpretation.cjs; this implementation was absent from the treatment answer.');
