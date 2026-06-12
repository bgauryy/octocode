# Kotlin (.kt)

Source sample: `kt/Collections.kt`

Strategy: `conservative`

Agent rating: **9/10 (excellent)**

Agent understanding from minified output: **9.8/10 (excellent)**

Artifacts:

- `raw/source.excerpt.txt`
- `minified/content-view.excerpt.txt`
- `minified/apply-minification.excerpt.txt`
- `minified/minify-content-sync.excerpt.txt`
- `minified/minify-content-async.excerpt.txt`
- `symbol/signatures.txt`

| Tool | Bytes | Cut | Time | Rating |
| --- | ---: | ---: | ---: | ---: |
| input | 20559 | - | - | - |
| content-view | 10457 | 49.1% | 3.342 ms | 9/10 |
| applyMinification | 10492 | 49% | 3.062 ms | 9/10 |
| sync minify | 10492 | 49% | 2.998 ms | 9/10 |
| async minify | 10492 | 49% | 3.142 ms | 9/10 |
| symbols | 5961 | 71% | 0.488 ms | 9/10 |

## Agent Understanding

Measured from `standard` minified output.

| Component | Score |
| --- | ---: |
| syntax anchors | 10/10 (3/3) |
| delimiter structure | 10/10 |
| output health | 9/10 |
| context budget | 10/10 |
| symbol context | 10/10 |
| signals passed | 6/6 |

## Agent Observation By Output Level

Ratings are computed from the actual raw, standard, minify, and symbol outputs
for this language sample.

| Level | Bytes | Cut | Agent observation | Syntax anchors | Structure |
| --- | ---: | ---: | ---: | ---: | ---: |
| none | 20559 | 0% | 10/10 excellent | 10/10 | 10/10 |
| standard | 10457 | 49.1% | 9.8/10 excellent | 10/10 | 10/10 |
| minify | 10492 | 49% | 9.8/10 excellent | 10/10 | 10/10 |
| symbols | 5961 | 71% | 8/10 strong | 6.7/10 | 7.7/10 |

## Notes

- conservative text strategy.

## Before Excerpt

```kt
/*
 * Copyright 2010-2023 JetBrains s.r.o. and Kotlin Programming Language contributors.
 * Use of this source code is governed by the Apache 2.0 license that can be found in the license/LICENSE.txt file.
 */

@file:kotlin.js.JsFileName("CollectionsKt")
@file:kotlin.jvm.JvmMultifileClass
@file:kotlin.jvm.JvmName("CollectionsKt")
@file:OptIn(kotlin.experimental.ExperimentalTypeInference::class, kotlin.js.ExperimentalJsFileName::class)

package kotlin.collections

import kotlin.contracts.*
import kotlin.random.Random

internal object EmptyIterator : ListIterator<Nothing> {
    override fun hasNext(): Boolean = false
    override fun hasPrevious(): Boolean = false
    override fun nextIndex(): Int = 0
    override fun previousIndex(): Int = -1
    override fun next(): Nothing = throw NoSuchElementException()
    override fun previous(): Nothing = throw NoSuchElementException()
}

internal object EmptyList : List<Nothing>, Serializable, RandomAccess {
    private const val serialVersionUID: Long = -7390468764508069838L

    override fun equals(other: Any?): Boolean = other is List<*> && other.isEmpty()
    override fun hashCode(): Int = 1
    override fun toString(): String = "[]"

    override val size: Int 

... [truncated 18759 chars] ...

n.size)
    } else {
        array
    }

    val iterator = collection.iterator()
    var index = 0
    while (iterator.hasNext()) {
        @Suppress("UNCHECKED_CAST")
        destination[index++] = iterator.next() as T
    }

    return terminateCollectionToArray(collection.size, destination)
}

/**
 * In JVM if the size of [array] is bigger than [collectionSize], sets `array[collectionSize] = null`.
 * In other platforms does nothing.
 * Returns the given [array].
 */
internal expect fun <T> terminateCollectionToArray(collectionSize: Int, array: Array<T>): Array<T>

```

## Content-View Excerpt

