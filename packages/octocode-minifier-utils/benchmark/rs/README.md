# Rust (.rs)

Source sample: `rs/option.rs`

Strategy: `conservative`

Agent rating: **9.7/10 (excellent)**

Agent understanding from minified output: **10/10 (excellent)**

Artifacts:

- `raw/source.excerpt.txt`
- `minified/content-view.excerpt.txt`
- `minified/apply-minification.excerpt.txt`
- `minified/minify-content-sync.excerpt.txt`
- `minified/minify-content-async.excerpt.txt`
- `symbol/signatures.txt`

| Tool | Bytes | Cut | Time | Rating |
| --- | ---: | ---: | ---: | ---: |
| input | 100057 | - | - | - |
| content-view | 37827 | 62.2% | 10.425 ms | 9.5/10 |
| applyMinification | 37827 | 62.2% | 11.786 ms | 9.5/10 |
| sync minify | 37827 | 62.2% | 10.591 ms | 9.5/10 |
| async minify | 37827 | 62.2% | 14.865 ms | 9.5/10 |
| symbols | 7552 | 92.5% | 0.61 ms | 10/10 |

## Agent Understanding

Measured from `standard` minified output.

| Component | Score |
| --- | ---: |
| syntax anchors | 10/10 (3/3) |
| delimiter structure | 10/10 |
| output health | 10/10 |
| context budget | 10/10 |
| symbol context | 10/10 |
| signals passed | 6/6 |

## Agent Observation By Output Level

Ratings are computed from the actual raw, standard, minify, and symbol outputs
for this language sample.

| Level | Bytes | Cut | Agent observation | Syntax anchors | Structure |
| --- | ---: | ---: | ---: | ---: | ---: |
| none | 100057 | 0% | 10/10 excellent | 10/10 | 10/10 |
| standard | 37827 | 62.2% | 10/10 excellent | 10/10 | 10/10 |
| minify | 37827 | 62.2% | 10/10 excellent | 10/10 | 10/10 |
| symbols | 7552 | 92.5% | 9.2/10 excellent | 10/10 | 7.1/10 |

## Notes

- conservative text strategy.

## Before Excerpt

```rs
//! Optional values.
//!
//! Type [`Option`] represents an optional value: every [`Option`]
//! is either [`Some`] and contains a value, or [`None`], and
//! does not. [`Option`] types are very common in Rust code, as
//! they have a number of uses:
//!
//! * Initial values
//! * Return values for functions that are not defined
//!   over their entire input range (partial functions)
//! * Return value for otherwise reporting simple errors, where [`None`] is
//!   returned on error
//! * Optional struct fields
//! * Struct fields that can be loaned or "taken"
//! * Optional function arguments
//! * Nullable pointers
//! * Swapping things out of difficult situations
//!
//! [`Option`]s are commonly paired with pattern matching to query the presence
//! of a value and take action, always accounting for the [`None`] case.
//!
//! ```
//! fn divide(numerator: f64, denominator: f64) -> Option<f64> {
//!     if denominator == 0.0 {
//!         None
//!     } else {
//!         Some(numerator / denominator)
//!     }
//! }
//!
//! // The return value of the function is an option
//! let result = divide(2.0, 3.0);
//!
//! // Pattern match to retrieve the value
//! match result {
//!     // The division was valid
/

... [truncated 98257 chars] ...

mples
    ///
    /// ```
    /// #![feature(option_array_transpose)]
    /// # use std::option::Option;
    ///
    /// let data = [Some(0); 1000];
    /// let data: Option<[u8; 1000]> = data.transpose();
    /// assert_eq!(data, Some([0; 1000]));
    ///
    /// let data = [Some(0), None];
    /// let data: Option<[u8; 2]> = data.transpose();
    /// assert_eq!(data, None);
    /// ```
    #[inline]
    #[unstable(feature = "option_array_transpose", issue = "130828")]
    pub fn transpose(self) -> Option<[T; N]> {
        self.try_map(core::convert::identity)
    }
}

```

## Content-View Excerpt

```rs
#![stable(feature = "rust1", since = "1.0.0")]

