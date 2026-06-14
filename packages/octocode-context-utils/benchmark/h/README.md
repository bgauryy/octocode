# C Header (.h)

Source sample: `h/git-compat-util.h`

Strategy: `conservative`

| Tool | Bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| input | 33059 | - | - |
| content-view | 20155 | 39% | 0.805 ms |
| applyMinification | 20200 | 38.9% | 0.809 ms |
| sync minify | 20200 | 38.9% | 0.831 ms |
| async minify | 20200 | 38.9% | 0.859 ms |
| symbols | 19461 | 41.1% | 3.544 ms |

## Notes

- conservative text strategy.

## Before Excerpt

```c
#ifndef GIT_COMPAT_UTIL_H
#define GIT_COMPAT_UTIL_H

#if __STDC_VERSION__ - 0 < 199901L
/*
 * Git is in a testing period for mandatory C99 support in the compiler.  If
 * your compiler is reasonably recent, you can try to enable C99 support (or,
 * for MSVC, C11 support).  If you encounter a problem and can't enable C99
 * support with your compiler (such as with "-std=gnu99") and don't have access
 * to one with this support, such as GCC or Clang, you can remove this #if
 * directive, but please report the details of your system to
 * git@vger.kernel.org.
 */
#error "Required C99 support is in a test phase.  Please see git-compat-util.h for more details."
#endif

#ifdef USE_MSVC_CRTDBG
/*
 * For these to work they must appear very early in each
 * file -- before most of the standard header files.
 */
#include <stdlib.h>
#include <crtdbg.h>
#endif

#include "compat/posix.h"

struct strbuf;

#if defined(__GNUC__) || defined(__clang__)
#  define PRAGMA(pragma)           _Pragma(#pragma)
#  define DISABLE_WARNING(warning) PRAGMA(GCC diagnostic ignored #warning)
#else
#  define DISABLE_WARNING(warning)
#endif

#undef FLEX_ARRAY
#define FLEX_ARRAY /* empty - weather balloon to require C99 FAM */

/*
 * BUILD_A

... [truncated 31259 chars] ...

n. false_but_the_compiler_does_not_know_it_
 * is defined in a compilation unit separate from where the macro is
 * used, initialized to 0, and never modified.
 */
#define NOT_CONSTANT(expr) ((expr) || false_but_the_compiler_does_not_know_it_)
extern int false_but_the_compiler_does_not_know_it_;

#ifdef CHECK_ASSERTION_SIDE_EFFECTS
#undef assert
extern int not_supposed_to_survive;
#define assert(expr) ((void)(not_supposed_to_survive || (expr)))
#endif /* CHECK_ASSERTION_SIDE_EFFECTS */

#endif

#ifdef DISABLE_SIGN_COMPARE_WARNINGS
DISABLE_WARNING(-Wsign-compare)
#endif

```

## Content-View Excerpt

```c
#ifndef GIT_COMPAT_UTIL_H
#define GIT_COMPAT_UTIL_H

#if __STDC_VERSION__ - 0 < 199901L

#error "Required C99 support is in a test phase.  Please see git-compat-util.h for more details."
#endif

#ifdef USE_MSVC_CRTDBG

#include <stdlib.h>
#include <crtdbg.h>
#endif

#include "compat/posix.h"

struct strbuf;

#if defined(__GNUC__) || defined(__clang__)
#  define PRAGMA(pragma)           _Pragma(#pragma)
#  define DISABLE_WARNING(warning) PRAGMA(GCC diagnostic ignored #warning)
#else
#  define DISABLE_WARNING(warning)
#endif

#undef FLEX_ARRAY
#define FLEX_ARRAY

#define BUILD_ASSERT_OR_ZERO(cond) \
	(sizeof(char [1 - 2*!(cond)]) - 1)

#if GIT_GNUC_PREREQ(3, 1)

# define BARF_UNLESS_AN_ARRAY(arr)						\
	BUILD_ASSERT_OR_ZERO(!__builtin_types_compatible_p(__typeof__(arr), \
							   __typeof__(&(arr)[0])))
# define BARF_UNLESS_COPYABLE(dst, src) \
	BUILD_ASSERT_OR_ZERO(__builtin_types_compatible_p(__typeof__(*(dst)), \
							  __typeof__(*(src))))

# define BARF_UNLESS_SIGNED(var)   BUILD_ASSERT_OR_ZERO(((__typeof__(var)) -1) < 0)
# define BARF_UNLESS_UNSIGNED(var) BUILD_ASSERT_OR_ZERO(((__typeof__(var)) -1) > 0)
#else
# define BARF_UNLESS_AN_ARRAY(arr) 0
# define BARF_UNLESS_COPYABLE(dst, src) \
	BUILD_AS

... [truncated 18355 chars] ...

 member))

#if defined(__GNUC__)
#define OFFSETOF_VAR(ptr, member) offsetof(__typeof__(*ptr), member)
#else
#define OFFSETOF_VAR(ptr, member) \
	((uintptr_t)&(ptr)->member - (uintptr_t)(ptr))
#endif

#define NOT_CONSTANT(expr) ((expr) || false_but_the_compiler_does_not_know_it_)
extern int false_but_the_compiler_does_not_know_it_;

#ifdef CHECK_ASSERTION_SIDE_EFFECTS
#undef assert
extern int not_supposed_to_survive;
#define assert(expr) ((void)(not_supposed_to_survive || (expr)))
#endif

#endif

#ifdef DISABLE_SIGN_COMPARE_WARNINGS
DISABLE_WARNING(-Wsign-compare)
#endif
```

