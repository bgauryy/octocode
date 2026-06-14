# Dart (.dart)

Source sample: `dart/dart-string.dart`

Strategy: `conservative`

| Tool | Bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| input | 37049 | - | - |
| content-view | 5383 | 85.5% | 0.444 ms |
| applyMinification | 5441 | 85.3% | 0.398 ms |
| sync minify | 5441 | 85.3% | 0.468 ms |
| async minify | 5441 | 85.3% | 0.447 ms |
| symbols | 450 | 98.8% | 0.118 ms |

## Notes

- conservative text strategy.
- symbols are not implemented for this extension.

## Before Excerpt

```dart
// Copyright (c) 2012, the Dart project authors.  Please see the AUTHORS file
// for details. All rights reserved. Use of this source code is governed by a
// BSD-style license that can be found in the LICENSE file.

part of "dart:core";

/// A sequence of UTF-16 code units.
///
/// Strings are mainly used to represent text. A character may be represented by
/// multiple code points, each code point consisting of one or two code
/// units. For example, the Papua New Guinea flag character requires four code
/// units to represent two code points, but should be treated like a single
/// character: "🇵🇬". Platforms that do not support the flag character may show
/// the letters "PG" instead. If the code points are swapped, it instead becomes
/// the Guadeloupe flag "🇬🇵" ("GP").
///
/// A string can be either single or multiline. Single line strings are
/// written using matching single or double quotes, and multiline strings are
/// written using triple quotes. The following are all valid Dart strings:
/// ```dart
/// 'Single quotes';
/// "Double quotes";
/// 'Double quotes in "single" quotes';
/// "Single quotes in 'double' quotes";
///
/// '''A
/// multiline
/// string''';
///
/// """
/// Another
/// mu

... [truncated 35227 chars] ...

 _nextPosition = _position;
    if (_position == 0) {
      _currentCodePoint = -1;
      return false;
    }
    int position = _position - 1;
    int codeUnit = string.codeUnitAt(position);
    if (_isTrailSurrogate(codeUnit) && position > 0) {
      int prevCodeUnit = string.codeUnitAt(position - 1);
      if (_isLeadSurrogate(prevCodeUnit)) {
        _position = position - 1;
        _currentCodePoint = _combineSurrogatePair(prevCodeUnit, codeUnit);
        return true;
      }
    }
    _position = position;
    _currentCodePoint = codeUnit;
    return true;
  }
}

```

## Content-View Excerpt

```dart
part of "dart:core";

@pragma('vm:entry-point')
abstract final class String implements Comparable<String>, Pattern {

  external factory String.fromCharCodes(
    Iterable<int> charCodes, [
    int start = 0,
    int? end,
  ]);

  external factory String.fromCharCode(int charCode);

  external const factory String.fromEnvironment(
    String name, {
    String defaultValue = "",
  });

  String operator [](int index);

  int codeUnitAt(int index);

  int get length;

  int get hashCode;

  bool operator ==(Object other);

  int compareTo(String other);

  bool endsWith(String other);

  bool startsWith(Pattern pattern, [int index = 0]);

  int indexOf(Pattern pattern, [int start = 0]);

  int lastIndexOf(Pattern pattern, [int? start]);

  bool get isEmpty;

  bool get isNotEmpty;

  String operator +(String other);

  String substring(int start, [int? end]);

  String trim();

  String trimLeft();

  String trimRight();

  String operator *(int times);

  String padLeft(int width, [String padding = ' ']);

  String padRight(int width, [String padding = ' ']);

  bool contains(Pattern other, [int startIndex = 0]);

  String replaceFirst(Pattern from, String to, [int startIndex = 0]);

  String replaceFirs

... [truncated 3583 chars] ...

  _nextPosition = _position;
    if (_position == 0) {
      _currentCodePoint = -1;
      return false;
    }
    int position = _position - 1;
    int codeUnit = string.codeUnitAt(position);
    if (_isTrailSurrogate(codeUnit) && position > 0) {
      int prevCodeUnit = string.codeUnitAt(position - 1);
      if (_isLeadSurrogate(prevCodeUnit)) {
        _position = position - 1;
        _currentCodePoint = _combineSurrogatePair(prevCodeUnit, codeUnit);
        return true;
      }
    }
    _position = position;
    _currentCodePoint = codeUnit;
    return true;
  }
}
```

## Apply Minification Excerpt

```dart


part of "dart:core";


@pragma('vm:entry-point')
abstract final class String implements Comparable<String>, Pattern {


  external factory String.fromCharCodes(
    Iterable<int> charCodes, [
    int start = 0,
    int? end,
  ]);


  external factory String.fromCharCode(int charCode);


  external const factory String.fromEnvironment(
    String name, {
    String defaultValue = "",
  });


  String operator [](int index);


  int codeUnitAt(int index);


  int get length;


  int get hashCode;


  bool operator ==(Object other);


  int compareTo(String other);


  bool endsWith(String other);


  bool startsWith(Pattern pattern, [int index = 0]);


  int indexOf(Pattern pattern, [int start = 0]);


  int lastIndexOf(Pattern pattern, [int? start]);


  bool get isEmpty;


  bool get isNotEmpty;


  String operator +(String other);


  String substring(int start, [int? end]);


  String trim();


  String trimLeft();


