# C++ (.cpp)

Source sample: `cpp/00-llvm-raw-ostream.cpp`

Strategy: `conservative`

| Tool | Bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| input | 32621 | - | - |
| content-view | 22759 | 30.2% | 0.99 ms |
| applyMinification | 22815 | 30.1% | 0.973 ms |
| sync minify | 22815 | 30.1% | 0.972 ms |
| async minify | 22815 | 30.1% | 1.03 ms |
| symbols | 7693 | 76.4% | 11.904 ms |

## Notes

- conservative text strategy.

## Before Excerpt

```cpp
//===--- raw_ostream.cpp - Implement the raw_ostream classes --------------===//
//
// Part of the LLVM Project, under the Apache License v2.0 with LLVM Exceptions.
// See https://llvm.org/LICENSE.txt for license information.
// SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception
//
//===----------------------------------------------------------------------===//
//
// This implements support for bulk buffered stream output.
//
//===----------------------------------------------------------------------===//

#include "llvm/Support/raw_ostream.h"
#include "llvm/ADT/StringExtras.h"
#include "llvm/Config/config.h"
#include "llvm/Support/AutoConvert.h"
#include "llvm/Support/Compiler.h"
#include "llvm/Support/Duration.h"
#include "llvm/Support/ErrorHandling.h"
#include "llvm/Support/FileSystem.h"
#include "llvm/Support/Format.h"
#include "llvm/Support/FormatVariadic.h"
#include "llvm/Support/IOSandbox.h"
#include "llvm/Support/MathExtras.h"
#include "llvm/Support/NativeFormatting.h"
#include "llvm/Support/Process.h"
#include "llvm/Support/Program.h"
#include <algorithm>
#include <cerrno>
#include <cstdio>
#include <sys/stat.h>

// <fcntl.h> may provide O_BINARY.
# include <fcntl.h>

#if defined(HAVE_UNISTD

... [truncated 30821 chars] ...

 + ".temp-stream-%%%%%%", Mode);
  if (!Temp)
    return createFileError(OutputFileName, Temp.takeError());

  raw_fd_ostream Out(Temp->FD, false);

#if defined(__MVS__)
  if (auto EC = llvm::copyFileTagAttributes(OutputFileName.str(), Temp->FD)) {
    if (EC != std::errc::no_such_file_or_directory)
      return createFileError(OutputFileName, EC);
  }
#endif

  if (Error E = Write(Out)) {
    if (Error DiscardError = Temp->discard())
      return joinErrors(std::move(E), std::move(DiscardError));
    return E;
  }
  Out.flush();

  return Temp->keep(OutputFileName);
}

```

## Content-View Excerpt

```cpp
#include "llvm/Support/raw_ostream.h"
#include "llvm/ADT/StringExtras.h"
#include "llvm/Config/config.h"
#include "llvm/Support/AutoConvert.h"
#include "llvm/Support/Compiler.h"
#include "llvm/Support/Duration.h"
#include "llvm/Support/ErrorHandling.h"
#include "llvm/Support/FileSystem.h"
#include "llvm/Support/Format.h"
#include "llvm/Support/FormatVariadic.h"
#include "llvm/Support/IOSandbox.h"
#include "llvm/Support/MathExtras.h"
#include "llvm/Support/NativeFormatting.h"
#include "llvm/Support/Process.h"
#include "llvm/Support/Program.h"
#include <algorithm>
#include <cerrno>
#include <cstdio>
#include <sys/stat.h>

# include <fcntl.h>

#if defined(HAVE_UNISTD_H)
# include <unistd.h>
#endif

#if defined(__CYGWIN__)
#include <io.h>
#endif

#if defined(_MSC_VER)
#include <io.h>
#ifndef STDIN_FILENO
# define STDIN_FILENO 0
#endif
#ifndef STDOUT_FILENO
# define STDOUT_FILENO 1
#endif
#ifndef STDERR_FILENO
# define STDERR_FILENO 2
#endif
#endif

#ifdef _WIN32
#include "llvm/Support/ConvertUTF.h"
#include "llvm/Support/Signals.h"
#include "llvm/Support/Windows/WindowsSupport.h"
#endif

using namespace llvm;

raw_ostream::~raw_ostream() {

  assert(OutBufCur == OutBufStart &&
         "raw_ostream destructor

... [truncated 20959 chars] ...

e + ".temp-stream-%%%%%%", Mode);
  if (!Temp)
    return createFileError(OutputFileName, Temp.takeError());

  raw_fd_ostream Out(Temp->FD, false);

#if defined(__MVS__)
  if (auto EC = llvm::copyFileTagAttributes(OutputFileName.str(), Temp->FD)) {
    if (EC != std::errc::no_such_file_or_directory)
      return createFileError(OutputFileName, EC);
  }
#endif

  if (Error E = Write(Out)) {
    if (Error DiscardError = Temp->discard())
      return joinErrors(std::move(E), std::move(DiscardError));
    return E;
  }
  Out.flush();

  return Temp->keep(OutputFileName);
}
```