## Apply Minification Excerpt

```c
#ifndef GIT_COMPAT_UTIL_H
#define GIT_COMPAT_UTIL_H

#if __STDC_VERSION__ - 0 < 199901L


#error "Required C99 support is in a test phase.  Please see git-compat-util.h for more details."
#endif

#ifdef USE_MSVC_CRTDBG


#include <stdlib.h>
#include <crtdbg.h>
#endif

#include "compat/posix.h"

struct strbuf;

#if defined(__GNUC__) || defined(__clang__)
#  define PRAGMA(pragma)           _Pragma(#pragma)
#  define DISABLE_WARNING(warning) PRAGMA(GCC diagnostic ignored #warning)
#else
#  define DISABLE_WARNING(warning)
#endif

#undef FLEX_ARRAY
#define FLEX_ARRAY


#define BUILD_ASSERT_OR_ZERO(cond) \
	(sizeof(char [1 - 2*!(cond)]) - 1)

#if GIT_GNUC_PREREQ(3, 1)

# define BARF_UNLESS_AN_ARRAY(arr)						\
	BUILD_ASSERT_OR_ZERO(!__builtin_types_compatible_p(__typeof__(arr), \
							   __typeof__(&(arr)[0])))
# define BARF_UNLESS_COPYABLE(dst, src) \
	BUILD_ASSERT_OR_ZERO(__builtin_types_compatible_p(__typeof__(*(dst)), \
							  __typeof__(*(src))))

# define BARF_UNLESS_SIGNED(var)   BUILD_ASSERT_OR_ZERO(((__typeof__(var)) -1) < 0)
# define BARF_UNLESS_UNSIGNED(var) BUILD_ASSERT_OR_ZERO(((__typeof__(var)) -1) > 0)
#else
# define BARF_UNLESS_AN_ARRAY(arr) 0
# define BARF_UNLESS_COPYABLE(dst, src) \
	BUILD

... [truncated 18400 chars] ...

ember))


#if defined(__GNUC__)
#define OFFSETOF_VAR(ptr, member) offsetof(__typeof__(*ptr), member)
#else
#define OFFSETOF_VAR(ptr, member) \
	((uintptr_t)&(ptr)->member - (uintptr_t)(ptr))
#endif


#define NOT_CONSTANT(expr) ((expr) || false_but_the_compiler_does_not_know_it_)
extern int false_but_the_compiler_does_not_know_it_;

#ifdef CHECK_ASSERTION_SIDE_EFFECTS
#undef assert
extern int not_supposed_to_survive;
#define assert(expr) ((void)(not_supposed_to_survive || (expr)))
#endif

#endif

#ifdef DISABLE_SIGN_COMPARE_WARNINGS
DISABLE_WARNING(-Wsign-compare)
#endif
```

## Sync Minify Excerpt

