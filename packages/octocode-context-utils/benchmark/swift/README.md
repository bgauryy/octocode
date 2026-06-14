# Swift (.swift)

Source sample: `swift/Optional.swift`

Strategy: `conservative`

| Tool | Bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| input | 33805 | - | - |
| content-view | 11652 | 65.5% | 0.683 ms |
| applyMinification | 11683 | 65.4% | 0.588 ms |
| sync minify | 11683 | 65.4% | 0.55 ms |
| async minify | 11683 | 65.4% | 0.623 ms |
| symbols | 6335 | 81.3% | 1.11 ms |

## Notes

- conservative text strategy.

## Before Excerpt

```swift
//===----------------------------------------------------------------------===//
//
// This source file is part of the Swift.org open source project
//
// Copyright (c) 2014 - 2026 Apple Inc. and the Swift project authors
// Licensed under Apache License v2.0 with Runtime Library Exception
//
// See https://swift.org/LICENSE.txt for license information
// See https://swift.org/CONTRIBUTORS.txt for the list of Swift project authors
//
//===----------------------------------------------------------------------===//

/// A type that represents either a wrapped value or the absence of a value.
///
/// You use the `Optional` type whenever you use optional values, even if you
/// never type the word `Optional`. Swift's type system usually shows the
/// wrapped type's name with a trailing question mark (`?`) instead of showing
/// the full type name. For example, if a variable has the type `Int?`, that's
/// just another way of writing `Optional<Int>`. The shortened form is
/// preferred for ease of reading and writing code.
///
/// The types of `shortForm` and `longForm` in the following code sample are
/// the same:
///
///     let shortForm: Int? = Int("42")
///     let longForm: Optional<Int> = Int("42")
///

... [truncated 32005 chars] ...

t = .some(.some(unwrappedResult))
      return true
    } else {
      result = .none
      return false
    }
  }

  @_effects(readonly)
  public static func _unconditionallyBridgeFromObjectiveC(_ source: AnyObject?)
      -> Optional<Wrapped> {
    if let nonnullSource = source {
      // Map the nil sentinel back to none.
      if nonnullSource === _nilSentinel {
        return .none
      } else {
        return .some(nonnullSource as! Wrapped)
      }
    } else {
      // If we unexpectedly got nil, just map it to `none` too.
      return .none
    }
  }
}
#endif

```

## Content-View Excerpt

```swift
@frozen
public enum Optional<Wrapped: ~Copyable & ~Escapable>: ~Copyable, ~Escapable {

  case none

  case some(Wrapped)
}

extension Optional: Copyable where Wrapped: Copyable & ~Escapable {}

extension Optional: Escapable where Wrapped: Escapable & ~Copyable {}

extension Optional: BitwiseCopyable
where Wrapped: BitwiseCopyable & ~Escapable {}

extension Optional: Sendable where Wrapped: ~Copyable & ~Escapable & Sendable {}

@_preInverseGenerics
extension Optional: ExpressibleByNilLiteral
where Wrapped: ~Copyable & ~Escapable {

  @_transparent
  @_preInverseGenerics
  @_lifetime(immortal)
  public init(nilLiteral: ()) {
    self = .none
  }
}

extension Optional where Wrapped: ~Copyable & ~Escapable {

  @_transparent
  @_preInverseGenerics
  @_lifetime(copy value)
  public init(_ value: consuming Wrapped) {
    self = .some(value)
  }
}

extension Optional {

  @_alwaysEmitIntoClient
  public func map<E: Error, U: ~Copyable>(
    _ transform: (Wrapped) throws(E) -> U
  ) throws(E) -> U? {
    switch self {
    case .some(let y):
      return .some(try transform(y))
    case .none:
      return .none
    }
  }

  @_spi(SwiftStdlibLegacyABI) @available(swift, obsoleted: 1)
  @usableFromInline
  interna

... [truncated 9852 chars] ...

esult = .some(.none)
      return true
    }

    if let unwrappedResult = source as? Wrapped {
      result = .some(.some(unwrappedResult))
      return true
    } else {
      result = .none
      return false
    }
  }

  @_effects(readonly)
  public static func _unconditionallyBridgeFromObjectiveC(_ source: AnyObject?)
      -> Optional<Wrapped> {
    if let nonnullSource = source {

      if nonnullSource === _nilSentinel {
        return .none
      } else {
        return .some(nonnullSource as! Wrapped)
      }
    } else {

      return .none
    }
  }
}
#endif
```

## Apply Minification Excerpt