use crate::clone::TrivialClone;
use crate::iter::{self, FusedIterator, TrustedLen};
use crate::marker::Destruct;
use crate::ops::{self, ControlFlow, Deref, DerefMut, Residual, Try};
use crate::panicking::{panic, panic_display};
use crate::pin::Pin;
use crate::{cmp, convert, hint, mem, slice};

#[doc(search_unbox)]
#[derive(Copy, Debug, Hash)]
#[derive_const(Eq)]
#[rustc_diagnostic_item = "Option"]
#[lang = "Option"]
#[stable(feature = "rust1", since = "1.0.0")]
#[allow(clippy::derived_hash_with_manual_eq)]
pub enum Option<T> {

    #[lang = "None"]
    #[stable(feature = "rust1", since = "1.0.0")]
    None,

    #[lang = "Some"]
    #[stable(feature = "rust1", since = "1.0.0")]
    Some(#[stable(feature = "rust1", since = "1.0.0")] T),
}

impl<T> Option<T> {

    #[must_use = "if you intended to assert that this has a value, consider `.unwrap()` instead"]
    #[inline]
    #[stable(feature = "rust1", since = "1.0.0")]
    #[rustc_const_stable(feature = "const_option_basics", since = "1.48.0")]
    pub const fn is_some(&self) -> bool {
        matches!(*self, Some(_))
    }

    #[must_use]
    #[inline]
    #[stable(feature = "is_some_and", since = "1.70.0")

... [truncated 36027 chars] ...

et x: Option<&mut Option<u32>> = None;
    /// assert_eq!(None, x.flatten_mut());
    /// ```
    #[inline]
    #[unstable(feature = "option_reference_flattening", issue = "149221")]
    pub const fn flatten_mut(self) -> Option<&'a mut T> {
        match self {
            Some(inner) => inner.as_mut(),
            None => None,
        }
    }
}

impl<T, const N: usize> [Option<T>; N] {

    #[inline]
    #[unstable(feature = "option_array_transpose", issue = "130828")]
    pub fn transpose(self) -> Option<[T; N]> {
        self.try_map(core::convert::identity)
    }
}
```

## Apply Minification Excerpt

```rs
#![stable(feature = "rust1", since = "1.0.0")]

use crate::clone::TrivialClone;
use crate::iter::{self, FusedIterator, TrustedLen};
use crate::marker::Destruct;
use crate::ops::{self, ControlFlow, Deref, DerefMut, Residual, Try};
use crate::panicking::{panic, panic_display};
use crate::pin::Pin;
use crate::{cmp, convert, hint, mem, slice};

#[doc(search_unbox)]
#[derive(Copy, Debug, Hash)]
#[derive_const(Eq)]
#[rustc_diagnostic_item = "Option"]
#[lang = "Option"]
#[stable(feature = "rust1", since = "1.0.0")]
#[allow(clippy::derived_hash_with_manual_eq)]
pub enum Option<T> {

    #[lang = "None"]
    #[stable(feature = "rust1", since = "1.0.0")]
    None,

    #[lang = "Some"]
    #[stable(feature = "rust1", since = "1.0.0")]
    Some(#[stable(feature = "rust1", since = "1.0.0")] T),
}

impl<T> Option<T> {

    #[must_use = "if you intended to assert that this has a value, consider `.unwrap()` instead"]
    #[inline]
    #[stable(feature = "rust1", since = "1.0.0")]
    #[rustc_const_stable(feature = "const_option_basics", since = "1.48.0")]
    pub const fn is_some(&self) -> bool {
        matches!(*self, Some(_))
    }

    #[must_use]
    #[inline]
    #[stable(feature = "is_some_and", since = "1.70.0")

... [truncated 36027 chars] ...

et x: Option<&mut Option<u32>> = None;
    /// assert_eq!(None, x.flatten_mut());
    /// ```
    #[inline]
    #[unstable(feature = "option_reference_flattening", issue = "149221")]
    pub const fn flatten_mut(self) -> Option<&'a mut T> {
        match self {
            Some(inner) => inner.as_mut(),
            None => None,
        }
    }
}

