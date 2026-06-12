# Haskell (.hs)

Source sample: `hs/cabal-simple.hs`

Strategy: `conservative`

Agent rating: **7/10 (good)**

Agent understanding from minified output: **9.3/10 (excellent)**

Artifacts:

- `raw/source.excerpt.txt`
- `minified/content-view.excerpt.txt`
- `minified/apply-minification.excerpt.txt`
- `minified/minify-content-sync.excerpt.txt`
- `minified/minify-content-async.excerpt.txt`
- `symbol/signatures.txt`

| Tool | Bytes | Cut | Time | Rating |
| --- | ---: | ---: | ---: | ---: |
| input | 41400 | - | - | - |
| content-view | 36301 | 12.3% | 10.696 ms | 7/10 |
| applyMinification | 36301 | 12.3% | 13.298 ms | 7/10 |
| sync minify | 36301 | 12.3% | 17.284 ms | 7/10 |
| async minify | 36301 | 12.3% | 7.037 ms | 7/10 |
| symbols | n/a | n/a | 0.015 ms | n/a |

## Agent Understanding

Measured from `standard` minified output.

| Component | Score |
| --- | ---: |
| syntax anchors | 10/10 (3/3) |
| delimiter structure | 10/10 |
| output health | 9/10 |
| context budget | 8/10 |
| symbol context | 7/10 |
| signals passed | 6/6 |

## Agent Observation By Output Level

Ratings are computed from the actual raw, standard, minify, and symbol outputs
for this language sample.

| Level | Bytes | Cut | Agent observation | Syntax anchors | Structure |
| --- | ---: | ---: | ---: | ---: | ---: |
| none | 41400 | 0% | 10/10 excellent | 10/10 | 10/10 |
| standard | 36301 | 12.3% | 9.3/10 excellent | 10/10 | 10/10 |
| minify | 36301 | 12.3% | 9.3/10 excellent | 10/10 | 10/10 |
| symbols | n/a | n/a | n/a | n/a | n/a |

## Notes

- conservative text strategy.
- symbols are not implemented for this extension.

## Before Excerpt

```hs
{-# LANGUAGE DataKinds #-}
{-# LANGUAGE DuplicateRecordFields #-}
{-# LANGUAGE FlexibleContexts #-}
{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE RankNTypes #-}
{-# LANGUAGE ScopedTypeVariables #-}
{-# LANGUAGE TypeApplications #-}
-----------------------------------------------------------------------------
{-
Work around this warning:
libraries/Cabal/Distribution/Simple.hs:78:0:
    Warning: In the use of `runTests'
             (imported from Distribution.Simple.UserHooks):
             Deprecated: "Please use the new testing interface instead!"