```c
#ifndef GIT_COMPAT_UTIL_H
#define GIT_COMPAT_UTIL_H

#if __STDC_VERSION__ - 0 < 199901L


#error "Required C99 support is in a test phase.  Please see git-compat-util.h for more details."
#endif

#ifdef USE_MSVC_CRTDBG


#include <stdlib.h>
#include <crtdbg.h>
#endif

#include "compat/posix.h"

struct strbuf;

#if defined(__GNUC__) || defined(__clang__)
#  define PRAGMA(pragma)           _Pragma(#pragma)
#  define DISABLE_WARNING(warning) PRAGMA(GCC diagnostic ignored #warning)
#else
#  define DISABLE_WARNING(warning)
#endif

#undef FLEX_ARRAY
#define FLEX_ARRAY


#define BUILD_ASSERT_OR_ZERO(cond) \
	(sizeof(char [1 - 2*!(cond)]) - 1)

#if GIT_GNUC_PREREQ(3, 1)

# define BARF_UNLESS_AN_ARRAY(arr)						\
	BUILD_ASSERT_OR_ZERO(!__builtin_types_compatible_p(__typeof__(arr), \
							   __typeof__(&(arr)[0])))
# define BARF_UNLESS_COPYABLE(dst, src) \
	BUILD_ASSERT_OR_ZERO(__builtin_types_compatible_p(__typeof__(*(dst)), \
							  __typeof__(*(src))))

# define BARF_UNLESS_SIGNED(var)   BUILD_ASSERT_OR_ZERO(((__typeof__(var)) -1) < 0)
# define BARF_UNLESS_UNSIGNED(var) BUILD_ASSERT_OR_ZERO(((__typeof__(var)) -1) > 0)
#else
# define BARF_UNLESS_AN_ARRAY(arr) 0
# define BARF_UNLESS_COPYABLE(dst, src) \
	BUILD

... [truncated 18400 chars] ...

ember))


#if defined(__GNUC__)
#define OFFSETOF_VAR(ptr, member) offsetof(__typeof__(*ptr), member)
#else
#define OFFSETOF_VAR(ptr, member) \
	((uintptr_t)&(ptr)->member - (uintptr_t)(ptr))
#endif


#define NOT_CONSTANT(expr) ((expr) || false_but_the_compiler_does_not_know_it_)
extern int false_but_the_compiler_does_not_know_it_;

#ifdef CHECK_ASSERTION_SIDE_EFFECTS
#undef assert
extern int not_supposed_to_survive;
#define assert(expr) ((void)(not_supposed_to_survive || (expr)))
#endif

#endif

#ifdef DISABLE_SIGN_COMPARE_WARNINGS
DISABLE_WARNING(-Wsign-compare)
#endif
```

## Async Minify Excerpt

```c
#ifndef GIT_COMPAT_UTIL_H
#define GIT_COMPAT_UTIL_H

#if __STDC_VERSION__ - 0 < 199901L


#error "Required C99 support is in a test phase.  Please see git-compat-util.h for more details."
#endif

#ifdef USE_MSVC_CRTDBG


#include <stdlib.h>
#include <crtdbg.h>
#endif

#include "compat/posix.h"

struct strbuf;

#if defined(__GNUC__) || defined(__clang__)
#  define PRAGMA(pragma)           _Pragma(#pragma)
#  define DISABLE_WARNING(warning) PRAGMA(GCC diagnostic ignored #warning)
#else
#  define DISABLE_WARNING(warning)
#endif

#undef FLEX_ARRAY
#define FLEX_ARRAY


#define BUILD_ASSERT_OR_ZERO(cond) \
	(sizeof(char [1 - 2*!(cond)]) - 1)

#if GIT_GNUC_PREREQ(3, 1)

# define BARF_UNLESS_AN_ARRAY(arr)						\
	BUILD_ASSERT_OR_ZERO(!__builtin_types_compatible_p(__typeof__(arr), \
							   __typeof__(&(arr)[0])))
# define BARF_UNLESS_COPYABLE(dst, src) \
	BUILD_ASSERT_OR_ZERO(__builtin_types_compatible_p(__typeof__(*(dst)), \
							  __typeof__(*(src))))

# define BARF_UNLESS_SIGNED(var)   BUILD_ASSERT_OR_ZERO(((__typeof__(var)) -1) < 0)
# define BARF_UNLESS_UNSIGNED(var) BUILD_ASSERT_OR_ZERO(((__typeof__(var)) -1) > 0)
#else
# define BARF_UNLESS_AN_ARRAY(arr) 0
# define BARF_UNLESS_COPYABLE(dst, src) \
	BUILD

... [truncated 18400 chars] ...

ember))


#if defined(__GNUC__)
#define OFFSETOF_VAR(ptr, member) offsetof(__typeof__(*ptr), member)
#else
#define OFFSETOF_VAR(ptr, member) \
	((uintptr_t)&(ptr)->member - (uintptr_t)(ptr))
#endif


#define NOT_CONSTANT(expr) ((expr) || false_but_the_compiler_does_not_know_it_)
extern int false_but_the_compiler_does_not_know_it_;

#ifdef CHECK_ASSERTION_SIDE_EFFECTS
#undef assert
extern int not_supposed_to_survive;
#define assert(expr) ((void)(not_supposed_to_survive || (expr)))
#endif

#endif

#ifdef DISABLE_SIGN_COMPARE_WARNINGS
DISABLE_WARNING(-Wsign-compare)
#endif
```

