# TSX (.tsx)

Source sample: `tsx/00-next-app-router.tsx`

Strategy: `conservative`

Agent rating: **9.7/10 (excellent)**

Agent understanding from minified output: **9.7/10 (excellent)**

Artifacts:

- `raw/source.excerpt.txt`
- `minified/content-view.excerpt.txt`
- `minified/apply-minification.excerpt.txt`
- `minified/minify-content-sync.excerpt.txt`
- `minified/minify-content-async.excerpt.txt`
- `symbol/signatures.txt`

| Tool | Bytes | Cut | Time | Rating |
| --- | ---: | ---: | ---: | ---: |
| input | 23197 | - | - | - |
| content-view | 15062 | 35.1% | 4.46 ms | 9.5/10 |
| applyMinification | 12161 | 47.6% | 4.597 ms | 9.5/10 |
| sync minify | 12161 | 47.6% | 4.394 ms | 9.5/10 |
| async minify | 12161 | 47.6% | 4.84 ms | 9.5/10 |
| symbols | 3742 | 83.9% | 22.543 ms | 10/10 |

## Agent Understanding

Measured from `standard` minified output.

| Component | Score |
| --- | ---: |
| syntax anchors | 10/10 (3/3) |
| delimiter structure | 10/10 |
| output health | 9/10 |
| context budget | 9/10 |
| symbol context | 10/10 |
| signals passed | 6/6 |

## Agent Observation By Output Level

Ratings are computed from the actual raw, standard, minify, and symbol outputs
for this language sample.

| Level | Bytes | Cut | Agent observation | Syntax anchors | Structure |
| --- | ---: | ---: | ---: | ---: | ---: |
| none | 23197 | 0% | 10/10 excellent | 10/10 | 10/10 |
| standard | 15062 | 35.1% | 9.7/10 excellent | 10/10 | 10/10 |
| minify | 12161 | 47.6% | 9.8/10 excellent | 10/10 | 10/10 |
| symbols | 3742 | 83.9% | 9.5/10 excellent | 10/10 | 9.6/10 |

## Notes

- engine-backed or parser-backed path.

## Before Excerpt

```tsx
import React, {
  useEffect,
  useMemo,
  startTransition,
  useInsertionEffect,
  useDeferredValue,
} from 'react'
import {
  AppRouterContext,
  LayoutRouterContext,
  GlobalLayoutRouterContext,
} from '../../shared/lib/app-router-context.shared-runtime'
import type { CacheNode } from '../../shared/lib/app-router-types'
import { ACTION_RESTORE } from './router-reducer/router-reducer-types'
import type {
  AppHistoryState,
  AppRouterState,
} from './router-reducer/router-reducer-types'
import { createHrefFromUrl } from './router-reducer/create-href-from-url'
import {
  SearchParamsContext,
  PathnameContext,
  PathParamsContext,
  NavigationPromisesContext,
  type NavigationPromises,
} from '../../shared/lib/hooks-client-context.shared-runtime'
import { dispatchAppRouterAction, useActionQueue } from './use-action-queue'
import { setLastCommittedTree } from './router-reducer/reducers/committed-state'
import { AppRouterAnnouncer } from './app-router-announcer'
import { RedirectBoundary } from './redirect-boundary'
import { findHeadInCache } from './router-reducer/reducers/find-head-in-cache'
import { unresolvedThenable } from './unresolved-thenable'
import { removeBasePath } from '../remove-base-path'
imp

... [truncated 21397 chars] ...

) => forceUpdate((c) => c + 1)
    runtimeStyleChanged.add(changed)
    if (renderedStylesSize !== runtimeStyles.size) {
      changed()
    }
    return () => {
      runtimeStyleChanged.delete(changed)
    }
  }, [renderedStylesSize, forceUpdate])

  const query = getAssetTokenQuery()
  return [...(runtimeStyles || [])].map((href, i) => (
    <link
      key={i}
      rel="stylesheet"
      href={`${href}${query}`}
      // @ts-ignore
      precedence="next"
      // TODO figure out crossOrigin and nonce
      // crossOrigin={TODO}
      // nonce={TODO}
    />
  ))
}

```

## Content-View Excerpt