-}
{-# OPTIONS_GHC -Wno-deprecations #-}

-- |
-- Module      :  Distribution.Simple
-- Copyright   :  Isaac Jones 2003-2005
-- License     :  BSD3
--
-- Maintainer  :  cabal-devel@haskell.org
-- Portability :  portable
--
-- This is the command line front end to the Simple build system. When given
-- the parsed command-line args and package information, is able to perform
-- basic commands like configure, build, install, register, etc.
--
-- This module exports the main functions that Setup.hs scripts use. It
-- re-exports the 'UserHooks' type, the standard entry points like
-- 'defaultMain' and 'defaultMainWithHooks' and the predefined sets of
-- 'UserHooks

... [truncated 39600 chars] ...


      (allSuffixHandlers hooks)
      args

defaultRegHook
  :: VerbosityHandles
  -> PackageDescription
  -> LocalBuildInfo
  -> UserHooks
  -> RegisterFlags
  -> IO ()
defaultRegHook verbHandles pkg_descr localbuildinfo _ flags
  | hasLibs pkg_descr =
      registerWithHandles verbHandles pkg_descr localbuildinfo flags
  | otherwise =
      setupMessage
        verbosity
        "Package contains no library to register:"
        (packageId pkg_descr)
  where
    verbosity =
      mkVerbosity verbHandles $
        fromFlag (setupVerbosity $ registerCommonFlags flags)

```

## Content-View Excerpt

```hs
module Distribution.Simple
  ( module Distribution.Package
  , module Distribution.Version
  , module Distribution.License
  , module Distribution.Simple.Compiler
  , module Language.Haskell.Extension

  , defaultMain
  , defaultMainNoRead
  , defaultMainArgs
  , defaultMainArgsWithHandles

  , UserHooks (..)
  , Args
  , defaultMainWithHooks
  , defaultMainWithSetupHooks
  , defaultMainWithSetupHooksArgs
  , defaultMainWithHooksArgs
  , defaultMainWithHooksNoRead
  , defaultMainWithHooksNoReadArgs

  , simpleUserHooks
  , simpleUserHooksWithHandles
  , autoconfUserHooks
  , autoconfSetupHooks
  , emptyUserHooks

  , configureAction
  , buildAction
  , replAction
  , installAction
  , copyAction
  , haddockAction
  , cleanAction
  , sdistAction
  , hscolourAction
  , registerAction
  , unregisterAction
  , testAction
  , benchAction
  ) where

import Control.Exception (try)

import Distribution.Compat.Prelude
import Distribution.Compat.ResponseFile (expandResponse)
import Prelude ()

import Distribution.Package
import Distribution.PackageDescription
import Distribution.PackageDescription.Configuration
import Distribution.Simple.Command
import Distribution.Simple.Compiler
import Distribution.Simple.Package

... [truncated 34501 chars] ...

s
      (allSuffixHandlers hooks)
      args

defaultRegHook
  :: VerbosityHandles
  -> PackageDescription
  -> LocalBuildInfo
  -> UserHooks
  -> RegisterFlags
  -> IO ()
defaultRegHook verbHandles pkg_descr localbuildinfo _ flags
  | hasLibs pkg_descr =
      registerWithHandles verbHandles pkg_descr localbuildinfo flags
  | otherwise =
      setupMessage
        verbosity
        "Package contains no library to register:"
        (packageId pkg_descr)
  where
    verbosity =
      mkVerbosity verbHandles $
        fromFlag (setupVerbosity $ registerCommonFlags flags)
```

## Apply Minification Excerpt

```hs
module Distribution.Simple
  ( module Distribution.Package
  , module Distribution.Version
  , module Distribution.License
  , module Distribution.Simple.Compiler
  , module Language.Haskell.Extension

  , defaultMain
  , defaultMainNoRead
  , defaultMainArgs
  , defaultMainArgsWithHandles

  , UserHooks (..)
  , Args
  , defaultMainWithHooks
  , defaultMainWithSetupHooks
  , defaultMainWithSetupHooksArgs
  , defaultMainWithHooksArgs
  , defaultMainWithHooksNoRead
  , defaultMainWithHooksNoReadArgs

  , simpleUserHooks
  , simpleUserHooksWithHandles
  , autoconfUserHooks
  , autoconfSetupHooks
  , emptyUserHooks

  , configureAction
  , buildAction
  , replAction
  , installAction
  , copyAction
  , haddockAction
  , cleanAction
  , sdistAction
  , hscolourAction
  , registerAction
  , unregisterAction
  , testAction
  , benchAction
  ) where

import Control.Exception (try)

import Distribution.Compat.Prelude
import Distribution.Compat.ResponseFile (expandResponse)
import Prelude ()

import Distribution.Package
import Distribution.PackageDescription
import Distribution.PackageDescription.Configuration
import Distribution.Simple.Command
import Distribution.Simple.Compiler
import Distribution.Simple.Package

... [truncated 34501 chars] ...

s
      (allSuffixHandlers hooks)
      args

defaultRegHook
  :: VerbosityHandles
  -> PackageDescription
  -> LocalBuildInfo
  -> UserHooks
  -> RegisterFlags
  -> IO ()
defaultRegHook verbHandles pkg_descr localbuildinfo _ flags
  | hasLibs pkg_descr =
      registerWithHandles verbHandles pkg_descr localbuildinfo flags
  | otherwise =
      setupMessage
        verbosity
        "Package contains no library to register:"
        (packageId pkg_descr)
  where
    verbosity =
      mkVerbosity verbHandles $
        fromFlag (setupVerbosity $ registerCommonFlags flags)
```

## Sync Minify Excerpt

```hs
module Distribution.Simple
  ( module Distribution.Package
  , module Distribution.Version
  , module Distribution.License
  , module Distribution.Simple.Compiler
  , module Language.Haskell.Extension

  , defaultMain
  , defaultMainNoRead
  , defaultMainArgs
  , defaultMainArgsWithHandles

  , UserHooks (..)
  , Args
  , defaultMainWithHooks
  , defaultMainWithSetupHooks
  , defaultMainWithSetupHooksArgs
  , defaultMainWithHooksArgs
  , defaultMainWithHooksNoRead
  , defaultMainWithHooksNoReadArgs

  , simpleUserHooks
  , simpleUserHooksWithHandles
  , autoconfUserHooks
  , autoconfSetupHooks
  , emptyUserHooks

  , configureAction
  , buildAction
  , replAction
  , installAction
  , copyAction
  , haddockAction
  , cleanAction
  , sdistAction
  , hscolourAction
  , registerAction
  , unregisterAction
  , testAction
  , benchAction
  ) where

import Control.Exception (try)

import Distribution.Compat.Prelude
import Distribution.Compat.ResponseFile (expandResponse)
import Prelude ()

import Distribution.Package
import Distribution.PackageDescription
import Distribution.PackageDescription.Configuration
import Distribution.Simple.Command
import Distribution.Simple.Compiler
import Distribution.Simple.Package

... [truncated 34501 chars] ...

s
      (allSuffixHandlers hooks)
      args

defaultRegHook
  :: VerbosityHandles
  -> PackageDescription
  -> LocalBuildInfo
  -> UserHooks
  -> RegisterFlags
  -> IO ()
defaultRegHook verbHandles pkg_descr localbuildinfo _ flags
  | hasLibs pkg_descr =
      registerWithHandles verbHandles pkg_descr localbuildinfo flags
  | otherwise =
      setupMessage
        verbosity
        "Package contains no library to register:"
        (packageId pkg_descr)
  where
    verbosity =
      mkVerbosity verbHandles $
        fromFlag (setupVerbosity $ registerCommonFlags flags)
```

## Async Minify Excerpt

```hs
module Distribution.Simple
  ( module Distribution.Package
  , module Distribution.Version
  , module Distribution.License
  , module Distribution.Simple.Compiler
  , module Language.Haskell.Extension

  , defaultMain
  , defaultMainNoRead
  , defaultMainArgs
  , defaultMainArgsWithHandles

  , UserHooks (..)
  , Args
  , defaultMainWithHooks
  , defaultMainWithSetupHooks
  , defaultMainWithSetupHooksArgs
  , defaultMainWithHooksArgs
  , defaultMainWithHooksNoRead
  , defaultMainWithHooksNoReadArgs

  , simpleUserHooks
  , simpleUserHooksWithHandles
  , autoconfUserHooks
  , autoconfSetupHooks
  , emptyUserHooks

  , configureAction
  , buildAction
  , replAction
  , installAction
  , copyAction
  , haddockAction
  , cleanAction
  , sdistAction
  , hscolourAction
  , registerAction
  , unregisterAction
  , testAction
  , benchAction
  ) where

import Control.Exception (try)

import Distribution.Compat.Prelude
import Distribution.Compat.ResponseFile (expandResponse)
import Prelude ()

import Distribution.Package
import Distribution.PackageDescription
import Distribution.PackageDescription.Configuration
import Distribution.Simple.Command
import Distribution.Simple.Compiler
import Distribution.Simple.Package

... [truncated 34501 chars] ...

s
      (allSuffixHandlers hooks)
      args

defaultRegHook
  :: VerbosityHandles
  -> PackageDescription
  -> LocalBuildInfo
  -> UserHooks
  -> RegisterFlags
  -> IO ()
defaultRegHook verbHandles pkg_descr localbuildinfo _ flags
  | hasLibs pkg_descr =
      registerWithHandles verbHandles pkg_descr localbuildinfo flags
  | otherwise =
      setupMessage
        verbosity
        "Package contains no library to register:"
        (packageId pkg_descr)
  where
    verbosity =
      mkVerbosity verbHandles $
        fromFlag (setupVerbosity $ registerCommonFlags flags)
```

## Symbols

```txt
No symbols returned for this sample.
```