```swift


@frozen
public enum Optional<Wrapped: ~Copyable & ~Escapable>: ~Copyable, ~Escapable {


  case none


  case some(Wrapped)
}

extension Optional: Copyable where Wrapped: Copyable & ~Escapable {}

extension Optional: Escapable where Wrapped: Escapable & ~Copyable {}

extension Optional: BitwiseCopyable
where Wrapped: BitwiseCopyable & ~Escapable {}

extension Optional: Sendable where Wrapped: ~Copyable & ~Escapable & Sendable {}


@_preInverseGenerics
extension Optional: ExpressibleByNilLiteral
where Wrapped: ~Copyable & ~Escapable {


  @_transparent
  @_preInverseGenerics
  @_lifetime(immortal)
  public init(nilLiteral: ()) {
    self = .none
  }
}

extension Optional where Wrapped: ~Copyable & ~Escapable {

  @_transparent
  @_preInverseGenerics
  @_lifetime(copy value)
  public init(_ value: consuming Wrapped) {
    self = .some(value)
  }
}

extension Optional {


  @_alwaysEmitIntoClient
  public func map<E: Error, U: ~Copyable>(
    _ transform: (Wrapped) throws(E) -> U
  ) throws(E) -> U? {
    switch self {
    case .some(let y):
      return .some(try transform(y))
    case .none:
      return .none
    }
  }

  @_spi(SwiftStdlibLegacyABI) @available(swift, obsoleted: 1)
  @usableFromInline


... [truncated 9883 chars] ...

esult = .some(.none)
      return true
    }

    if let unwrappedResult = source as? Wrapped {
      result = .some(.some(unwrappedResult))
      return true
    } else {
      result = .none
      return false
    }
  }

  @_effects(readonly)
  public static func _unconditionallyBridgeFromObjectiveC(_ source: AnyObject?)
      -> Optional<Wrapped> {
    if let nonnullSource = source {

      if nonnullSource === _nilSentinel {
        return .none
      } else {
        return .some(nonnullSource as! Wrapped)
      }
    } else {

      return .none
    }
  }
}
#endif
```

## Sync Minify Excerpt

```swift


@frozen
public enum Optional<Wrapped: ~Copyable & ~Escapable>: ~Copyable, ~Escapable {


  case none


  case some(Wrapped)
}

extension Optional: Copyable where Wrapped: Copyable & ~Escapable {}

extension Optional: Escapable where Wrapped: Escapable & ~Copyable {}

extension Optional: BitwiseCopyable
where Wrapped: BitwiseCopyable & ~Escapable {}

extension Optional: Sendable where Wrapped: ~Copyable & ~Escapable & Sendable {}


@_preInverseGenerics
extension Optional: ExpressibleByNilLiteral
where Wrapped: ~Copyable & ~Escapable {


  @_transparent
  @_preInverseGenerics
  @_lifetime(immortal)
  public init(nilLiteral: ()) {
    self = .none
  }
}

extension Optional where Wrapped: ~Copyable & ~Escapable {

  @_transparent
  @_preInverseGenerics
  @_lifetime(copy value)
  public init(_ value: consuming Wrapped) {
    self = .some(value)
  }
}

extension Optional {


  @_alwaysEmitIntoClient
  public func map<E: Error, U: ~Copyable>(
    _ transform: (Wrapped) throws(E) -> U
  ) throws(E) -> U? {
    switch self {
    case .some(let y):
      return .some(try transform(y))
    case .none:
      return .none
    }
  }

  @_spi(SwiftStdlibLegacyABI) @available(swift, obsoleted: 1)
  @usableFromInline


... [truncated 9883 chars] ...

esult = .some(.none)
      return true
    }

    if let unwrappedResult = source as? Wrapped {
      result = .some(.some(unwrappedResult))
      return true
    } else {
      result = .none
      return false
    }
  }

  @_effects(readonly)
  public static func _unconditionallyBridgeFromObjectiveC(_ source: AnyObject?)
      -> Optional<Wrapped> {
    if let nonnullSource = source {

      if nonnullSource === _nilSentinel {
        return .none
      } else {
        return .some(nonnullSource as! Wrapped)
      }
    } else {

      return .none
    }
  }
}
#endif
```

## Async Minify Excerpt