```tsx
import React, {
  useEffect,
  useMemo,
  startTransition,
  useInsertionEffect,
  useDeferredValue,
} from 'react'
import {
  AppRouterContext,
  LayoutRouterContext,
  GlobalLayoutRouterContext,
} from '../../shared/lib/app-router-context.shared-runtime'
import type { CacheNode } from '../../shared/lib/app-router-types'
import { ACTION_RESTORE } from './router-reducer/router-reducer-types'
import type {
  AppHistoryState,
  AppRouterState,
} from './router-reducer/router-reducer-types'
import { createHrefFromUrl } from './router-reducer/create-href-from-url'
import {
  SearchParamsContext,
  PathnameContext,
  PathParamsContext,
  NavigationPromisesContext,
  type NavigationPromises,
} from '../../shared/lib/hooks-client-context.shared-runtime'
import { dispatchAppRouterAction, useActionQueue } from './use-action-queue'
import { setLastCommittedTree } from './router-reducer/reducers/committed-state'
import { AppRouterAnnouncer } from './app-router-announcer'
import { RedirectBoundary } from './redirect-boundary'
import { findHeadInCache } from './router-reducer/reducers/find-head-in-cache'
import { unresolvedThenable } from './unresolved-thenable'
import { removeBasePath } from '../remove-base-path'
imp

... [truncated 13262 chars] ...

meStyles?.size ?? 0
  useEffect(() => {
    if (!runtimeStyles || !runtimeStyleChanged) return
    const changed = () => forceUpdate((c) => c + 1)
    runtimeStyleChanged.add(changed)
    if (renderedStylesSize !== runtimeStyles.size) {
      changed()
    }
    return () => {
      runtimeStyleChanged.delete(changed)
    }
  }, [renderedStylesSize, forceUpdate])

  const query = getAssetTokenQuery()
  return [...(runtimeStyles || [])].map((href, i) => (
    <link
      key={i}
      rel="stylesheet"
      href={`${href}${query}`}

      precedence="next"

    />
  ))
}
```

## Apply Minification Excerpt

```tsx
import React,{useEffect,useMemo,startTransition,useInsertionEffect,useDeferredValue,}from 'react' import{AppRouterContext,LayoutRouterContext,GlobalLayoutRouterContext,}from '../../shared/lib/app-router-context.shared-runtime' import type{CacheNode}from '../../shared/lib/app-router-types' import{ACTION_RESTORE}from './router-reducer/router-reducer-types' import type{AppHistoryState,AppRouterState,}from './router-reducer/router-reducer-types' import{createHrefFromUrl}from './router-reducer/create-href-from-url' import{SearchParamsContext,PathnameContext,PathParamsContext,NavigationPromisesContext,type NavigationPromises,}from '../../shared/lib/hooks-client-context.shared-runtime' import{dispatchAppRouterAction,useActionQueue}from './use-action-queue' import{setLastCommittedTree}from './router-reducer/reducers/committed-state' import{AppRouterAnnouncer}from './app-router-announcer' import{RedirectBoundary}from './redirect-boundary' import{findHeadInCache}from './router-reducer/reducers/find-head-in-cache' import{unresolvedThenable}from './unresolved-thenable' import{removeBasePath}from '../remove-base-path' import{hasBasePath}from '../has-base-path' import{extractSourcePageFromFlightRouterState,getSelectedP

... [truncated 10361 chars] ...

resolve()}}function RuntimeStylesForWebpack(){const [,forceUpdate] = React.useState(0)const renderedStylesSize = runtimeStyles?.size ?? 0 useEffect(()=>{if(!runtimeStyles || !runtimeStyleChanged)return const changed =()=> forceUpdate((c)=> c + 1)runtimeStyleChanged.add(changed)if(renderedStylesSize !== runtimeStyles.size){changed()}return()=>{runtimeStyleChanged.delete(changed)}},[renderedStylesSize,forceUpdate])const query = getAssetTokenQuery()return [...(runtimeStyles || [])].map((href,i)=>(<link key={i}rel="stylesheet" href={`${href}${query}`}precedence="next" />))}
```

## Sync Minify Excerpt

```tsx
import React,{useEffect,useMemo,startTransition,useInsertionEffect,useDeferredValue,}from 'react' import{AppRouterContext,LayoutRouterContext,GlobalLayoutRouterContext,}from '../../shared/lib/app-router-context.shared-runtime' import type{CacheNode}from '../../shared/lib/app-router-types' import{ACTION_RESTORE}from './router-reducer/router-reducer-types' import type{AppHistoryState,AppRouterState,}from './router-reducer/router-reducer-types' import{createHrefFromUrl}from './router-reducer/create-href-from-url' import{SearchParamsContext,PathnameContext,PathParamsContext,NavigationPromisesContext,type NavigationPromises,}from '../../shared/lib/hooks-client-context.shared-runtime' import{dispatchAppRouterAction,useActionQueue}from './use-action-queue' import{setLastCommittedTree}from './router-reducer/reducers/committed-state' import{AppRouterAnnouncer}from './app-router-announcer' import{RedirectBoundary}from './redirect-boundary' import{findHeadInCache}from './router-reducer/reducers/find-head-in-cache' import{unresolvedThenable}from './unresolved-thenable' import{removeBasePath}from '../remove-base-path' import{hasBasePath}from '../has-base-path' import{extractSourcePageFromFlightRouterState,getSelectedP

... [truncated 10361 chars] ...

resolve()}}function RuntimeStylesForWebpack(){const [,forceUpdate] = React.useState(0)const renderedStylesSize = runtimeStyles?.size ?? 0 useEffect(()=>{if(!runtimeStyles || !runtimeStyleChanged)return const changed =()=> forceUpdate((c)=> c + 1)runtimeStyleChanged.add(changed)if(renderedStylesSize !== runtimeStyles.size){changed()}return()=>{runtimeStyleChanged.delete(changed)}},[renderedStylesSize,forceUpdate])const query = getAssetTokenQuery()return [...(runtimeStyles || [])].map((href,i)=>(<link key={i}rel="stylesheet" href={`${href}${query}`}precedence="next" />))}
```