## Symbols

```txt
   1| #ifndef GIT_COMPAT_UTIL_H
   2| #define GIT_COMPAT_UTIL_H
   4| #if __STDC_VERSION__ - 0 < 199901L
  14| #error "Required C99 support is in a test phase.  Please see git-compat-util.h for more details."
  15| #endif
  17| #ifdef USE_MSVC_CRTDBG
  22| #include <stdlib.h>
  23| #include <crtdbg.h>
  24| #endif
  26| #include "compat/posix.h"
  28| struct strbuf;
  30| #if defined(__GNUC__) || defined(__clang__)
  31| #  define PRAGMA(pragma)           _Pragma(#pragma)
  32| #  define DISABLE_WARNING(warning) PRAGMA(GCC diagnostic ignored #warning)
  33| #else
  34| #  define DISABLE_WARNING(warning)
  35| #endif
  37| #undef FLEX_ARRAY
  38| #define FLEX_ARRAY /* empty - weather balloon to require C99 FAM */
  52| #define BUILD_ASSERT_OR_ZERO(cond) \
  53| 	(sizeof(char [1 - 2*!(cond)]) - 1)
  55| #if GIT_GNUC_PREREQ(3, 1)
  57| # define BARF_UNLESS_AN_ARRAY(arr)						\
  58| 	BUILD_ASSERT_OR_ZERO(!__builtin_types_compatible_p(__typeof__(arr), \
  59| 							   __typeof__(&(arr)[0])))
  60| # define BARF_UNLESS_COPYABLE(dst, src) \
  61| 	BUILD_ASSERT_OR_ZERO(__builtin_types_compatible_p(__typeof__(*(dst)), \
  62| 							  __typeof__(*(src))))
  64| # define BARF_UNLESS_SIGNED(var)   BUILD_ASSERT_OR_ZERO(((__typeof__(var)) -1) < 0)
  65| # define BARF_UNLESS_UNSIGNED(var) BUILD_ASSERT_OR_ZERO(((__typeof__(var)) -1) > 0)
  66| #else
  67| # define BARF_UNLESS_AN_ARRAY(arr) 0
  68| # define BARF_UNLESS_COPYABLE(dst, src) \
  69| 	BUILD_ASSERT_OR_ZERO(0 ? ((*(dst) = *(src)), 0) : \
  70| 				 sizeof(*(dst)) == sizeof(*(src)))
  72| # define BARF_UNLESS_SIGNED(var)   0
  73| # define BARF_UNLESS_UNSIGNED(var) 0
  74| #endif
  84| #define ARRAY_SIZE(x) (sizeof(x) / sizeof((x)[0]) + BARF_UNLESS_AN_ARRAY(x))
  86| #define bitsizeof(x)  (CHA

... [truncated 16861 chars] ...

 \
1105| 	(type *)container_of_or_null_offset(ptr, offsetof(type, member))
1113| #if defined(__GNUC__) /* clang sets this, too */
1114| #define OFFSETOF_VAR(ptr, member) offsetof(__typeof__(*ptr), member)
1115| #else /* !__GNUC__ */
1116| #define OFFSETOF_VAR(ptr, member) \
1117| 	((uintptr_t)&(ptr)->member - (uintptr_t)(ptr))
1118| #endif /* !__GNUC__ */
1127| #define NOT_CONSTANT(expr) ((expr) || false_but_the_compiler_does_not_know_it_)
1128| extern int false_but_the_compiler_does_not_know_it_;
1130| #ifdef CHECK_ASSERTION_SIDE_EFFECTS
1131| #undef assert
1132| extern int not_supposed_to_survive;
1133| #define assert(expr) ((void)(not_supposed_to_survive || (expr)))
1134| #endif /* CHECK_ASSERTION_SIDE_EFFECTS */
1136| #endif
1138| #ifdef DISABLE_SIGN_COMPARE_WARNINGS
1139| DISABLE_WARNING(-Wsign-compare)
1140| #endif
```