impl<T, const N: usize> [Option<T>; N] {

    #[inline]
    #[unstable(feature = "option_array_transpose", issue = "130828")]
    pub fn transpose(self) -> Option<[T; N]> {
        self.try_map(core::convert::identity)
    }
}
```

## Sync Minify Excerpt

```rs
#![stable(feature = "rust1", since = "1.0.0")]

use crate::clone::TrivialClone;
use crate::iter::{self, FusedIterator, TrustedLen};
use crate::marker::Destruct;
use crate::ops::{self, ControlFlow, Deref, DerefMut, Residual, Try};
use crate::panicking::{panic, panic_display};
use crate::pin::Pin;
use crate::{cmp, convert, hint, mem, slice};

#[doc(search_unbox)]
#[derive(Copy, Debug, Hash)]
#[derive_const(Eq)]
#[rustc_diagnostic_item = "Option"]
#[lang = "Option"]
#[stable(feature = "rust1", since = "1.0.0")]
#[allow(clippy::derived_hash_with_manual_eq)]
pub enum Option<T> {

    #[lang = "None"]
    #[stable(feature = "rust1", since = "1.0.0")]
    None,

    #[lang = "Some"]
    #[stable(feature = "rust1", since = "1.0.0")]
    Some(#[stable(feature = "rust1", since = "1.0.0")] T),
}

impl<T> Option<T> {

    #[must_use = "if you intended to assert that this has a value, consider `.unwrap()` instead"]
    #[inline]
    #[stable(feature = "rust1", since = "1.0.0")]
    #[rustc_const_stable(feature = "const_option_basics", since = "1.48.0")]
    pub const fn is_some(&self) -> bool {
        matches!(*self, Some(_))
    }

    #[must_use]
    #[inline]
    #[stable(feature = "is_some_and", since = "1.70.0")

... [truncated 36027 chars] ...

et x: Option<&mut Option<u32>> = None;
    /// assert_eq!(None, x.flatten_mut());
    /// ```
    #[inline]
    #[unstable(feature = "option_reference_flattening", issue = "149221")]
    pub const fn flatten_mut(self) -> Option<&'a mut T> {
        match self {
            Some(inner) => inner.as_mut(),
            None => None,
        }
    }
}

impl<T, const N: usize> [Option<T>; N] {

    #[inline]
    #[unstable(feature = "option_array_transpose", issue = "130828")]
    pub fn transpose(self) -> Option<[T; N]> {
        self.try_map(core::convert::identity)
    }
}
```

## Async Minify Excerpt

```rs
#![stable(feature = "rust1", since = "1.0.0")]

use crate::clone::TrivialClone;
use crate::iter::{self, FusedIterator, TrustedLen};
use crate::marker::Destruct;
use crate::ops::{self, ControlFlow, Deref, DerefMut, Residual, Try};
use crate::panicking::{panic, panic_display};
use crate::pin::Pin;
use crate::{cmp, convert, hint, mem, slice};

#[doc(search_unbox)]
#[derive(Copy, Debug, Hash)]
#[derive_const(Eq)]
#[rustc_diagnostic_item = "Option"]
#[lang = "Option"]
#[stable(feature = "rust1", since = "1.0.0")]
#[allow(clippy::derived_hash_with_manual_eq)]
pub enum Option<T> {

    #[lang = "None"]
    #[stable(feature = "rust1", since = "1.0.0")]
    None,

    #[lang = "Some"]
    #[stable(feature = "rust1", since = "1.0.0")]
    Some(#[stable(feature = "rust1", since = "1.0.0")] T),
}

impl<T> Option<T> {

    #[must_use = "if you intended to assert that this has a value, consider `.unwrap()` instead"]
    #[inline]
    #[stable(feature = "rust1", since = "1.0.0")]
    #[rustc_const_stable(feature = "const_option_basics", since = "1.48.0")]
    pub const fn is_some(&self) -> bool {
        matches!(*self, Some(_))
    }

