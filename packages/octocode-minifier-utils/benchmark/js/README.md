# JavaScript (.js)

Source sample: `js/00-react-hooks.js`

Strategy: `terser`

Agent rating: **7.7/10 (good)**

Agent understanding from minified output: **9.5/10 (excellent)**

Artifacts:

- `raw/source.excerpt.txt`
- `minified/content-view.excerpt.txt`
- `minified/apply-minification.excerpt.txt`
- `minified/minify-content-sync.excerpt.txt`
- `minified/minify-content-async.excerpt.txt`
- `symbol/signatures.txt`

| Tool | Bytes | Cut | Time | Rating |
| --- | ---: | ---: | ---: | ---: |
| input | 6864 | - | - | - |
| content-view | 6205 | 9.6% | 1.518 ms | 8.3/10 |
| applyMinification | 5431 | 20.9% | 1.602 ms | 8.3/10 |
| sync minify | 5431 | 20.9% | 1.616 ms | 8.3/10 |
| async minify | 5431 | 20.9% | 1.68 ms | 8.3/10 |
| symbols | 5765 | 16% | 11.273 ms | 6.5/10 |

## Agent Understanding

Measured from `standard` minified output.

| Component | Score |
| --- | ---: |
| syntax anchors | 10/10 (3/3) |
| delimiter structure | 10/10 |
| output health | 9/10 |
| context budget | 7/10 |
| symbol context | 10/10 |
| signals passed | 6/6 |

## Agent Observation By Output Level

Ratings are computed from the actual raw, standard, minify, and symbol outputs
for this language sample.

| Level | Bytes | Cut | Agent observation | Syntax anchors | Structure |
| --- | ---: | ---: | ---: | ---: | ---: |
| none | 6864 | 0% | 10/10 excellent | 10/10 | 10/10 |
| standard | 6205 | 9.6% | 9.5/10 excellent | 10/10 | 10/10 |
| minify | 5431 | 20.9% | 9.6/10 excellent | 10/10 | 10/10 |
| symbols | 5765 | 16% | 9.6/10 excellent | 10/10 | 9.8/10 |

## Notes

- engine-backed or parser-backed path.

## Before Excerpt

```js
/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 *
 * @flow
 */

import type {Dispatcher} from 'react-reconciler/src/ReactInternalTypes';
import type {
  ReactContext,
  StartTransitionOptions,
  Usable,
  Awaited,
} from 'shared/ReactTypes';
import {REACT_CONSUMER_TYPE} from 'shared/ReactSymbols';

import ReactSharedInternals from 'shared/ReactSharedInternals';

type BasicStateAction<S> = (S => S) | S;
type Dispatch<A> = A => void;

function resolveDispatcher() {
  const dispatcher = ReactSharedInternals.H;
  if (__DEV__) {
    if (dispatcher === null) {
      console.error(
        'Invalid hook call. Hooks can only be called inside of the body of a function component. This could happen for' +
          ' one of the following reasons:\n' +
          '1. You might have mismatching versions of React and the renderer (such as React DOM)\n' +
          '2. You might be breaking the Rules of Hooks\n' +
          '3. You might have more than one copy of React in the same app\n' +
          'See https://react.dev/link/invalid-hook-call for tips about how to debug and fix th

... [truncated 5064 chars] ...

e[not-a-function] This is unstable, thus optional
  return dispatcher.useEffectEvent(callback);
}

export function useOptimistic<S, A>(
  passthrough: S,
  reducer: ?(S, A) => S,
): [S, (A) => void] {
  const dispatcher = resolveDispatcher();
  return dispatcher.useOptimistic(passthrough, reducer);
}

export function useActionState<S, P>(
  action: (Awaited<S>, P) => S,
  initialState: Awaited<S>,
  permalink?: string,
): [Awaited<S>, (P) => void, boolean] {
  const dispatcher = resolveDispatcher();
  return dispatcher.useActionState(action, initialState, permalink);
}

```

## Content-View Excerpt

```js
import type {Dispatcher} from 'react-reconciler/src/ReactInternalTypes';
import type {
  ReactContext,
  StartTransitionOptions,
  Usable,
  Awaited,
} from 'shared/ReactTypes';
import {REACT_CONSUMER_TYPE} from 'shared/ReactSymbols';

import ReactSharedInternals from 'shared/ReactSharedInternals';

type BasicStateAction<S> = (S => S) | S;
type Dispatch<A> = A => void;

function resolveDispatcher() {
  const dispatcher = ReactSharedInternals.H;
  if (__DEV__) {
    if (dispatcher === null) {
      console.error(
        'Invalid hook call. Hooks can only be called inside of the body of a function component. This could happen for' +
          ' one of the following reasons:\n' +
          '1. You might have mismatching versions of React and the renderer (such as React DOM)\n' +
          '2. You might be breaking the Rules of Hooks\n' +
          '3. You might have more than one copy of React in the same app\n' +
          'See https://react.dev/link/invalid-hook-call for tips about how to debug and fix this problem.',
      );
    }
  }

  return dispatcher as any as Dispatcher;
}

export function getCacheForType<T>(resourceType: () => T): T {
  const dispatcher = ReactSharedInternals.A;
  if (!dispatcher

... [truncated 4405 chars] ...


): F {
  const dispatcher = resolveDispatcher();

  return dispatcher.useEffectEvent(callback);
}

export function useOptimistic<S, A>(
  passthrough: S,
  reducer: ?(S, A) => S,
): [S, (A) => void] {
  const dispatcher = resolveDispatcher();
  return dispatcher.useOptimistic(passthrough, reducer);
}

export function useActionState<S, P>(
  action: (Awaited<S>, P) => S,
  initialState: Awaited<S>,
  permalink?: string,
): [Awaited<S>, (P) => void, boolean] {
  const dispatcher = resolveDispatcher();
  return dispatcher.useActionState(action, initialState, permalink);
}
```