## Apply Minification Excerpt

```cpp


#include "llvm/Support/raw_ostream.h"
#include "llvm/ADT/StringExtras.h"
#include "llvm/Config/config.h"
#include "llvm/Support/AutoConvert.h"
#include "llvm/Support/Compiler.h"
#include "llvm/Support/Duration.h"
#include "llvm/Support/ErrorHandling.h"
#include "llvm/Support/FileSystem.h"
#include "llvm/Support/Format.h"
#include "llvm/Support/FormatVariadic.h"
#include "llvm/Support/IOSandbox.h"
#include "llvm/Support/MathExtras.h"
#include "llvm/Support/NativeFormatting.h"
#include "llvm/Support/Process.h"
#include "llvm/Support/Program.h"
#include <algorithm>
#include <cerrno>
#include <cstdio>
#include <sys/stat.h>


# include <fcntl.h>

#if defined(HAVE_UNISTD_H)
# include <unistd.h>
#endif

#if defined(__CYGWIN__)
#include <io.h>
#endif

#if defined(_MSC_VER)
#include <io.h>
#ifndef STDIN_FILENO
# define STDIN_FILENO 0
#endif
#ifndef STDOUT_FILENO
# define STDOUT_FILENO 1
#endif
#ifndef STDERR_FILENO
# define STDERR_FILENO 2
#endif
#endif

#ifdef _WIN32
#include "llvm/Support/ConvertUTF.h"
#include "llvm/Support/Signals.h"
#include "llvm/Support/Windows/WindowsSupport.h"
#endif

using namespace llvm;

raw_ostream::~raw_ostream() {


  assert(OutBufCur == OutBufStart &&
         "raw_ostream destru

... [truncated 21015 chars] ...

e + ".temp-stream-%%%%%%", Mode);
  if (!Temp)
    return createFileError(OutputFileName, Temp.takeError());

  raw_fd_ostream Out(Temp->FD, false);

#if defined(__MVS__)
  if (auto EC = llvm::copyFileTagAttributes(OutputFileName.str(), Temp->FD)) {
    if (EC != std::errc::no_such_file_or_directory)
      return createFileError(OutputFileName, EC);
  }
#endif

  if (Error E = Write(Out)) {
    if (Error DiscardError = Temp->discard())
      return joinErrors(std::move(E), std::move(DiscardError));
    return E;
  }
  Out.flush();

  return Temp->keep(OutputFileName);
}
```

## Sync Minify Excerpt

