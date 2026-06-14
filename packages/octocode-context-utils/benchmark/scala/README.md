# Scala (.scala)

Source sample: `scala/Option.scala`

Strategy: `conservative`

| Tool | Bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| input | 20107 | - | - |
| content-view | 3882 | 80.7% | 0.231 ms |
| applyMinification | 3919 | 80.5% | 0.223 ms |
| sync minify | 3919 | 80.5% | 0.221 ms |
| async minify | 3919 | 80.5% | 0.222 ms |
| symbols | 1189 | 94.1% | 0.786 ms |

## Notes

- conservative text strategy.

## Before Excerpt

```scala
/*
 * Scala (https://www.scala-lang.org)
 *
 * Copyright EPFL and Lightbend, Inc. dba Akka
 *
 * Licensed under Apache License 2.0
 * (http://www.apache.org/licenses/LICENSE-2.0).
 *
 * See the NOTICE file distributed with this work for
 * additional information regarding copyright ownership.
 */

package scala

object Option {

  import scala.language.implicitConversions

  /** An implicit conversion that converts an option to an iterable value. */
  implicit def option2Iterable[A](xo: Option[A]): Iterable[A] =
    if (xo.isEmpty) Iterable.empty else Iterable.single(xo.get)

  /** An `Option` factory which creates `Some(x)` if the argument is not `null`,
   *  and `None` if it is `null`.
   *
   *  @param  x the value
   *  @return   `Some(value)` if value != null, `None` if value == null
   */
  def apply[A](x: A): Option[A] = if (x == null) None else Some(x)

  /** An Option factory which returns `None` in a manner consistent with
   *  the collections hierarchy.
   */
  def empty[A] : Option[A] = None

  /** When a given condition is true, evaluates the `a` argument and returns
   *  `Some(a)`. When the condition is false, `a` is not evaluated and `None` is
   *  returned.
   */
  def when[A](cond: Bo

... [truncated 18267 chars] ...

y) Right(right) else Left(this.get)
}

/** Class `Some[A]` represents existing values of type
 *  `A`.
 */
@SerialVersionUID(1234815782226070388L) // value computed by serialver for 2.11.2, annotation added in 2.11.4
final case class Some[+A](value: A) extends Option[A] {
  def get: A = value
}


/** This case object represents non-existent values.
 */
@SerialVersionUID(5066590221178148012L) // value computed by serialver for 2.11.2, annotation added in 2.11.4
case object None extends Option[Nothing] {
  def get: Nothing = throw new NoSuchElementException("None.get")
}

```

## Content-View Excerpt

```scala
package scala

object Option {

  import scala.language.implicitConversions

  implicit def option2Iterable[A](xo: Option[A]): Iterable[A] =
    if (xo.isEmpty) Iterable.empty else Iterable.single(xo.get)

  def apply[A](x: A): Option[A] = if (x == null) None else Some(x)

  def empty[A] : Option[A] = None

  def when[A](cond: Boolean)(a: => A): Option[A] =
    if (cond) Some(a) else None

  @inline def unless[A](cond: Boolean)(a: => A): Option[A] =
    when(!cond)(a)
}

@SerialVersionUID(-114498752079829388L)
sealed abstract class Option[+A] extends IterableOnce[A] with Product with Serializable {
  self =>

  final def isEmpty: Boolean = this eq None

  final def isDefined: Boolean = !isEmpty

  override final def knownSize: Int = if (isEmpty) 0 else 1

  def get: A

  @inline final def getOrElse[B >: A](default: => B): B =
    if (isEmpty) default else this.get

  @inline final def orNull[A1 >: A](implicit ev: Null <:< A1): A1 = this getOrElse ev(null)

  @inline final def map[B](f: A => B): Option[B] =
    if (isEmpty) None else Some(f(this.get))

  @inline final def fold[B](ifEmpty: => B)(f: A => B): B =
    if (isEmpty) ifEmpty else f(this.get)

  @inline final def flatMap[B](f: A => Option[B]): Opt

... [truncated 2082 chars] ...

is.get)

  def toList: List[A] =
    if (isEmpty) List() else new ::(this.get, Nil)

  @inline final def toRight[X](left: => X): Either[X, A] =
    if (isEmpty) Left(left) else Right(this.get)

  @inline final def toLeft[X](right: => X): Either[A, X] =
    if (isEmpty) Right(right) else Left(this.get)
}

@SerialVersionUID(1234815782226070388L)
final case class Some[+A](value: A) extends Option[A] {
  def get: A = value
}

@SerialVersionUID(5066590221178148012L)
case object None extends Option[Nothing] {
  def get: Nothing = throw new NoSuchElementException("None.get")
}
```

