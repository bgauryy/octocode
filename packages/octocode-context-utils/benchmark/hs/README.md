# Haskell (.hs)

Source sample: `hs/cabal-simple.hs`

Strategy: `conservative`

| Tool | Bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| input | 41400 | - | - |
| content-view | 36301 | 12.3% | 1.091 ms |
| applyMinification | 36334 | 12.2% | 1.19 ms |
| sync minify | 36334 | 12.2% | 1.547 ms |
| async minify | 36334 | 12.2% | 1.161 ms |
| symbols | 7478 | 81.9% | 0.082 ms |

## Notes

- conservative text strategy.

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
import Distribution.Simple.

... [truncated 34534 chars] ...

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
import Distribution.Simple.

... [truncated 34534 chars] ...

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
import Distribution.Simple.

... [truncated 34534 chars] ...

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
  10| Work around this warning:
  11| libraries/Cabal/Distribution/Simple.hs:78:0:
  15| -}
  42| module Distribution.Simple
  88| import Control.Exception (try)
  90| import Distribution.Compat.Prelude
  91| import Distribution.Compat.ResponseFile (expandResponse)
  92| import Prelude ()
  96| import Distribution.Package
  97| import Distribution.PackageDescription
  98| import Distribution.PackageDescription.Configuration
  99| import Distribution.Simple.Command
 100| import Distribution.Simple.Compiler
 101| import Distribution.Simple.PackageDescription
 102| import Distribution.Simple.PreProcess
 103| import Distribution.Simple.Program
 104| import Distribution.Simple.Setup
 105| import qualified Distribution.Simple.SetupHooks.Internal as SetupHooks
 106| import Distribution.Simple.UserHooks
 108| import Distribution.Simple.Build
 109| import Distribution.Simple.Register
 110| import Distribution.Simple.SrcDist
 112| import Distribution.Simple.Configure
 114| import Distribution.License
 115| import Distribution.Pretty
 116| import Distribution.Simple.Bench
 117| import Distribution.Simple.BuildPaths
 118| import Distribution.Simple.ConfigureScript (runConfigureScript)
 119| import Distribution.Simple.Errors
 120| import Distribution.Simple.Haddock
 121| import Distribution.Simple.Install
 122| import Distribution.Simple.LocalBuildInfo
 123| import Distribution.Simple.SetupHooks.Internal
 126| import Distribution.Simple.Test
 127| import Distribution.Simple.Utils
 128| import qualified Distribution.Types.LocalBuildConfig as LBC
 129| import Distribution.Utils.Path
 130| import Distribution.Verbosity
 131| import Distribution.Version
 132| import Language.Haskell.Extension
 135| import Data.List (unionBy, (\\))
 136| import System.Dire

... [truncated 4878 chars] ...

ooks =
1038| getHookedBuildInfo
1043| getHookedBuildInfo verbosity mbWorkDir build_dir = do
1051| autoconfSetupHooks :: SetupHooks
1052| autoconfSetupHooks =
1110| defaultTestHook
1118| defaultTestHook verbHandles args pkg_descr localbuildinfo _ flags =
1121| defaultBenchHook
1129| defaultBenchHook verbHandles args pkg_descr localbuildinfo _ flags =
1132| defaultInstallHook
1139| defaultInstallHook verbHandles =
1142| defaultInstallHook_setupHooks
1150| defaultInstallHook_setupHooks inst_hooks verbHandles pkg_descr localbuildinfo _ flags = do
1166| defaultBuildHook
1173| defaultBuildHook verbHandles pkg_descr localbuildinfo hooks flags =
1183| defaultReplHook
1191| defaultReplHook verbHandles pkg_descr localbuildinfo hooks flags args =
1202| defaultRegHook
1209| defaultRegHook verbHandles pkg_descr localbuildinfo _ flags
```