  String trimRight();


  String operator *(int times);


  String padLeft(int width, [String padding = ' ']);


  String padRight(int width, [String padding = ' ']);


  bool contains(Pattern other, [int startIndex = 0]);


  String replaceFirst(Pattern from, String to, [int startIndex

... [truncated 3641 chars] ...

  _nextPosition = _position;
    if (_position == 0) {
      _currentCodePoint = -1;
      return false;
    }
    int position = _position - 1;
    int codeUnit = string.codeUnitAt(position);
    if (_isTrailSurrogate(codeUnit) && position > 0) {
      int prevCodeUnit = string.codeUnitAt(position - 1);
      if (_isLeadSurrogate(prevCodeUnit)) {
        _position = position - 1;
        _currentCodePoint = _combineSurrogatePair(prevCodeUnit, codeUnit);
        return true;
      }
    }
    _position = position;
    _currentCodePoint = codeUnit;
    return true;
  }
}
```

## Sync Minify Excerpt

```dart


part of "dart:core";


@pragma('vm:entry-point')
abstract final class String implements Comparable<String>, Pattern {


  external factory String.fromCharCodes(
    Iterable<int> charCodes, [
    int start = 0,
    int? end,
  ]);


  external factory String.fromCharCode(int charCode);


  external const factory String.fromEnvironment(
    String name, {
    String defaultValue = "",
  });


  String operator [](int index);


  int codeUnitAt(int index);


  int get length;


  int get hashCode;


  bool operator ==(Object other);


  int compareTo(String other);


  bool endsWith(String other);


  bool startsWith(Pattern pattern, [int index = 0]);


  int indexOf(Pattern pattern, [int start = 0]);


  int lastIndexOf(Pattern pattern, [int? start]);


  bool get isEmpty;


  bool get isNotEmpty;


  String operator +(String other);


  String substring(int start, [int? end]);


  String trim();


  String trimLeft();


  String trimRight();


  String operator *(int times);


  String padLeft(int width, [String padding = ' ']);


  String padRight(int width, [String padding = ' ']);


  bool contains(Pattern other, [int startIndex = 0]);


  String replaceFirst(Pattern from, String to, [int startIndex

... [truncated 3641 chars] ...

  _nextPosition = _position;
    if (_position == 0) {
      _currentCodePoint = -1;
      return false;
    }
    int position = _position - 1;
    int codeUnit = string.codeUnitAt(position);
    if (_isTrailSurrogate(codeUnit) && position > 0) {
      int prevCodeUnit = string.codeUnitAt(position - 1);
      if (_isLeadSurrogate(prevCodeUnit)) {
        _position = position - 1;
        _currentCodePoint = _combineSurrogatePair(prevCodeUnit, codeUnit);
        return true;
      }
    }
    _position = position;
    _currentCodePoint = codeUnit;
    return true;
  }
}
```

## Async Minify Excerpt

```dart


part of "dart:core";


@pragma('vm:entry-point')
abstract final class String implements Comparable<String>, Pattern {


  external factory String.fromCharCodes(
    Iterable<int> charCodes, [
    int start = 0,
    int? end,
  ]);


  external factory String.fromCharCode(int charCode);


  external const factory String.fromEnvironment(
    String name, {
    String defaultValue = "",
  });


  String operator [](int index);


  int codeUnitAt(int index);


  int get length;


  int get hashCode;


  bool operator ==(Object other);


  int compareTo(String other);


  bool endsWith(String other);


  bool startsWith(Pattern pattern, [int index = 0]);


  int indexOf(Pattern pattern, [int start = 0]);


  int lastIndexOf(Pattern pattern, [int? start]);


  bool get isEmpty;


  bool get isNotEmpty;


  String operator +(String other);


  String substring(int start, [int? end]);


  String trim();


  String trimLeft();


  String trimRight();


  String operator *(int times);


  String padLeft(int width, [String padding = ' ']);


  String padRight(int width, [String padding = ' ']);


  bool contains(Pattern other, [int startIndex = 0]);


  String replaceFirst(Pattern from, String to, [int startIndex

... [truncated 3641 chars] ...

  _nextPosition = _position;
    if (_position == 0) {
      _currentCodePoint = -1;
      return false;
    }
    int position = _position - 1;
    int codeUnit = string.codeUnitAt(position);
    if (_isTrailSurrogate(codeUnit) && position > 0) {
      int prevCodeUnit = string.codeUnitAt(position - 1);
      if (_isLeadSurrogate(prevCodeUnit)) {
        _position = position - 1;
        _currentCodePoint = _combineSurrogatePair(prevCodeUnit, codeUnit);
        return true;
      }
    }
    _position = position;
    _currentCodePoint = codeUnit;
    return true;
  }
}
```

## Symbols

```txt
  5| part of "dart:core";
107| @pragma('vm:entry-point')
108| abstract final class String implements Comparable<String>, Pattern {
743| }
781| final class Runes extends Iterable<int> {
804| }
807| bool _isLeadSurrogate(int code) => (code & 0xFC00) == 0xD800;
810| bool _isTrailSurrogate(int code) => (code & 0xFC00) == 0xDC00;
813| int _combineSurrogatePair(int start, int end) {
815| }
818| final class RuneIterator implements Iterator<int> {
978| }
```