## Apply Minification Excerpt

```js
import type{Dispatcher}from 'react-reconciler/src/ReactInternalTypes';import type{ReactContext,StartTransitionOptions,Usable,Awaited,}from 'shared/ReactTypes';import{REACT_CONSUMER_TYPE}from 'shared/ReactSymbols';import ReactSharedInternals from 'shared/ReactSharedInternals';type BasicStateAction<S> =(S => S)| S;type Dispatch<A> = A => void;function resolveDispatcher(){const dispatcher = ReactSharedInternals.H;if(__DEV__){if(dispatcher === null){console.error('Invalid hook call. Hooks can only be called inside of the body of a function component. This could happen for' + ' one of the following reasons:\n' + '1. You might have mismatching versions of React and the renderer(such as React DOM)\n' + '2. You might be breaking the Rules of Hooks\n' + '3. You might have more than one copy of React in the same app\n' + 'See https://react.dev/link/invalid-hook-call for tips about how to debug and fix this problem.',);}}return dispatcher as any as Dispatcher;}export function getCacheForType<T>(resourceType:()=> T): T{const dispatcher = ReactSharedInternals.A;if(!dispatcher){return resourceType();}return dispatcher.getCacheForType(resourceType);}export function useContext<T>(Context: ReactContext<T>): T{const dispat

... [truncated 3631 chars] ...

n useEffectEvent<Args,F:(...Array<Args>)=> mixed>(callback: F,): F{const dispatcher = resolveDispatcher();return dispatcher.useEffectEvent(callback);}export function useOptimistic<S,A>(passthrough: S,reducer: ?(S,A)=> S,): [S,(A)=> void]{const dispatcher = resolveDispatcher();return dispatcher.useOptimistic(passthrough,reducer);}export function useActionState<S,P>(action:(Awaited<S>,P)=> S,initialState: Awaited<S>,permalink?: string,): [Awaited<S>,(P)=> void,boolean]{const dispatcher = resolveDispatcher();return dispatcher.useActionState(action,initialState,permalink);}
```

## Sync Minify Excerpt

```js
import type{Dispatcher}from 'react-reconciler/src/ReactInternalTypes';import type{ReactContext,StartTransitionOptions,Usable,Awaited,}from 'shared/ReactTypes';import{REACT_CONSUMER_TYPE}from 'shared/ReactSymbols';import ReactSharedInternals from 'shared/ReactSharedInternals';type BasicStateAction<S> =(S => S)| S;type Dispatch<A> = A => void;function resolveDispatcher(){const dispatcher = ReactSharedInternals.H;if(__DEV__){if(dispatcher === null){console.error('Invalid hook call. Hooks can only be called inside of the body of a function component. This could happen for' + ' one of the following reasons:\n' + '1. You might have mismatching versions of React and the renderer(such as React DOM)\n' + '2. You might be breaking the Rules of Hooks\n' + '3. You might have more than one copy of React in the same app\n' + 'See https://react.dev/link/invalid-hook-call for tips about how to debug and fix this problem.',);}}return dispatcher as any as Dispatcher;}export function getCacheForType<T>(resourceType:()=> T): T{const dispatcher = ReactSharedInternals.A;if(!dispatcher){return resourceType();}return dispatcher.getCacheForType(resourceType);}export function useContext<T>(Context: ReactContext<T>): T{const dispat

... [truncated 3631 chars] ...

n useEffectEvent<Args,F:(...Array<Args>)=> mixed>(callback: F,): F{const dispatcher = resolveDispatcher();return dispatcher.useEffectEvent(callback);}export function useOptimistic<S,A>(passthrough: S,reducer: ?(S,A)=> S,): [S,(A)=> void]{const dispatcher = resolveDispatcher();return dispatcher.useOptimistic(passthrough,reducer);}export function useActionState<S,P>(action:(Awaited<S>,P)=> S,initialState: Awaited<S>,permalink?: string,): [Awaited<S>,(P)=> void,boolean]{const dispatcher = resolveDispatcher();return dispatcher.useActionState(action,initialState,permalink);}
```

## Async Minify Excerpt

