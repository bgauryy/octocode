# TypeScript (.ts)

Source sample: `ts/00-typescript-core.ts`

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
| input | 92419 | - | - | - |
| content-view | 65054 | 29.6% | 50.511 ms | 10/10 |
| applyMinification | 29680 | 67.9% | 338.707 ms | 10/10 |
| sync minify | 29680 | 67.9% | 237.852 ms | 10/10 |
| async minify | 29680 | 67.9% | 248.05 ms | 10/10 |
| symbols | 28537 | 69.1% | 33.581 ms | 9/10 |

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
| standard | 65054 | 29.6% | 9.7/10 excellent | 10/10 | 10/10 |
| minify | 29680 | 67.9% | 9.8/10 excellent | 10/10 | 10/10 |
| symbols | 28537 | 69.1% | 9.3/10 excellent | 10/10 | 7.5/10 |

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

export const emptyArray: never[] = [] as never[];

export const emptyMap: ReadonlyMap<never, never> = new Map<never, never>();

export function length(array: readonly any[] | undefined): number {
    return array !== undefined ? array.length : 0;
}

export function forEach<T, U>(array: readonly T[] | undefined, callback: (element: T, index: number) => U | undefined): U | undefined {
    if (array !== undefined) {
        for (let i = 0; i < array.length; i++) {
            const result = callback(array[i], i);
            if (result) {
                return result;
            }
        }
    }
    return undefined;
}

export function forEachRight<T, U>(array: readonly T[] | undefined, callback: (element: T, index: number) => U | undefined): U | undefined {
    if (array !== undefined) {
        for (let i = array.length - 1; i >= 0; i--) {
            const result = callback(array[i], i);
            if (result) {
                return result;
            }
        }
    }
    return undefined;
}

expor

... [truncated 63254 chars] ...

ction skipWhile<T, U extends T>(array: readonly T[] | undefined, predicate: (element: T) => element is U): Exclude<T, U>[] | undefined {
    if (array !== undefined) {
        const len = array.length;
        let index = 0;
        while (index < len && predicate(array[index])) {
            index++;
        }
        return array.slice(index) as Exclude<T, U>[];
    }
}

