# TypeScript (.ts)

Source sample: `ts/00-typescript-core.ts`

Strategy: `conservative`

Agent rating: **9/10 (excellent)**

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
| input | 92419 | - | - | - |
| content-view | 69160 | 25.2% | 17.796 ms | 9/10 |
| applyMinification | 69160 | 25.2% | 21.314 ms | 9/10 |
| sync minify | 69160 | 25.2% | 21.674 ms | 9/10 |
| async minify | 69160 | 25.2% | 21.866 ms | 9/10 |
| symbols | 28507 | 69.2% | 54.738 ms | 9/10 |

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
| none | 92419 | 0% | 10/10 excellent | 10/10 | 10/10 |
| standard | 69160 | 25.2% | 9.7/10 excellent | 10/10 | 10/10 |
| minify | 69160 | 25.2% | 9.7/10 excellent | 10/10 | 10/10 |
| symbols | 28507 | 69.2% | 9.3/10 excellent | 10/10 | 7.4/10 |

## Notes

- engine-backed or parser-backed path.

## Before Excerpt

```ts
import {
    CharacterCodes,
    Comparer,
    Comparison,
    Debug,
    EqualityComparer,
    MapLike,
    Queue,
    SortedArray,
    SortedReadonlyArray,
    TextSpan,
} from "./_namespaces/ts.js";

/* eslint-disable @typescript-eslint/prefer-for-of */

/** @internal */
export const emptyArray: never[] = [] as never[];
/** @internal */
export const emptyMap: ReadonlyMap<never, never> = new Map<never, never>();

/** @internal */
export function length(array: readonly any[] | undefined): number {
    return array !== undefined ? array.length : 0;
}

/**
 * Iterates through 'array' by index and performs the callback on each element of array until the callback
 * returns a truthy value, then returns that value.
 * If no such value is found, the callback is applied to each element of array and undefined is returned.
 *
 * @internal
 */
export function forEach<T, U>(array: readonly T[] | undefined, callback: (element: T, index: number) => U | undefined): U | undefined {
    if (array !== undefined) {
        for (let i = 0; i < array.length; i++) {
            const result = callback(array[i], i);
            if (result) {
                return result;
            }
 

... [truncated 90615 chars] ...

ay !== undefined) {
        const len = array.length;
        let index = 0;
        while (index < len && predicate(array[index])) {
            index++;
        }
        return array.slice(index) as Exclude<T, U>[];
    }
}

/** @internal */
export function isNodeLikeSystem(): boolean {
    // This is defined here rather than in sys.ts to prevent a cycle from its
    // use in performanceCore.ts.
    return typeof process !== "undefined"
        && !!process.nextTick
        && !(process as any).browser
        && typeof require !== "undefined";
}

```

## Content-View Excerpt

```ts
import{CharacterCodes,Comparer,Comparison,Debug,EqualityComparer,MapLike,Queue,SortedArray,SortedReadonlyArray,TextSpan}from"./_namespaces/ts.js";
/* eslint-disable @typescript-eslint/prefer-for-of */
/** @internal */
export const emptyArray:never[]=[] as never[];
/** @internal */
export const emptyMap:ReadonlyMap<never,never>=new Map;
/** @internal */
export function length(array:readonly any[]|undefined): number{return array===void 0?0:array.length}
/**
* Iterates through 'array' by index and performs the callback on each element of array until the callback
* returns a truthy value, then returns that value.
* If no such value is found, the callback is applied to each element of array and undefined is returned.
*
* @internal
*/
export function forEach<T,U>(array:readonly T[]|undefined,callback:(element:T,index:number)=>U|undefined): U|undefined{if(array!==void 0)for(let i=0;i<array.length;i++){let result=callback(array[i],i);if(result)return result}}
/**
* Like `forEach`, but iterates in reverse order.
*
* @internal
*/
export function forEachRight<T,U>(array:readonly T[]|undefined,callback:(element:T,index:number)=>U|undefined): U|undefined{if(array!==void 0)for(let i=array.length-1;i>=0;i--){let result=

... [truncated 67356 chars] ...

clude<T,U>[]|undefined;
/** @internal */
export function skipWhile<T,U extends T>(array:readonly T[]|undefined,predicate:(element:T)=>element is U): Exclude<T,U>[]|undefined{if(array!==void 0){let len=array.length,index=0;for(;index<len&&predicate(array[index]);)index++;return array.slice(index) as Exclude<T,U>[]}}
/** @internal */
export function isNodeLikeSystem(): boolean{
// This is defined here rather than in sys.ts to prevent a cycle from its
// use in performanceCore.ts.
return typeof process<`u`&&!!process.nextTick&&!(process as any).browser&&typeof require<`u`}
```