```swift


@frozen
public enum Optional<Wrapped: ~Copyable & ~Escapable>: ~Copyable, ~Escapable {


  case none


  case some(Wrapped)
}

extension Optional: Copyable where Wrapped: Copyable & ~Escapable {}

extension Optional: Escapable where Wrapped: Escapable & ~Copyable {}

extension Optional: BitwiseCopyable
where Wrapped: BitwiseCopyable & ~Escapable {}

extension Optional: Sendable where Wrapped: ~Copyable & ~Escapable & Sendable {}


@_preInverseGenerics
extension Optional: ExpressibleByNilLiteral
where Wrapped: ~Copyable & ~Escapable {


  @_transparent
  @_preInverseGenerics
  @_lifetime(immortal)
  public init(nilLiteral: ()) {
    self = .none
  }
}

extension Optional where Wrapped: ~Copyable & ~Escapable {

  @_transparent
  @_preInverseGenerics
  @_lifetime(copy value)
  public init(_ value: consuming Wrapped) {
    self = .some(value)
  }
}

extension Optional {


  @_alwaysEmitIntoClient
  public func map<E: Error, U: ~Copyable>(
    _ transform: (Wrapped) throws(E) -> U
  ) throws(E) -> U? {
    switch self {
    case .some(let y):
      return .some(try transform(y))
    case .none:
      return .none
    }
  }

  @_spi(SwiftStdlibLegacyABI) @available(swift, obsoleted: 1)
  @usableFromInline


... [truncated 9883 chars] ...

esult = .some(.none)
      return true
    }

    if let unwrappedResult = source as? Wrapped {
      result = .some(.some(unwrappedResult))
      return true
    } else {
      result = .none
      return false
    }
  }

  @_effects(readonly)
  public static func _unconditionallyBridgeFromObjectiveC(_ source: AnyObject?)
      -> Optional<Wrapped> {
    if let nonnullSource = source {

      if nonnullSource === _nilSentinel {
        return .none
      } else {
        return .some(nonnullSource as! Wrapped)
      }
    } else {

      return .none
    }
  }
}
#endif
```

## Symbols

```txt
 120| @frozen
 121| public enum Optional<Wrapped: ~Copyable & ~Escapable>: ~Copyable, ~Escapable {
 135| extension Optional: Copyable where Wrapped: Copyable & ~Escapable {}
 137| extension Optional: Escapable where Wrapped: Escapable & ~Copyable {}
 139| extension Optional: BitwiseCopyable
 142| extension Optional: Sendable where Wrapped: ~Copyable & ~Escapable & Sendable {}
 145| @_preInverseGenerics
 146| extension Optional: ExpressibleByNilLiteral
 157|   @_transparent
 158|   @_preInverseGenerics
 159|   @_lifetime(immortal)
 160|   public init(nilLiteral: ()) {
 165| extension Optional where Wrapped: ~Copyable & ~Escapable {
 167|   @_transparent
 168|   @_preInverseGenerics
 169|   @_lifetime(copy value)
 170|   public init(_ value: consuming Wrapped) {
 175| extension Optional {
 197|   @_alwaysEmitIntoClient
 198|   public func map<E: Error, U: ~Copyable>(
 199|     _ transform: (Wrapped) throws(E) -> U
 200|   ) throws(E) -> U? {
 210|   @usableFromInline
 211|   internal func map<U>(
 212|     _ transform: (Wrapped) throws -> U
 213|   ) rethrows -> U? {
 223| extension Optional where Wrapped: ~Copyable {
 225|   @_alwaysEmitIntoClient
 238|   @_alwaysEmitIntoClient
 251| extension Optional {
 271|   @_alwaysEmitIntoClient
 272|   public func flatMap<E: Error, U: ~Copyable>(
 273|     _ transform: (Wrapped) throws(E) -> U?
 274|   ) throws(E) -> U? {
 284|   @usableFromInline
 285|   internal func flatMap<U>(
 286|     _ transform: (Wrapped) throws -> U?
 287|   ) rethrows -> U? {
 297| extension Optional where Wrapped: ~Copyable {
 299|   @_alwaysEmitIntoClient
 312|   @_alwaysEmitIntoClient
 313|   public func _borrowingFlatMap<U: ~Copyable, E: Error>(
 314|     _ transform: (borrowing Wrapped) throws(E) -> U?
 315|   ) throw

... [truncated 3735 chars] ...

ped throws
 914| ) rethrows -> T? {
 929| @usableFromInline
 930| internal func ?? <T>(
 931|   optional: T?,
 932|   defaultValue: @autoclosure () throws -> T?
 933| ) rethrows -> T? {
 947| extension Optional: _ObjectiveCBridgeable {
 949|   internal static var _nilSentinel: AnyObject {
 950|     @_silgen_name("_swift_Foundation_getOptionalNilSentinelObject")
 954|   public func _bridgeToObjectiveC() -> AnyObject {
 963|   public static func _forceBridgeFromObjectiveC(
 964|     _ source: AnyObject,
 965|     result: inout Optional<Wrapped>?
 966|   ) {
 980|   public static func _conditionallyBridgeFromObjectiveC(
 981|     _ source: AnyObject,
 982|     result: inout Optional<Wrapped>?
 983|   ) -> Bool {
1003|   @_effects(readonly)
1004|   public static func _unconditionallyBridgeFromObjectiveC(_ source: AnyObject?)
```
