# C Header (.h)

Source sample: `h/git-compat-util.h`

Strategy: `conservative`

Agent rating: **8.7/10 (strong)**

Agent understanding from minified output: **9.9/10 (excellent)**

Artifacts:

- `raw/source.excerpt.txt`
- `minified/content-view.excerpt.txt`
- `minified/apply-minification.excerpt.txt`
- `minified/minify-content-sync.excerpt.txt`
- `minified/minify-content-async.excerpt.txt`
- `symbol/signatures.txt`

| Tool | Bytes | Cut | Time | Rating |
| --- | ---: | ---: | ---: | ---: |
| input | 33059 | - | - | - |
| content-view | 20155 | 39% | 5.036 ms | 8.5/10 |
| applyMinification | 20155 | 39% | 6.77 ms | 8.5/10 |
| sync minify | 20155 | 39% | 6.751 ms | 8.5/10 |
| async minify | 20155 | 39% | 8.46 ms | 8.5/10 |
| symbols | 12379 | 62.6% | 1.026 ms | 9/10 |

## Agent Understanding

Measured from `standard` minified output.

| Component | Score |
| --- | ---: |
| syntax anchors | 10/10 (3/3) |
| delimiter structure | 10/10 |
| output health | 10/10 |
| context budget | 9/10 |
| symbol context | 10/10 |
| signals passed | 6/6 |

## Agent Observation By Output Level

Ratings are computed from the actual raw, standard, minify, and symbol outputs
for this language sample.

| Level | Bytes | Cut | Agent observation | Syntax anchors | Structure |
| --- | ---: | ---: | ---: | ---: | ---: |
| none | 33059 | 0% | 10/10 excellent | 10/10 | 10/10 |
| standard | 20155 | 39% | 9.9/10 excellent | 10/10 | 10/10 |
| minify | 20155 | 39% | 9.9/10 excellent | 10/10 | 10/10 |
| symbols | 12379 | 62.6% | 9.6/10 excellent | 10/10 | 8.1/10 |

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

## Symbols

```txt
   2| #define GIT_COMPAT_UTIL_H
  22| #include <stdlib.h>
  23| #include <crtdbg.h>
  26| #include "compat/posix.h"
  28| struct strbuf;
  31| #  define PRAGMA(pragma)           _Pragma(#pragma)
  32| #  define DISABLE_WARNING(warning) PRAGMA(GCC diagnostic ignored #warning)
  34| #  define DISABLE_WARNING(warning)
  38| #define FLEX_ARRAY /* empty - weather balloon to require C99 FAM */
  52| #define BUILD_ASSERT_OR_ZERO(cond) \
  57| # define BARF_UNLESS_AN_ARRAY(arr)						\
  60| # define BARF_UNLESS_COPYABLE(dst, src) \
  64| # define BARF_UNLESS_SIGNED(var)   BUILD_ASSERT_OR_ZERO(((__typeof__(var)) -1) < 0)
  65| # define BARF_UNLESS_UNSIGNED(var) BUILD_ASSERT_OR_ZERO(((__typeof__(var)) -1) > 0)
  67| # define BARF_UNLESS_AN_ARRAY(arr) 0
  68| # define BARF_UNLESS_COPYABLE(dst, src) \
  72| # define BARF_UNLESS_SIGNED(var)   0
  73| # define BARF_UNLESS_UNSIGNED(var) 0
  84| #define ARRAY_SIZE(x) (sizeof(x) / sizeof((x)[0]) + BARF_UNLESS_AN_ARRAY(x))
  86| #define bitsizeof(x)  (CHAR_BIT * sizeof(x))
  88| #define maximum_signed_value_of_type(a) \
  91| #define maximum_unsigned_value_of_type(a) \
 100| #define signed_add_overflows(a, b) \
 103| #define unsigned_add_overflows(a, b) \
 111| #define unsigned_mult_overflows(a, b) \
 118| #define unsigned_left_shift_overflows(a, shift) \
 123| #define TYPEOF(x) (__typeof__(x))
 125| #define TYPEOF(x)
 128| #define MSB(x, bits) ((x) & TYPEOF(x)(~0ULL << (bitsizeof(x) - (bits))))
 129| #define HAS_MULTI_BITS(i)  ((i) & ((i) - 1))  /* checks if an integer has more than 1 bit set */
 131| #define DIV_ROUND_UP(n,d) (((n) + (d) - 1) / (d))
 134| #define decimal_length(x)	((int)(sizeof(x) * 2.56 + 0.5) + 1)
 137| static inline int _have_unix_sockets(void)
 145| #define have_unix_sockets _have_un

... [truncated 9779 chars] ...

fine FSYNC_METHOD_DEFAULT FSYNC_METHOD_FSYNC
1049| # define SHELL_PATH "/bin/sh"
1062| static inline int is_missing_file_error(int errno_)
1067| int cmd_main(int, const char **);
1073| int common_exit(const char *file, int line, int code);
1074| #define exit(code) exit(common_exit(__FILE__, __LINE__, (code)))
1080| #include "banned.h"
1089| #define container_of(ptr, type, member) \
1096| static inline void *container_of_or_null_offset(void *ptr, size_t offset)
1104| #define container_of_or_null(ptr, type, member) \
1114| #define OFFSETOF_VAR(ptr, member) offsetof(__typeof__(*ptr), member)
1116| #define OFFSETOF_VAR(ptr, member) \
1127| #define NOT_CONSTANT(expr) ((expr) || false_but_the_compiler_does_not_know_it_)
1133| #define assert(expr) ((void)(not_supposed_to_survive || (expr)))
1139| DISABLE_WARNING(-Wsign-compare)
```