```kt
@file:kotlin.js.JsFileName("CollectionsKt")
@file:kotlin.jvm.JvmMultifileClass
@file:kotlin.jvm.JvmName("CollectionsKt")
@file:OptIn(kotlin.experimental.ExperimentalTypeInference::class, kotlin.js.ExperimentalJsFileName::class)

package kotlin.collections

import kotlin.contracts.*
import kotlin.random.Random

internal object EmptyIterator : ListIterator<Nothing> {
    override fun hasNext(): Boolean = false
    override fun hasPrevious(): Boolean = false
    override fun nextIndex(): Int = 0
    override fun previousIndex(): Int = -1
    override fun next(): Nothing = throw NoSuchElementException()
    override fun previous(): Nothing = throw NoSuchElementException()
}

internal object EmptyList : List<Nothing>, Serializable, RandomAccess {
    private const val serialVersionUID: Long = -7390468764508069838L

    override fun equals(other: Any?): Boolean = other is List<*> && other.isEmpty()
    override fun hashCode(): Int = 1
    override fun toString(): String = "[]"

    override val size: Int get() = 0
    override fun isEmpty(): Boolean = true
    override fun contains(element: Nothing): Boolean = false
    override fun containsAll(elements: Collection<Nothing>): Boolean = elements.isEmpty()

    o

... [truncated 8657 chars] ...

ay<T> {
    if (collection.isEmpty()) return terminateCollectionToArray(0, array)

    val destination = if (array.size < collection.size) {
        arrayOfNulls(array, collection.size)
    } else {
        array
    }

    val iterator = collection.iterator()
    var index = 0
    while (iterator.hasNext()) {
        @Suppress("UNCHECKED_CAST")
        destination[index++] = iterator.next() as T
    }

    return terminateCollectionToArray(collection.size, destination)
}

internal expect fun <T> terminateCollectionToArray(collectionSize: Int, array: Array<T>): Array<T>
```

## Apply Minification Excerpt

```kt


@file:kotlin.js.JsFileName("CollectionsKt")
@file:kotlin.jvm.JvmMultifileClass
@file:kotlin.jvm.JvmName("CollectionsKt")
@file:OptIn(kotlin.experimental.ExperimentalTypeInference::class, kotlin.js.ExperimentalJsFileName::class)

package kotlin.collections

import kotlin.contracts.*
import kotlin.random.Random

internal object EmptyIterator : ListIterator<Nothing> {
    override fun hasNext(): Boolean = false
    override fun hasPrevious(): Boolean = false
    override fun nextIndex(): Int = 0
    override fun previousIndex(): Int = -1
    override fun next(): Nothing = throw NoSuchElementException()
    override fun previous(): Nothing = throw NoSuchElementException()
}

internal object EmptyList : List<Nothing>, Serializable, RandomAccess {
    private const val serialVersionUID: Long = -7390468764508069838L

    override fun equals(other: Any?): Boolean = other is List<*> && other.isEmpty()
    override fun hashCode(): Int = 1
    override fun toString(): String = "[]"

    override val size: Int get() = 0
    override fun isEmpty(): Boolean = true
    override fun contains(element: Nothing): Boolean = false
    override fun containsAll(elements: Collection<Nothing>): Boolean = elements.isEmpty()

   

... [truncated 8692 chars] ...

y<T> {
    if (collection.isEmpty()) return terminateCollectionToArray(0, array)

    val destination = if (array.size < collection.size) {
        arrayOfNulls(array, collection.size)
    } else {
        array
    }

    val iterator = collection.iterator()
    var index = 0
    while (iterator.hasNext()) {
        @Suppress("UNCHECKED_CAST")
        destination[index++] = iterator.next() as T
    }

    return terminateCollectionToArray(collection.size, destination)
}


internal expect fun <T> terminateCollectionToArray(collectionSize: Int, array: Array<T>): Array<T>
```

## Sync Minify Excerpt

```kt


@file:kotlin.js.JsFileName("CollectionsKt")
@file:kotlin.jvm.JvmMultifileClass
@file:kotlin.jvm.JvmName("CollectionsKt")
@file:OptIn(kotlin.experimental.ExperimentalTypeInference::class, kotlin.js.ExperimentalJsFileName::class)

package kotlin.collections

import kotlin.contracts.*
import kotlin.random.Random

internal object EmptyIterator : ListIterator<Nothing> {
    override fun hasNext(): Boolean = false
    override fun hasPrevious(): Boolean = false
    override fun nextIndex(): Int = 0
    override fun previousIndex(): Int = -1
    override fun next(): Nothing = throw NoSuchElementException()
    override fun previous(): Nothing = throw NoSuchElementException()
}

internal object EmptyList : List<Nothing>, Serializable, RandomAccess {
    private const val serialVersionUID: Long = -7390468764508069838L

    override fun equals(other: Any?): Boolean = other is List<*> && other.isEmpty()
    override fun hashCode(): Int = 1
    override fun toString(): String = "[]"

    override val size: Int get() = 0
    override fun isEmpty(): Boolean = true
    override fun contains(element: Nothing): Boolean = false
    override fun containsAll(elements: Collection<Nothing>): Boolean = elements.isEmpty()

   

... [truncated 8692 chars] ...

y<T> {
    if (collection.isEmpty()) return terminateCollectionToArray(0, array)

    val destination = if (array.size < collection.size) {
        arrayOfNulls(array, collection.size)
    } else {
        array
    }

    val iterator = collection.iterator()
    var index = 0
    while (iterator.hasNext()) {
        @Suppress("UNCHECKED_CAST")
        destination[index++] = iterator.next() as T
    }

    return terminateCollectionToArray(collection.size, destination)
}


internal expect fun <T> terminateCollectionToArray(collectionSize: Int, array: Array<T>): Array<T>
```