## Apply Minification Excerpt

```scala


package scala

object Option {

  import scala.language.implicitConversions


  implicit def option2Iterable[A](xo: Option[A]): Iterable[A] =
    if (xo.isEmpty) Iterable.empty else Iterable.single(xo.get)


  def apply[A](x: A): Option[A] = if (x == null) None else Some(x)


  def empty[A] : Option[A] = None


  def when[A](cond: Boolean)(a: => A): Option[A] =
    if (cond) Some(a) else None


  @inline def unless[A](cond: Boolean)(a: => A): Option[A] =
    when(!cond)(a)
}


@SerialVersionUID(-114498752079829388L)
sealed abstract class Option[+A] extends IterableOnce[A] with Product with Serializable {
  self =>


  final def isEmpty: Boolean = this eq None


  final def isDefined: Boolean = !isEmpty

  override final def knownSize: Int = if (isEmpty) 0 else 1


  def get: A


  @inline final def getOrElse[B >: A](default: => B): B =
    if (isEmpty) default else this.get


  @inline final def orNull[A1 >: A](implicit ev: Null <:< A1): A1 = this getOrElse ev(null)


  @inline final def map[B](f: A => B): Option[B] =
    if (isEmpty) None else Some(f(this.get))


  @inline final def fold[B](ifEmpty: => B)(f: A => B): B =
    if (isEmpty) ifEmpty else f(this.get)


  @inline final def flatMap[B](f: A =>

... [truncated 2119 chars] ...

t)


  def toList: List[A] =
    if (isEmpty) List() else new ::(this.get, Nil)


  @inline final def toRight[X](left: => X): Either[X, A] =
    if (isEmpty) Left(left) else Right(this.get)


  @inline final def toLeft[X](right: => X): Either[A, X] =
    if (isEmpty) Right(right) else Left(this.get)
}


@SerialVersionUID(1234815782226070388L)
final case class Some[+A](value: A) extends Option[A] {
  def get: A = value
}


@SerialVersionUID(5066590221178148012L)
case object None extends Option[Nothing] {
  def get: Nothing = throw new NoSuchElementException("None.get")
}
```

## Sync Minify Excerpt

```scala


package scala

object Option {

  import scala.language.implicitConversions


  implicit def option2Iterable[A](xo: Option[A]): Iterable[A] =
    if (xo.isEmpty) Iterable.empty else Iterable.single(xo.get)


  def apply[A](x: A): Option[A] = if (x == null) None else Some(x)


  def empty[A] : Option[A] = None


  def when[A](cond: Boolean)(a: => A): Option[A] =
    if (cond) Some(a) else None


  @inline def unless[A](cond: Boolean)(a: => A): Option[A] =
    when(!cond)(a)
}


@SerialVersionUID(-114498752079829388L)
sealed abstract class Option[+A] extends IterableOnce[A] with Product with Serializable {
  self =>


  final def isEmpty: Boolean = this eq None


  final def isDefined: Boolean = !isEmpty

  override final def knownSize: Int = if (isEmpty) 0 else 1


  def get: A


  @inline final def getOrElse[B >: A](default: => B): B =
    if (isEmpty) default else this.get


  @inline final def orNull[A1 >: A](implicit ev: Null <:< A1): A1 = this getOrElse ev(null)


  @inline final def map[B](f: A => B): Option[B] =
    if (isEmpty) None else Some(f(this.get))


  @inline final def fold[B](ifEmpty: => B)(f: A => B): B =
    if (isEmpty) ifEmpty else f(this.get)


  @inline final def flatMap[B](f: A =>

... [truncated 2119 chars] ...

t)


  def toList: List[A] =
    if (isEmpty) List() else new ::(this.get, Nil)


  @inline final def toRight[X](left: => X): Either[X, A] =
    if (isEmpty) Left(left) else Right(this.get)


  @inline final def toLeft[X](right: => X): Either[A, X] =
    if (isEmpty) Right(right) else Left(this.get)
}


@SerialVersionUID(1234815782226070388L)
final case class Some[+A](value: A) extends Option[A] {
  def get: A = value
}


@SerialVersionUID(5066590221178148012L)
case object None extends Option[Nothing] {
  def get: Nothing = throw new NoSuchElementException("None.get")
}
```