## Async Minify Excerpt

```tsx
import React,{useEffect,useMemo,startTransition,useInsertionEffect,useDeferredValue,}from 'react' import{AppRouterContext,LayoutRouterContext,GlobalLayoutRouterContext,}from '../../shared/lib/app-router-context.shared-runtime' import type{CacheNode}from '../../shared/lib/app-router-types' import{ACTION_RESTORE}from './router-reducer/router-reducer-types' import type{AppHistoryState,AppRouterState,}from './router-reducer/router-reducer-types' import{createHrefFromUrl}from './router-reducer/create-href-from-url' import{SearchParamsContext,PathnameContext,PathParamsContext,NavigationPromisesContext,type NavigationPromises,}from '../../shared/lib/hooks-client-context.shared-runtime' import{dispatchAppRouterAction,useActionQueue}from './use-action-queue' import{setLastCommittedTree}from './router-reducer/reducers/committed-state' import{AppRouterAnnouncer}from './app-router-announcer' import{RedirectBoundary}from './redirect-boundary' import{findHeadInCache}from './router-reducer/reducers/find-head-in-cache' import{unresolvedThenable}from './unresolved-thenable' import{removeBasePath}from '../remove-base-path' import{hasBasePath}from '../has-base-path' import{extractSourcePageFromFlightRouterState,getSelectedP

... [truncated 10361 chars] ...

resolve()}}function RuntimeStylesForWebpack(){const [,forceUpdate] = React.useState(0)const renderedStylesSize = runtimeStyles?.size ?? 0 useEffect(()=>{if(!runtimeStyles || !runtimeStyleChanged)return const changed =()=> forceUpdate((c)=> c + 1)runtimeStyleChanged.add(changed)if(renderedStylesSize !== runtimeStyles.size){changed()}return()=>{runtimeStyleChanged.delete(changed)}},[renderedStylesSize,forceUpdate])const query = getAssetTokenQuery()return [...(runtimeStyles || [])].map((href,i)=>(<link key={i}rel="stylesheet" href={`${href}${query}`}precedence="next" />))}
```

## Symbols

```txt
  1| import React, {
  2|   useEffect,
  3|   useMemo,
  4|   startTransition,
  5|   useInsertionEffect,
  6|   useDeferredValue,
  7| } from 'react'
  8| import {
  9|   AppRouterContext,
 10|   LayoutRouterContext,
 11|   GlobalLayoutRouterContext,
 12| } from '../../shared/lib/app-router-context.shared-runtime'
 13| import type { CacheNode } from '../../shared/lib/app-router-types'
 14| import { ACTION_RESTORE } from './router-reducer/router-reducer-types'
 15| import type {
 16|   AppHistoryState,
 17|   AppRouterState,
 18| } from './router-reducer/router-reducer-types'
 19| import { createHrefFromUrl } from './router-reducer/create-href-from-url'
 20| import {
 21|   SearchParamsContext,
 22|   PathnameContext,
 23|   PathParamsContext,
 24|   NavigationPromisesContext,
 25|   type NavigationPromises,
 26| } from '../../shared/lib/hooks-client-context.shared-runtime'
 27| import { dispatchAppRouterAction, useActionQueue } from './use-action-queue'
 28| import { setLastCommittedTree } from './router-reducer/reducers/committed-state'
 29| import { AppRouterAnnouncer } from './app-router-announcer'
 30| import { RedirectBoundary } from './redirect-boundary'
 31| import { findHeadInCache } from './router-reducer/reducers/find-head-in-cache'
 32| import { unresolvedThenable } from './unresolved-thenable'
 33| import { removeBasePath } from '../remove-base-path'
 34| import { hasBasePath } from '../has-base-path'
 35| import {
 36|   extractSourcePageFromFlightRouterState,
 37|   getSelectedParams,
 38| } from './router-reducer/compute-changed-path'
 39| import { useNavFailureHandler } from './nav-failure-handler'
 40| import {
 41|   dispatchTraverseAction,
 42|   publicAppRouterInstance,
 43|   type AppRouterActionQueue,
 44|   type Gl

... [truncated 1142 chars] ...

obalError: GlobalErrorState
162|   webSocket: WebSocket | undefined
163|   staticIndicatorState: StaticIndicatorState | undefined
164| }) {
577| export default function AppRouter({
578|   actionQueue,
579|   globalErrorState,
580|   webSocket,
581|   staticIndicatorState,
582| }: {
583|   actionQueue: AppRouterActionQueue
584|   globalErrorState: GlobalErrorState
585|   webSocket?: WebSocket
586|   staticIndicatorState?: StaticIndicatorState
587| }) {
608| let runtimeStyles: Set<string> | undefined
609| let runtimeStyleChanged: Set<() => void> | undefined
610| if (!process.env.TURBOPACK && typeof window !== 'undefined') {
611|   runtimeStyles = new Set<string>()
612|   runtimeStyleChanged = new Set<() => void>()
614|   globalThis._N_E_STYLE_LOAD = function (href: string) {
625| }
627| function RuntimeStylesForWebpack() {
```
