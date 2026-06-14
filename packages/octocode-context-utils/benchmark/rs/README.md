# Rust (.rs)

Source sample: `rs/option.rs`

Strategy: `conservative`

| Tool | Bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| input | 100057 | - | - |
| content-view | 37827 | 62.2% | 1.532 ms |
| applyMinification | 37906 | 62.1% | 1.454 ms |
| sync minify | 37906 | 62.1% | 1.356 ms |
| async minify | 37906 | 62.1% | 1.417 ms |
| symbols | 33924 | 66.1% | 16.403 ms |

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
    #[stable(feature = "is_some_and", since = "1.

... [truncated 36106 chars] ...

t x: Option<&mut Option<u32>> = None;
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
    #[stable(feature = "is_some_and", since = "1.

... [truncated 36106 chars] ...

t x: Option<&mut Option<u32>> = None;
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
    #[stable(feature = "is_some_and", since = "1.

... [truncated 36106 chars] ...

t x: Option<&mut Option<u32>> = None;
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
 579| #![stable(feature = "rust1", since = "1.0.0")]
 581| use crate::clone::TrivialClone;
 582| use crate::iter::{self, FusedIterator, TrustedLen};
 583| use crate::marker::Destruct;
 584| use crate::ops::{self, ControlFlow, Deref, DerefMut, Residual, Try};
 585| use crate::panicking::{panic, panic_display};
 586| use crate::pin::Pin;
 587| use crate::{cmp, convert, hint, mem, slice};
 590| #[doc(search_unbox)]
 591| #[derive(Copy, Debug, Hash)]
 592| #[derive_const(Eq)]
 593| #[rustc_diagnostic_item = "Option"]
 594| #[lang = "Option"]
 595| #[stable(feature = "rust1", since = "1.0.0")]
 596| #[allow(clippy::derived_hash_with_manual_eq)] // PartialEq is manually implemented equivalently
 597| pub enum Option<T> {
 599|     #[lang = "None"]
 600|     #[stable(feature = "rust1", since = "1.0.0")]
 601|     None,
 603|     #[lang = "Some"]
 604|     #[stable(feature = "rust1", since = "1.0.0")]
 605|     Some(#[stable(feature = "rust1", since = "1.0.0")] T),
 606| }
 612| impl<T> Option<T> {
 628|     #[must_use = "if you intended to assert that this has a value, consider `.unwrap()` instead"]
 629|     #[inline]
 630|     #[stable(feature = "rust1", since = "1.0.0")]
 631|     #[rustc_const_stable(feature = "const_option_basics", since = "1.48.0")]
 632|     pub const fn is_some(&self) -> bool {
 654|     #[must_use]
 655|     #[inline]
 656|     #[stable(feature = "is_some_and", since = "1.70.0")]
 657|     #[rustc_const_unstable(feature = "const_option_ops", issue = "143956")]
 658|     pub const fn is_some_and(self, f: impl [const] FnOnce(T) -> bool + [const] Destruct) -> bool {
 659|         match self {
 660|             None => false,
 661|             Some(x) => f(x),
 662|         }
 663|     }
 676|     #[must_use = "if you inten

... [truncated 31324 chars] ...

 flatten(self) -> Option<T> {
2850| }
2852| impl<'a, T> Option<&'a Option<T>> {
2871|     #[inline]
2872|     #[unstable(feature = "option_reference_flattening", issue = "149221")]
2873|     pub const fn flatten_ref(self) -> Option<&'a T> {
2879| }
2881| impl<'a, T> Option<&'a mut Option<T>> {
2902|     #[inline]
2903|     #[unstable(feature = "option_reference_flattening", issue = "149221")]
2904|     pub const fn flatten_ref(self) -> Option<&'a T> {
2931|     #[inline]
2932|     #[unstable(feature = "option_reference_flattening", issue = "149221")]
2933|     pub const fn flatten_mut(self) -> Option<&'a mut T> {
2939| }
2941| impl<T, const N: usize> [Option<T>; N] {
2958|     #[inline]
2959|     #[unstable(feature = "option_array_transpose", issue = "130828")]
2960|     pub fn transpose(self) -> Option<[T; N]> {
2963| }
```