## Async Minify Excerpt

```scala


package scala

object Option {

  import scala.language.implicitConversions


  implicit def option2Iterable[A](xo: Option[A]): Iterable[A] =
    if (xo.isEmpty) Iterable.empty else Iterable.single(xo.get)


  def apply[A](x: A): Option[A] = if (x == null) None else Some(x)


  def empty[A] : Option[A] = None


  def when[A](cond: Boolean)(a: => A): Option[A] =
    if (cond) Some(a) else None


  @inline def unless[A](cond: Boolean)(a: => A): Option[A] =
    when(!cond)(a)
}


@SerialVersionUID(-114498752079829388L)
sealed abstract class Option[+A] extends IterableOnce[A] with Product with Serializable {
  self =>


  final def isEmpty: Boolean = this eq None


  final def isDefined: Boolean = !isEmpty

  override final def knownSize: Int = if (isEmpty) 0 else 1


  def get: A


  @inline final def getOrElse[B >: A](default: => B): B =
    if (isEmpty) default else this.get


  @inline final def orNull[A1 >: A](implicit ev: Null <:< A1): A1 = this getOrElse ev(null)


  @inline final def map[B](f: A => B): Option[B] =
    if (isEmpty) None else Some(f(this.get))


  @inline final def fold[B](ifEmpty: => B)(f: A => B): B =
    if (isEmpty) ifEmpty else f(this.get)


  @inline final def flatMap[B](f: A =>

... [truncated 2119 chars] ...

t)


  def toList: List[A] =
    if (isEmpty) List() else new ::(this.get, Nil)


  @inline final def toRight[X](left: => X): Either[X, A] =
    if (isEmpty) Left(left) else Right(this.get)


  @inline final def toLeft[X](right: => X): Either[A, X] =
    if (isEmpty) Right(right) else Left(this.get)
}


@SerialVersionUID(1234815782226070388L)
final case class Some[+A](value: A) extends Option[A] {
  def get: A = value
}


@SerialVersionUID(5066590221178148012L)
case object None extends Option[Nothing] {
  def get: Nothing = throw new NoSuchElementException("None.get")
}
```

## Symbols

```txt
 13| package scala
 15| object Option {
 17|   import scala.language.implicitConversions
 20|   implicit def option2Iterable[A](xo: Option[A]): Iterable[A] =
 29|   def apply[A](x: A): Option[A] = if (x == null) None else Some(x)
 34|   def empty[A] : Option[A] = None
 40|   def when[A](cond: Boolean)(a: => A): Option[A] =
144| sealed abstract class Option[+A] extends IterableOnce[A] with Product with Serializable {
185|   def get: A
303|   def flatten[B](implicit ev: A <:< Option[B]): Option[B] =
358|   class WithFilter(p: A => Boolean) {
359|     def map[B](f: A => B): Option[B] = self filter p map f
360|     def flatMap[B](f: A => Option[B]): Option[B] = self filter p flatMap f
361|     def foreach[U](f: A => U): Unit = self filter p foreach f
362|     def withFilter(q: A => Boolean): WithFilter = new WithFilter(x => p(x) && q(x))
526|       val e = asPair(this.get)
552|       val e = asTriple(this.get)
560|   def iterator: Iterator[A] =
574|   def toList: List[A] =
618| final case class Some[+A](value: A) extends Option[A] {
619|   def get: A = value
626| case object None extends Option[Nothing] {
627|   def get: Nothing = throw new NoSuchElementException("None.get")
```