    #[must_use]
    #[inline]
    #[stable(feature = "is_some_and", since = "1.70.0")

... [truncated 36027 chars] ...

et x: Option<&mut Option<u32>> = None;
    /// assert_eq!(None, x.flatten_mut());
    /// ```
    #[inline]
    #[unstable(feature = "option_reference_flattening", issue = "149221")]
    pub const fn flatten_mut(self) -> Option<&'a mut T> {
        match self {
            Some(inner) => inner.as_mut(),
            None => None,
        }
    }
}

impl<T, const N: usize> [Option<T>; N] {

    #[inline]
    #[unstable(feature = "option_array_transpose", issue = "130828")]
    pub fn transpose(self) -> Option<[T; N]> {
        self.try_map(core::convert::identity)
    }
}
```

## Symbols

```txt
 581| use crate::clone::TrivialClone;
 582| use crate::iter::{self, FusedIterator, TrustedLen};
 583| use crate::marker::Destruct;
 584| use crate::ops::{self, ControlFlow, Deref, DerefMut, Residual, Try};
 585| use crate::panicking::{panic, panic_display};
 586| use crate::pin::Pin;
 587| use crate::{cmp, convert, hint, mem, slice};
 597| pub enum Option<T> {
 632|     pub const fn is_some(&self) -> bool {
 658|     pub const fn is_some_and(self, f: impl [const] FnOnce(T) -> bool + [const] Destruct) -> bool {
 681|     pub const fn is_none(&self) -> bool {
 707|     pub const fn is_none_or(self, f: impl [const] FnOnce(T) -> bool + [const] Destruct) -> bool {
 741|     pub const fn as_ref(&self) -> Option<&T> {
 763|     pub const fn as_mut(&mut self) -> Option<&mut T> {
 777|     pub const fn as_pin_ref(self: Pin<&Self>) -> Option<Pin<&T>> {
 794|     pub const fn as_pin_mut(self: Pin<&mut Self>) -> Option<Pin<&mut T>> {
 807|     const fn len(&self) -> usize {
 841|     pub const fn as_slice(&self) -> &[T] {
 896|     pub const fn as_mut_slice(&mut self) -> &mut [T] {
 965|     pub const fn expect(self, msg: &str) -> T {
1010|     pub const fn unwrap(self) -> T {
1035|     pub const fn unwrap_or(self, default: T) -> T
1058|     pub const fn unwrap_or_else<F>(self, f: F) -> T
1090|     pub const fn unwrap_or_default(self) -> T
1125|     pub const unsafe fn unwrap_unchecked(self) -> T {
1157|     pub const fn map<U, F>(self, f: F) -> Option<U>
1188|     pub const fn inspect<F>(self, f: F) -> Self
1221|     pub const fn map_or<U, F>(self, default: U, f: F) -> U
1268|     pub const fn map_or_else<U, D, F>(self, default: D, f: F) -> U
1297|     pub const fn map_or_default<U, F>(self, f: F) -> U
1332|     pub const fn ok_or<E: [const] Destruc

... [truncated 4952 chars] ...

nfallible>;
2772|     fn from_output(output: Self::Output) -> Self {
2777|     fn branch(self) -> ControlFlow<Self::Residual, Self::Output> {
2789| const impl<T> ops::FromResidual<Option<convert::Infallible>> for Option<T> {
2791|     fn from_residual(residual: Option<convert::Infallible>) -> Self {
2801| const impl<T> ops::FromResidual<ops::Yeet<()>> for Option<T> {
2803|     fn from_residual(ops::Yeet(()): ops::Yeet<()>) -> Self {
2810| const impl<T> ops::Residual<T> for Option<convert::Infallible> {
2811|     type TryType = Option<T>;
2843|     pub const fn flatten(self) -> Option<T> {
2873|     pub const fn flatten_ref(self) -> Option<&'a T> {
2904|     pub const fn flatten_ref(self) -> Option<&'a T> {
2933|     pub const fn flatten_mut(self) -> Option<&'a mut T> {
2960|     pub fn transpose(self) -> Option<[T; N]> {
```