## Async Minify Excerpt

```kt


@file:kotlin.js.JsFileName("CollectionsKt")
@file:kotlin.jvm.JvmMultifileClass
@file:kotlin.jvm.JvmName("CollectionsKt")
@file:OptIn(kotlin.experimental.ExperimentalTypeInference::class, kotlin.js.ExperimentalJsFileName::class)

package kotlin.collections

import kotlin.contracts.*
import kotlin.random.Random

internal object EmptyIterator : ListIterator<Nothing> {
    override fun hasNext(): Boolean = false
    override fun hasPrevious(): Boolean = false
    override fun nextIndex(): Int = 0
    override fun previousIndex(): Int = -1
    override fun next(): Nothing = throw NoSuchElementException()
    override fun previous(): Nothing = throw NoSuchElementException()
}

internal object EmptyList : List<Nothing>, Serializable, RandomAccess {
    private const val serialVersionUID: Long = -7390468764508069838L

    override fun equals(other: Any?): Boolean = other is List<*> && other.isEmpty()
    override fun hashCode(): Int = 1
    override fun toString(): String = "[]"

    override val size: Int get() = 0
    override fun isEmpty(): Boolean = true
    override fun contains(element: Nothing): Boolean = false
    override fun containsAll(elements: Collection<Nothing>): Boolean = elements.isEmpty()

   

... [truncated 8692 chars] ...

y<T> {
    if (collection.isEmpty()) return terminateCollectionToArray(0, array)

    val destination = if (array.size < collection.size) {
        arrayOfNulls(array, collection.size)
    } else {
        array
    }

    val iterator = collection.iterator()
    var index = 0
    while (iterator.hasNext()) {
        @Suppress("UNCHECKED_CAST")
        destination[index++] = iterator.next() as T
    }

    return terminateCollectionToArray(collection.size, destination)
}


internal expect fun <T> terminateCollectionToArray(collectionSize: Int, array: Array<T>): Array<T>
```

## Symbols

```txt
 11| package kotlin.collections
 13| import kotlin.contracts.*
 14| import kotlin.random.Random
 16| internal object EmptyIterator : ListIterator<Nothing> {
 17|     override fun hasNext(): Boolean = false
 18|     override fun hasPrevious(): Boolean = false
 19|     override fun nextIndex(): Int = 0
 20|     override fun previousIndex(): Int = -1
 21|     override fun next(): Nothing = throw NoSuchElementException()
 22|     override fun previous(): Nothing = throw NoSuchElementException()
 25| internal object EmptyList : List<Nothing>, Serializable, RandomAccess {
 26|     private const val serialVersionUID: Long = -7390468764508069838L
 28|     override fun equals(other: Any?): Boolean = other is List<*> && other.isEmpty()
 29|     override fun hashCode(): Int = 1
 30|     override fun toString(): String = "[]"
 32|     override val size: Int get() = 0
 33|     override fun isEmpty(): Boolean = true
 34|     override fun contains(element: Nothing): Boolean = false
 35|     override fun containsAll(elements: Collection<Nothing>): Boolean = elements.isEmpty()
 37|     override fun get(index: Int): Nothing = throw IndexOutOfBoundsException("Empty list doesn't contain element at index $index.")
 38|     override fun indexOf(element: Nothing): Int = -1
 39|     override fun lastIndexOf(element: Nothing): Int = -1
 41|     override fun iterator(): Iterator<Nothing> = EmptyIterator
 42|     override fun listIterator(): ListIterator<Nothing> = EmptyIterator
 43|     override fun listIterator(index: Int): ListIterator<Nothing> {
 48|     override fun subList(fromIndex: Int, toIndex: Int): List<Nothing> {
 53|     private fun readResolve(): Any = EmptyList
 57| internal expect inline fun <T> Array<out T>.asArrayList(): ArrayList<T>
 59| internal

... [truncated 3361 chars] ...

r: (T) -> K?
409| ): Int =
436| public fun <T> List<T>.binarySearch(fromIndex: Int = 0, toIndex: Int = size, comparison: (T) -> Int): Int {
461| private fun rangeCheck(size: Int, fromIndex: Int, toIndex: Int) {
473| internal expect fun checkIndexOverflow(index: Int): Int
478| internal expect fun checkCountOverflow(count: Int): Int
483| internal fun throwIndexOverflow() { throw ArithmeticException("Index overflow has happened.") }
487| internal fun throwCountOverflow() { throw ArithmeticException("Count overflow has happened.") }
490| internal fun collectionToArrayCommonImpl(collection: Collection<*>): Array<Any?> {
504| internal fun <T> collectionToArrayCommonImpl(collection: Collection<*>, array: Array<T>): Array<T> {
528| internal expect fun <T> terminateCollectionToArray(collectionSize: Int, array: Array<T>): Array<T>
```
