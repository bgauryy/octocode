# Kotlin (.kt)

Source sample: `kt/Collections.kt`

Strategy: `conservative`

Agent rating: **9/10 (excellent)**

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
| content-view | 10457 | 49.1% | 2.492 ms | 9/10 |
| applyMinification | 10457 | 49.1% | 1.986 ms | 9/10 |
| sync minify | 10457 | 49.1% | 1.785 ms | 9/10 |
| async minify | 10457 | 49.1% | 2.25 ms | 9/10 |
| symbols | 4690 | 77.2% | 0.163 ms | 9/10 |

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

## Symbols

```txt
 11| package kotlin.collections
 13| import kotlin.contracts.*
 14| import kotlin.random.Random
 17|     override fun hasNext(): Boolean = false
 18|     override fun hasPrevious(): Boolean = false
 19|     override fun nextIndex(): Int = 0
 20|     override fun previousIndex(): Int = -1
 21|     override fun next(): Nothing = throw NoSuchElementException()
 22|     override fun previous(): Nothing = throw NoSuchElementException()
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
 61| private class ArrayAsCollection<T>(val values: Array<out T>, val isVarargs: Boolean) : Collection<T> {
 62|     override val size: Int get() = values.size
 63|     override fun isEmpty(): Boolean = values.isEmpty()
 64|    

... [truncated 2090 chars] ...

llection<T>.containsAll(elements: Collection<T>): Boolean = this.containsAll(elements)
306| public fun <T> Iterable<T>.shuffled(random: Random): List<T> = toMutableList().apply { shuffle(random) }
331| public fun <T : Comparable<T>> List<T?>.binarySearch(element: T?, fromIndex: Int = 0, toIndex: Int = size): Int {
367| public fun <T> List<T>.binarySearch(element: T, comparator: Comparator<in T>, fromIndex: Int = 0, toIndex: Int = size): Int {
404| public inline fun <T, K : Comparable<K>> List<T>.binarySearchBy(
405|     key: K?,
406|     fromIndex: Int = 0,
407|     toIndex: Int = size,
408|     crossinline selector: (T) -> K?
409| ): Int =
436| public fun <T> List<T>.binarySearch(fromIndex: Int = 0, toIndex: Int = size, comparison: (T) -> Int): Int {
461| private fun rangeCheck(size: Int, fromIndex: Int, toIndex: Int) {
```
