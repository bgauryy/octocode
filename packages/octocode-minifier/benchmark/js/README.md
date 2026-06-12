# JavaScript (.js)

Source sample: `js/00-react-hooks.js`

Strategy: `terser`

Agent rating: **8.5/10 (strong)**

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
| content-view | 6205 | 9.6% | 3.086 ms | 8.3/10 |
| applyMinification | 5383 | 21.6% | 3.821 ms | 8.3/10 |
| sync minify | 5383 | 21.6% | 3.995 ms | 8.3/10 |
| async minify | 5383 | 21.6% | 1.27 ms | 8.3/10 |
| symbols | 2657 | 61.3% | 15.143 ms | 9/10 |

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
| minify | 5383 | 21.6% | 9.6/10 excellent | 10/10 | 10/10 |
| symbols | 2657 | 61.3% | 9.3/10 excellent | 10/10 | 7.7/10 |

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
import type{Dispatcher}from 'react-reconciler/src/ReactInternalTypes';import type{ReactContext,StartTransitionOptions,Usable,Awaited,}from 'shared/ReactTypes';import{REACT_CONSUMER_TYPE}from 'shared/ReactSymbols';import ReactSharedInternals from 'shared/ReactSharedInternals';type BasicStateAction<S> =(S => S)| S;type Dispatch<A> = A => void;function resolveDispatcher(){const dispatcher = ReactSharedInternals.H;if(__DEV__){if(dispatcher === null){console.error('Invalid hook call. Hooks can only be called inside of the body of a function component. This could happen for' + ' one of the following reasons:\n' + '1. You might have mismatching versions of React and the renderer(such as React DOM)\n' + '2. You might be breaking the Rules of Hooks\n' + '3. You might have more than one copy of React in the same app\n' + 'See https://react.dev/link/invalid-hook-call for tips about how to debug and fix this problem.',);}}return dispatcher as any as Dispatcher;}export function getCacheForType<T>(resourceType:()=> T):T{const dispatcher = ReactSharedInternals.A;if(!dispatcher){return resourceType();}return dispatcher.getCacheForType(resourceType);}export function useContext<T>(Context:ReactContext<T>):T{const dispatche

... [truncated 3583 chars] ...

 function useEffectEvent<Args,F:(...Array<Args>)=> mixed>(callback:F,):F{const dispatcher = resolveDispatcher();return dispatcher.useEffectEvent(callback);}export function useOptimistic<S,A>(passthrough:S,reducer:?(S,A)=> S,):[S,(A)=> void]{const dispatcher = resolveDispatcher();return dispatcher.useOptimistic(passthrough,reducer);}export function useActionState<S,P>(action:(Awaited<S>,P)=> S,initialState:Awaited<S>,permalink?:string,):[Awaited<S>,(P)=> void,boolean]{const dispatcher = resolveDispatcher();return dispatcher.useActionState(action,initialState,permalink);}
```

## Sync Minify Excerpt

```js
import type{Dispatcher}from 'react-reconciler/src/ReactInternalTypes';import type{ReactContext,StartTransitionOptions,Usable,Awaited,}from 'shared/ReactTypes';import{REACT_CONSUMER_TYPE}from 'shared/ReactSymbols';import ReactSharedInternals from 'shared/ReactSharedInternals';type BasicStateAction<S> =(S => S)| S;type Dispatch<A> = A => void;function resolveDispatcher(){const dispatcher = ReactSharedInternals.H;if(__DEV__){if(dispatcher === null){console.error('Invalid hook call. Hooks can only be called inside of the body of a function component. This could happen for' + ' one of the following reasons:\n' + '1. You might have mismatching versions of React and the renderer(such as React DOM)\n' + '2. You might be breaking the Rules of Hooks\n' + '3. You might have more than one copy of React in the same app\n' + 'See https://react.dev/link/invalid-hook-call for tips about how to debug and fix this problem.',);}}return dispatcher as any as Dispatcher;}export function getCacheForType<T>(resourceType:()=> T):T{const dispatcher = ReactSharedInternals.A;if(!dispatcher){return resourceType();}return dispatcher.getCacheForType(resourceType);}export function useContext<T>(Context:ReactContext<T>):T{const dispatche

... [truncated 3583 chars] ...

 function useEffectEvent<Args,F:(...Array<Args>)=> mixed>(callback:F,):F{const dispatcher = resolveDispatcher();return dispatcher.useEffectEvent(callback);}export function useOptimistic<S,A>(passthrough:S,reducer:?(S,A)=> S,):[S,(A)=> void]{const dispatcher = resolveDispatcher();return dispatcher.useOptimistic(passthrough,reducer);}export function useActionState<S,P>(action:(Awaited<S>,P)=> S,initialState:Awaited<S>,permalink?:string,):[Awaited<S>,(P)=> void,boolean]{const dispatcher = resolveDispatcher();return dispatcher.useActionState(action,initialState,permalink);}
```

## Async Minify Excerpt

```js
import type{Dispatcher}from 'react-reconciler/src/ReactInternalTypes';import type{ReactContext,StartTransitionOptions,Usable,Awaited,}from 'shared/ReactTypes';import{REACT_CONSUMER_TYPE}from 'shared/ReactSymbols';import ReactSharedInternals from 'shared/ReactSharedInternals';type BasicStateAction<S> =(S => S)| S;type Dispatch<A> = A => void;function resolveDispatcher(){const dispatcher = ReactSharedInternals.H;if(__DEV__){if(dispatcher === null){console.error('Invalid hook call. Hooks can only be called inside of the body of a function component. This could happen for' + ' one of the following reasons:\n' + '1. You might have mismatching versions of React and the renderer(such as React DOM)\n' + '2. You might be breaking the Rules of Hooks\n' + '3. You might have more than one copy of React in the same app\n' + 'See https://react.dev/link/invalid-hook-call for tips about how to debug and fix this problem.',);}}return dispatcher as any as Dispatcher;}export function getCacheForType<T>(resourceType:()=> T):T{const dispatcher = ReactSharedInternals.A;if(!dispatcher){return resourceType();}return dispatcher.getCacheForType(resourceType);}export function useContext<T>(Context:ReactContext<T>):T{const dispatche

... [truncated 3583 chars] ...

 function useEffectEvent<Args,F:(...Array<Args>)=> mixed>(callback:F,):F{const dispatcher = resolveDispatcher();return dispatcher.useEffectEvent(callback);}export function useOptimistic<S,A>(passthrough:S,reducer:?(S,A)=> S,):[S,(A)=> void]{const dispatcher = resolveDispatcher();return dispatcher.useOptimistic(passthrough,reducer);}export function useActionState<S,P>(action:(Awaited<S>,P)=> S,initialState:Awaited<S>,permalink?:string,):[Awaited<S>,(P)=> void,boolean]{const dispatcher = resolveDispatcher();return dispatcher.useActionState(action,initialState,permalink);}
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
 53| export function useContext<T>(Context: ReactContext<T>): T {
 66| export function useState<S>(
 67|   initialState: (() => S) | S,
 68| ): [S, Dispatch<BasicStateAction<S>>] {
 73| export function useReducer<S, I, A>(
 74|   reducer: (S, A) => S,
 75|   initialArg: I,
 76|   init?: I => S,
 77| ): [S, Dispatch<A>] {
 82| export function useRef<T>(initialValue: T): {current: T} {
 87| export function useEffect(
 88|   create: () => (() => void) | void,
 89|   deps: Array<mixed> | void | null,
 90| ): void {
103| export function useInsertionEffect(
104|   create: () => (() => void) | void,
105|   deps: Array<mixed> | void | null,
106| ): void {
119| export function useLayoutEffect(
120|   create: () => (() => void) | void,
121|   deps: Array<mixed> | void | null,
122| ): void {
135| export function useCallback<T>(
136|   callback: T,
137|   deps: Array<mixed> | void | null,
138| ): T {
143| export function useMemo<T>(
144|   create: () => T,
145|   deps: Array<mixed> | void | null,
146| ): T {
151| export function useImperativeHandle<T>(
152|   ref: {current: T | null} | ((inst: T | null) => mixed) | null | void,
153|   create: () => T,
154|   deps: Array<mixed> | void | null,
155| ): void {
160| export function useD

... [truncated 57 chars] ...

e: T) => mixed,
163| ): void {
170| export function useTransition(): [
171|   boolean,
172|   (callback: () => void, options?: StartTransitionOptions) => void,
173| ] {
178| export function useDeferredValue<T>(value: T, initialValue?: T): T {
183| export function useId(): string {
188| export function useSyncExternalStore<T>(
189|   subscribe: (() => void) => () => void,
201| export function useCacheRefresh(): <T>(?() => T, ?T) => void {
207| export function use<T>(usable: Usable<T>): T {
212| export function useMemoCache(size: number): Array<mixed> {
218| export function useEffectEvent<Args, F: (...Array<Args>) => mixed>(
226| export function useOptimistic<S, A>(
227|   passthrough: S,
228|   reducer: ?(S, A) => S,
229| ): [S, (A) => void] {
234| export function useActionState<S, P>(
235|   action: (Awaited<S>, P) => S,
```