```js
import type{Dispatcher}from 'react-reconciler/src/ReactInternalTypes';import type{ReactContext,StartTransitionOptions,Usable,Awaited,}from 'shared/ReactTypes';import{REACT_CONSUMER_TYPE}from 'shared/ReactSymbols';import ReactSharedInternals from 'shared/ReactSharedInternals';type BasicStateAction<S> =(S => S)| S;type Dispatch<A> = A => void;function resolveDispatcher(){const dispatcher = ReactSharedInternals.H;if(__DEV__){if(dispatcher === null){console.error('Invalid hook call. Hooks can only be called inside of the body of a function component. This could happen for' + ' one of the following reasons:\n' + '1. You might have mismatching versions of React and the renderer(such as React DOM)\n' + '2. You might be breaking the Rules of Hooks\n' + '3. You might have more than one copy of React in the same app\n' + 'See https://react.dev/link/invalid-hook-call for tips about how to debug and fix this problem.',);}}return dispatcher as any as Dispatcher;}export function getCacheForType<T>(resourceType:()=> T): T{const dispatcher = ReactSharedInternals.A;if(!dispatcher){return resourceType();}return dispatcher.getCacheForType(resourceType);}export function useContext<T>(Context: ReactContext<T>): T{const dispat

... [truncated 3631 chars] ...

n useEffectEvent<Args,F:(...Array<Args>)=> mixed>(callback: F,): F{const dispatcher = resolveDispatcher();return dispatcher.useEffectEvent(callback);}export function useOptimistic<S,A>(passthrough: S,reducer: ?(S,A)=> S,): [S,(A)=> void]{const dispatcher = resolveDispatcher();return dispatcher.useOptimistic(passthrough,reducer);}export function useActionState<S,P>(action:(Awaited<S>,P)=> S,initialState: Awaited<S>,permalink?: string,): [Awaited<S>,(P)=> void,boolean]{const dispatcher = resolveDispatcher();return dispatcher.useActionState(action,initialState,permalink);}
```

## Symbols

```txt
 10| import type {Dispatcher} from 'react-reconciler/src/ReactInternalTypes';
 11| import type {
 12|   ReactContext,
 13|   StartTransitionOptions,
 14|   Usable,
 15|   Awaited,
 16| } from 'shared/ReactTypes';
 17| import {REACT_CONSUMER_TYPE} from 'shared/ReactSymbols';
 19| import ReactSharedInternals from 'shared/ReactSharedInternals';
 21| type BasicStateAction<S> = (S => S) | S;
 22| type Dispatch<A> = A => void;
 24| function resolveDispatcher() {
 44| export function getCacheForType<T>(resourceType: () => T): T {
 45|   const dispatcher = ReactSharedInternals.A;
 46|   if (!dispatcher) {
 48|     return resourceType();
 49|   }
 50|   return dispatcher.getCacheForType(resourceType);
 51| }
 53| export function useContext<T>(Context: ReactContext<T>): T {
 54|   const dispatcher = resolveDispatcher();
 55|   if (__DEV__) {
 56|     if (Context.$$typeof === REACT_CONSUMER_TYPE) {
 57|       console.error(
 58|         'Calling useContext(Context.Consumer) is not supported and will cause bugs. ' +
 59|           'Did you mean to call useContext(Context) instead?',
 60|       );
 61|     }
 62|   }
 63|   return dispatcher.useContext(Context);
 64| }
 66| export function useState<S>(
 67|   initialState: (() => S) | S,
 68| ): [S, Dispatch<BasicStateAction<S>>] {
 69|   const dispatcher = resolveDispatcher();
 70|   return dispatcher.useState(initialState);
 71| }
 73| export function useReducer<S, I, A>(
 74|   reducer: (S, A) => S,
 75|   initialArg: I,
 76|   init?: I => S,
 77| ): [S, Dispatch<A>] {
 78|   const dispatcher = resolveDispatcher();
 79|   return dispatcher.useReducer(reducer, initialArg, init);
 80| }
 82| export function useRef<T>(initialValue: T): {current: T} {
 83|   const dispatcher = resolveDispatcher();
 84|

... [truncated 3165 chars] ...

esolveDispatcher();
215|   return dispatcher.useMemoCache(size);
216| }
218| export function useEffectEvent<Args, F: (...Array<Args>) => mixed>(
219|   callback: F,
220| ): F {
221|   const dispatcher = resolveDispatcher();
223|   return dispatcher.useEffectEvent(callback);
224| }
226| export function useOptimistic<S, A>(
227|   passthrough: S,
228|   reducer: ?(S, A) => S,
229| ): [S, (A) => void] {
230|   const dispatcher = resolveDispatcher();
231|   return dispatcher.useOptimistic(passthrough, reducer);
232| }
234| export function useActionState<S, P>(
235|   action: (Awaited<S>, P) => S,
236|   initialState: Awaited<S>,
237|   permalink?: string,
238| ): [Awaited<S>, (P) => void, boolean] {
239|   const dispatcher = resolveDispatcher();
240|   return dispatcher.useActionState(action, initialState, permalink);
241| }
```