## Apply Minification Excerpt

```ts
import{CharacterCodes,Comparer,Comparison,Debug,EqualityComparer,MapLike,Queue,SortedArray,SortedReadonlyArray,TextSpan}from"./_namespaces/ts.js";
/* eslint-disable @typescript-eslint/prefer-for-of */
/** @internal */
export const emptyArray:never[]=[] as never[];
/** @internal */
export const emptyMap:ReadonlyMap<never,never>=new Map;
/** @internal */
export function length(array:readonly any[]|undefined): number{return array===void 0?0:array.length}
/**
* Iterates through 'array' by index and performs the callback on each element of array until the callback
* returns a truthy value, then returns that value.
* If no such value is found, the callback is applied to each element of array and undefined is returned.
*
* @internal
*/
export function forEach<T,U>(array:readonly T[]|undefined,callback:(element:T,index:number)=>U|undefined): U|undefined{if(array!==void 0)for(let i=0;i<array.length;i++){let result=callback(array[i],i);if(result)return result}}
/**
* Like `forEach`, but iterates in reverse order.
*
* @internal
*/
export function forEachRight<T,U>(array:readonly T[]|undefined,callback:(element:T,index:number)=>U|undefined): U|undefined{if(array!==void 0)for(let i=array.length-1;i>=0;i--){let result=

... [truncated 67356 chars] ...

clude<T,U>[]|undefined;
/** @internal */
export function skipWhile<T,U extends T>(array:readonly T[]|undefined,predicate:(element:T)=>element is U): Exclude<T,U>[]|undefined{if(array!==void 0){let len=array.length,index=0;for(;index<len&&predicate(array[index]);)index++;return array.slice(index) as Exclude<T,U>[]}}
/** @internal */
export function isNodeLikeSystem(): boolean{
// This is defined here rather than in sys.ts to prevent a cycle from its
// use in performanceCore.ts.
return typeof process<`u`&&!!process.nextTick&&!(process as any).browser&&typeof require<`u`}
```

## Sync Minify Excerpt

```ts
import{CharacterCodes,Comparer,Comparison,Debug,EqualityComparer,MapLike,Queue,SortedArray,SortedReadonlyArray,TextSpan}from"./_namespaces/ts.js";
/* eslint-disable @typescript-eslint/prefer-for-of */
/** @internal */
export const emptyArray:never[]=[] as never[];
/** @internal */
export const emptyMap:ReadonlyMap<never,never>=new Map;
/** @internal */
export function length(array:readonly any[]|undefined): number{return array===void 0?0:array.length}
/**
* Iterates through 'array' by index and performs the callback on each element of array until the callback
* returns a truthy value, then returns that value.
* If no such value is found, the callback is applied to each element of array and undefined is returned.
*
* @internal
*/
export function forEach<T,U>(array:readonly T[]|undefined,callback:(element:T,index:number)=>U|undefined): U|undefined{if(array!==void 0)for(let i=0;i<array.length;i++){let result=callback(array[i],i);if(result)return result}}
/**
* Like `forEach`, but iterates in reverse order.
*
* @internal
*/
export function forEachRight<T,U>(array:readonly T[]|undefined,callback:(element:T,index:number)=>U|undefined): U|undefined{if(array!==void 0)for(let i=array.length-1;i>=0;i--){let result=

... [truncated 67356 chars] ...

clude<T,U>[]|undefined;
/** @internal */
export function skipWhile<T,U extends T>(array:readonly T[]|undefined,predicate:(element:T)=>element is U): Exclude<T,U>[]|undefined{if(array!==void 0){let len=array.length,index=0;for(;index<len&&predicate(array[index]);)index++;return array.slice(index) as Exclude<T,U>[]}}
/** @internal */
export function isNodeLikeSystem(): boolean{
// This is defined here rather than in sys.ts to prevent a cycle from its
// use in performanceCore.ts.
return typeof process<`u`&&!!process.nextTick&&!(process as any).browser&&typeof require<`u`}
```

## Async Minify Excerpt