```cpp


#include "llvm/Support/raw_ostream.h"
#include "llvm/ADT/StringExtras.h"
#include "llvm/Config/config.h"
#include "llvm/Support/AutoConvert.h"
#include "llvm/Support/Compiler.h"
#include "llvm/Support/Duration.h"
#include "llvm/Support/ErrorHandling.h"
#include "llvm/Support/FileSystem.h"
#include "llvm/Support/Format.h"
#include "llvm/Support/FormatVariadic.h"
#include "llvm/Support/IOSandbox.h"
#include "llvm/Support/MathExtras.h"
#include "llvm/Support/NativeFormatting.h"
#include "llvm/Support/Process.h"
#include "llvm/Support/Program.h"
#include <algorithm>
#include <cerrno>
#include <cstdio>
#include <sys/stat.h>


# include <fcntl.h>

#if defined(HAVE_UNISTD_H)
# include <unistd.h>
#endif

#if defined(__CYGWIN__)
#include <io.h>
#endif

#if defined(_MSC_VER)
#include <io.h>
#ifndef STDIN_FILENO
# define STDIN_FILENO 0
#endif
#ifndef STDOUT_FILENO
# define STDOUT_FILENO 1
#endif
#ifndef STDERR_FILENO
# define STDERR_FILENO 2
#endif
#endif

#ifdef _WIN32
#include "llvm/Support/ConvertUTF.h"
#include "llvm/Support/Signals.h"
#include "llvm/Support/Windows/WindowsSupport.h"
#endif

using namespace llvm;

raw_ostream::~raw_ostream() {


  assert(OutBufCur == OutBufStart &&
         "raw_ostream destru

... [truncated 21015 chars] ...

e + ".temp-stream-%%%%%%", Mode);
  if (!Temp)
    return createFileError(OutputFileName, Temp.takeError());

  raw_fd_ostream Out(Temp->FD, false);

#if defined(__MVS__)
  if (auto EC = llvm::copyFileTagAttributes(OutputFileName.str(), Temp->FD)) {
    if (EC != std::errc::no_such_file_or_directory)
      return createFileError(OutputFileName, EC);
  }
#endif

  if (Error E = Write(Out)) {
    if (Error DiscardError = Temp->discard())
      return joinErrors(std::move(E), std::move(DiscardError));
    return E;
  }
  Out.flush();

  return Temp->keep(OutputFileName);
}
```

## Async Minify Excerpt

```cpp


#include "llvm/Support/raw_ostream.h"
#include "llvm/ADT/StringExtras.h"
#include "llvm/Config/config.h"
#include "llvm/Support/AutoConvert.h"
#include "llvm/Support/Compiler.h"
#include "llvm/Support/Duration.h"
#include "llvm/Support/ErrorHandling.h"
#include "llvm/Support/FileSystem.h"
#include "llvm/Support/Format.h"
#include "llvm/Support/FormatVariadic.h"
#include "llvm/Support/IOSandbox.h"
#include "llvm/Support/MathExtras.h"
#include "llvm/Support/NativeFormatting.h"
#include "llvm/Support/Process.h"
#include "llvm/Support/Program.h"
#include <algorithm>
#include <cerrno>
#include <cstdio>
#include <sys/stat.h>


# include <fcntl.h>

#if defined(HAVE_UNISTD_H)
# include <unistd.h>
#endif

#if defined(__CYGWIN__)
#include <io.h>
#endif

#if defined(_MSC_VER)
#include <io.h>
#ifndef STDIN_FILENO
# define STDIN_FILENO 0
#endif
#ifndef STDOUT_FILENO
# define STDOUT_FILENO 1
#endif
#ifndef STDERR_FILENO
# define STDERR_FILENO 2
#endif
#endif

#ifdef _WIN32
#include "llvm/Support/ConvertUTF.h"
#include "llvm/Support/Signals.h"
#include "llvm/Support/Windows/WindowsSupport.h"
#endif

using namespace llvm;

raw_ostream::~raw_ostream() {


  assert(OutBufCur == OutBufStart &&
         "raw_ostream destru

... [truncated 21015 chars] ...

e + ".temp-stream-%%%%%%", Mode);
  if (!Temp)
    return createFileError(OutputFileName, Temp.takeError());

  raw_fd_ostream Out(Temp->FD, false);

#if defined(__MVS__)
  if (auto EC = llvm::copyFileTagAttributes(OutputFileName.str(), Temp->FD)) {
    if (EC != std::errc::no_such_file_or_directory)
      return createFileError(OutputFileName, EC);
  }
#endif

  if (Error E = Write(Out)) {
    if (Error DiscardError = Temp->discard())
      return joinErrors(std::move(E), std::move(DiscardError));
    return E;
  }
  Out.flush();

  return Temp->keep(OutputFileName);
}
```

