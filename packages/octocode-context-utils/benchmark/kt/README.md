# Kotlin (.kt)

Source sample: `kt/Collections.kt`

Strategy: `conservative`

| Tool | Bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| input | 20559 | - | - |
| content-view | 10457 | 49.1% | 0.457 ms |
| applyMinification | 10492 | 49% | 0.441 ms |
| sync minify | 10492 | 49% | 0.447 ms |
| async minify | 10492 | 49% | 0.454 ms |
| symbols | 6907 | 66.4% | 0.588 ms |

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

... [truncated 4307 chars] ...

nt): Int
478| internal expect fun checkCountOverflow(count: Int): Int
483| internal fun throwIndexOverflow() { throw ArithmeticException("Index overflow has happened.") }
487| internal fun throwCountOverflow() { throw ArithmeticException("Count overflow has happened.") }
490| internal fun collectionToArrayCommonImpl(collection: Collection<*>): Array<Any?> {
493|     val destination = arrayOfNulls<Any>(collection.size)
495|     val iterator = collection.iterator()
496|     var index = 0
504| internal fun <T> collectionToArrayCommonImpl(collection: Collection<*>, array: Array<T>): Array<T> {
507|     val destination = if (array.size < collection.size) {
513|     val iterator = collection.iterator()
514|     var index = 0
528| internal expect fun <T> terminateCollectionToArray(collectionSize: Int, array: Array<T>): Array<T>
```