```ts
import{CharacterCodes,Comparer,Comparison,Debug,EqualityComparer,MapLike,Queue,SortedArray,SortedReadonlyArray,TextSpan}from"./_namespaces/ts.js";
/* eslint-disable @typescript-eslint/prefer-for-of */
/** @internal */
export const emptyArray:never[]=[] as never[];
/** @internal */
export const emptyMap:ReadonlyMap<never,never>=new Map;
/** @internal */
export function length(array:readonly any[]|undefined): number{return array===void 0?0:array.length}
/**
* Iterates through 'array' by index and performs the callback on each element of array until the callback
* returns a truthy value, then returns that value.
* If no such value is found, the callback is applied to each element of array and undefined is returned.
*
* @internal
*/
export function forEach<T,U>(array:readonly T[]|undefined,callback:(element:T,index:number)=>U|undefined): U|undefined{if(array!==void 0)for(let i=0;i<array.length;i++){let result=callback(array[i],i);if(result)return result}}
/**
* Like `forEach`, but iterates in reverse order.
*
* @internal
*/
export function forEachRight<T,U>(array:readonly T[]|undefined,callback:(element:T,index:number)=>U|undefined): U|undefined{if(array!==void 0)for(let i=array.length-1;i>=0;i--){let result=

... [truncated 67356 chars] ...

clude<T,U>[]|undefined;
/** @internal */
export function skipWhile<T,U extends T>(array:readonly T[]|undefined,predicate:(element:T)=>element is U): Exclude<T,U>[]|undefined{if(array!==void 0){let len=array.length,index=0;for(;index<len&&predicate(array[index]);)index++;return array.slice(index) as Exclude<T,U>[]}}
/** @internal */
export function isNodeLikeSystem(): boolean{
// This is defined here rather than in sys.ts to prevent a cycle from its
// use in performanceCore.ts.
return typeof process<`u`&&!!process.nextTick&&!(process as any).browser&&typeof require<`u`}
```

## Symbols

```txt
   1| import {
   2|     CharacterCodes,
   3|     Comparer,
   4|     Comparison,
   5|     Debug,
   6|     EqualityComparer,
   7|     MapLike,
   8|     Queue,
   9|     SortedArray,
  10|     SortedReadonlyArray,
  11|     TextSpan,
  12| } from "./_namespaces/ts.js";
  17| export const emptyArray: never[] = [] as never[];
  19| export const emptyMap: ReadonlyMap<never, never> = new Map<never, never>();
  22| export function length(array: readonly any[] | undefined): number {
  33| export function forEach<T, U>(array: readonly T[] | undefined, callback: (element: T, index: number) => U | undefined): U | undefined {
  50| export function forEachRight<T, U>(array: readonly T[] | undefined, callback: (element: T, index: number) => U | undefined): U | undefined {
  67| export function firstDefined<T, U>(array: readonly T[] | undefined, callback: (element: T, index: number) => U | undefined): U | undefined {
  82| export function firstDefinedIterator<T, U>(iter: Iterable<T>, callback: (element: T) => U | undefined): U | undefined {
  93| export function reduceLeftIterator<T, U>(iterator: Iterable<T> | undefined, f: (memo: U, value: T, i: number) => U, initial: U): U {
 106| export function zipWith<T, U, V>(arrayA: readonly T[], arrayB: readonly U[], callback: (a: T, b: U, index: number) => V): V[] {
 121| export function intersperse<T>(input: T[], element: T): T[] {
 140| export function every<T, U extends T>(array: readonly T[], callback: (element: T, index: number) => element is U): array is readonly U[];
 142| export function every<T, U extends T>(array: readonly T[] | undefined, callback: (element: T, index: number) => element is U): array is readonly U[] | undefined;
 144| export function every<T>(array: readonly T[] | undefined, cal

... [truncated 25907 chars] ...

rray: readonly T[], predicate: (element: T) => element is U): U[];
2559| export function takeWhile<T, U extends T>(array: readonly T[] | undefined, predicate: (element: T) => element is U): U[] | undefined;
2560| export function takeWhile<T, U extends T>(array: readonly T[] | undefined, predicate: (element: T) => element is U): U[] | undefined {
2572| export function skipWhile<T, U extends T>(array: readonly T[], predicate: (element: T) => element is U): Exclude<T, U>[];
2574| export function skipWhile<T, U extends T>(array: readonly T[] | undefined, predicate: (element: T) => element is U): Exclude<T, U>[] | undefined;
2576| export function skipWhile<T, U extends T>(array: readonly T[] | undefined, predicate: (element: T) => element is U): Exclude<T, U>[] | undefined {
2588| export function isNodeLikeSystem(): boolean {
```