## Symbols

```txt
  13| #include "llvm/Support/raw_ostream.h"
  14| #include "llvm/ADT/StringExtras.h"
  15| #include "llvm/Config/config.h"
  16| #include "llvm/Support/AutoConvert.h"
  17| #include "llvm/Support/Compiler.h"
  18| #include "llvm/Support/Duration.h"
  19| #include "llvm/Support/ErrorHandling.h"
  20| #include "llvm/Support/FileSystem.h"
  21| #include "llvm/Support/Format.h"
  22| #include "llvm/Support/FormatVariadic.h"
  23| #include "llvm/Support/IOSandbox.h"
  24| #include "llvm/Support/MathExtras.h"
  25| #include "llvm/Support/NativeFormatting.h"
  26| #include "llvm/Support/Process.h"
  27| #include "llvm/Support/Program.h"
  28| #include <algorithm>
  29| #include <cerrno>
  30| #include <cstdio>
  31| #include <sys/stat.h>
  34| # include <fcntl.h>
  36| #if defined(HAVE_UNISTD_H)
  37| # include <unistd.h>
  38| #endif
  40| #if defined(__CYGWIN__)
  41| #include <io.h>
  42| #endif
  44| #if defined(_MSC_VER)
  45| #include <io.h>
  46| #ifndef STDIN_FILENO
  47| # define STDIN_FILENO 0
  48| #endif
  49| #ifndef STDOUT_FILENO
  50| # define STDOUT_FILENO 1
  51| #endif
  52| #ifndef STDERR_FILENO
  53| # define STDERR_FILENO 2
  54| #endif
  55| #endif
  57| #ifdef _WIN32
  58| #include "llvm/Support/ConvertUTF.h"
  59| #include "llvm/Support/Signals.h"
  60| #include "llvm/Support/Windows/WindowsSupport.h"
  61| #endif
  63| using namespace llvm;
  65| raw_ostream::~raw_ostream() {
  75| size_t raw_ostream::preferred_buffer_size() const {
  87| void raw_ostream::SetBuffered() {
  96| void raw_ostream::SetBufferAndMode(char *BufferStart, size_t Size,
  97|                                    BufferKind Mode) {
 115| raw_ostream &raw_ostream::operator<<(unsigned long N) {
 120| raw_ostream &raw_ostream::operator<<(long N) {
 125|

... [truncated 5093 chars] ...

raw_svector_ostream::write_impl(const char *Ptr, size_t Size) {
 972| void raw_svector_ostream::pwrite_impl(const char *Ptr, size_t Size,
 973|                                       uint64_t Offset) {
 977| bool raw_svector_ostream::classof(const raw_ostream *OS) {
 985| raw_null_ostream::~raw_null_ostream() {
 994| void raw_null_ostream::write_impl(const char *Ptr, size_t Size) {
 997| uint64_t raw_null_ostream::current_pos() const {
1001| void raw_null_ostream::pwrite_impl(const char *Ptr, size_t Size,
1002|                                    uint64_t Offset) {}
1004| void raw_pwrite_stream::anchor() {}
1006| void buffer_ostream::anchor() {}
1008| void buffer_unique_ostream::anchor() {}
1010| Error llvm::writeToOutput(StringRef OutputFileName,
1011|                           std::function<Error(raw_ostream &)> Write) {
```