export function isNodeLikeSystem(): boolean {

    return typeof process !== "undefined"
        && !!process.nextTick
        && !(process as any).browser
        && typeof require !== "undefined";
}
```

## Apply Minification Excerpt

```ts
import{CharacterCodes,Comparison,Debug}from"./_namespaces/ts.js";export const emptyArray=[];export const emptyMap=new Map;export function length(array){return void 0!==array?array.length:0}export function forEach(array,callback){if(void 0!==array)for(let i=0;i<array.length;i++){const result=callback(array[i],i);if(result)return result}}export function forEachRight(array,callback){if(void 0!==array)for(let i=array.length-1;i>=0;i--){const result=callback(array[i],i);if(result)return result}}export function firstDefined(array,callback){if(void 0!==array)for(let i=0;i<array.length;i++){const result=callback(array[i],i);if(void 0!==result)return result}}export function firstDefinedIterator(iter,callback){for(const value of iter){const result=callback(value);if(void 0!==result)return result}}export function reduceLeftIterator(iterator,f,initial){let result=initial;if(iterator){let pos=0;for(const value of iterator)result=f(result,value,pos),pos++}return result}export function zipWith(arrayA,arrayB,callback){const result=[];Debug.assertEqual(arrayA.length,arrayB.length);for(let i=0;i<arrayA.length;i++)result.push(callback(arrayA[i],arrayB[i],i));return result}export function intersperse(input,element){if(input.

... [truncated 27880 chars] ...

x===arrays.length-1?result.push(inner):cartesianProductWorker(arrays,result,inner,index+1)}}export function takeWhile(array,predicate){if(void 0!==array){const len=array.length;let index=0;while(index<len&&predicate(array[index]))index++;return array.slice(0,index)}}export function skipWhile(array,predicate){if(void 0!==array){const len=array.length;let index=0;while(index<len&&predicate(array[index]))index++;return array.slice(index)}}export function isNodeLikeSystem(){return"undefined"!=typeof process&&!!process.nextTick&&!process.browser&&"undefined"!=typeof require}
```

## Sync Minify Excerpt

```ts
import{CharacterCodes,Comparison,Debug}from"./_namespaces/ts.js";export const emptyArray=[];export const emptyMap=new Map;export function length(array){return void 0!==array?array.length:0}export function forEach(array,callback){if(void 0!==array)for(let i=0;i<array.length;i++){const result=callback(array[i],i);if(result)return result}}export function forEachRight(array,callback){if(void 0!==array)for(let i=array.length-1;i>=0;i--){const result=callback(array[i],i);if(result)return result}}export function firstDefined(array,callback){if(void 0!==array)for(let i=0;i<array.length;i++){const result=callback(array[i],i);if(void 0!==result)return result}}export function firstDefinedIterator(iter,callback){for(const value of iter){const result=callback(value);if(void 0!==result)return result}}export function reduceLeftIterator(iterator,f,initial){let result=initial;if(iterator){let pos=0;for(const value of iterator)result=f(result,value,pos),pos++}return result}export function zipWith(arrayA,arrayB,callback){const result=[];Debug.assertEqual(arrayA.length,arrayB.length);for(let i=0;i<arrayA.length;i++)result.push(callback(arrayA[i],arrayB[i],i));return result}export function intersperse(input,element){if(input.

... [truncated 27880 chars] ...

x===arrays.length-1?result.push(inner):cartesianProductWorker(arrays,result,inner,index+1)}}export function takeWhile(array,predicate){if(void 0!==array){const len=array.length;let index=0;while(index<len&&predicate(array[index]))index++;return array.slice(0,index)}}export function skipWhile(array,predicate){if(void 0!==array){const len=array.length;let index=0;while(index<len&&predicate(array[index]))index++;return array.slice(index)}}export function isNodeLikeSystem(){return"undefined"!=typeof process&&!!process.nextTick&&!process.browser&&"undefined"!=typeof require}
```

## Async Minify Excerpt

```ts
import{CharacterCodes,Comparison,Debug}from"./_namespaces/ts.js";export const emptyArray=[];export const emptyMap=new Map;export function length(array){return void 0!==array?array.length:0}export function forEach(array,callback){if(void 0!==array)for(let i=0;i<array.length;i++){const result=callback(array[i],i);if(result)return result}}export function forEachRight(array,callback){if(void 0!==array)for(let i=array.length-1;i>=0;i--){const result=callback(array[i],i);if(result)return result}}export function firstDefined(array,callback){if(void 0!==array)for(let i=0;i<array.length;i++){const result=callback(array[i],i);if(void 0!==result)return result}}export function firstDefinedIterator(iter,callback){for(const value of iter){const result=callback(value);if(void 0!==result)return result}}export function reduceLeftIterator(iterator,f,initial){let result=initial;if(iterator){let pos=0;for(const value of iterator)result=f(result,value,pos),pos++}return result}export function zipWith(arrayA,arrayB,callback){const result=[];Debug.assertEqual(arrayA.length,arrayB.length);for(let i=0;i<arrayA.length;i++)result.push(callback(arrayA[i],arrayB[i],i));return result}export function intersperse(input,element){if(input.

... [truncated 27880 chars] ...

x===arrays.length-1?result.push(inner):cartesianProductWorker(arrays,result,inner,index+1)}}export function takeWhile(array,predicate){if(void 0!==array){const len=array.length;let index=0;while(index<len&&predicate(array[index]))index++;return array.slice(0,index)}}export function skipWhile(array,predicate){if(void 0!==array){const len=array.length;let index=0;while(index<len&&predicate(array[index]))index++;return array.slice(index)}}export function isNodeLikeSystem(){return"undefined"!=typeof process&&!!process.nextTick&&!process.browser&&"undefined"!=typeof require}
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

... [truncated 25937 chars] ...

rray: readonly T[], predicate: (element: T) => element is U): U[];
2559| export function takeWhile<T, U extends T>(array: readonly T[] | undefined, predicate: (element: T) => element is U): U[] | undefined;
2560| export function takeWhile<T, U extends T>(array: readonly T[] | undefined, predicate: (element: T) => element is U): U[] | undefined {
2572| export function skipWhile<T, U extends T>(array: readonly T[], predicate: (element: T) => element is U): Exclude<T, U>[];
2574| export function skipWhile<T, U extends T>(array: readonly T[] | undefined, predicate: (element: T) => element is U): Exclude<T, U>[] | undefined;
2576| export function skipWhile<T, U extends T>(array: readonly T[] | undefined, predicate: (element: T) => element is U): Exclude<T, U>[] | undefined {
2588| export function isNodeLikeSystem(): boolean {
```
