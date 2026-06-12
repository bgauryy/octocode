# C# (.cs)

Source sample: `cs/00-dotnet-argument-exception.cs`

Strategy: `conservative`

Agent rating: **8.3/10 (strong)**

Agent understanding from minified output: **9.7/10 (excellent)**

Artifacts:

- `raw/source.excerpt.txt`
- `minified/content-view.excerpt.txt`
- `minified/apply-minification.excerpt.txt`
- `minified/minify-content-sync.excerpt.txt`
- `minified/minify-content-async.excerpt.txt`
- `symbol/signatures.txt`

| Tool | Bytes | Cut | Time | Rating |
| --- | ---: | ---: | ---: | ---: |
| input | 5603 | - | - | - |
| content-view | 4017 | 28.3% | 1.304 ms | 8.5/10 |
| applyMinification | 4024 | 28.2% | 1.317 ms | 8.5/10 |
| sync minify | 4024 | 28.2% | 1.225 ms | 8.5/10 |
| async minify | 4024 | 28.2% | 1.308 ms | 8.5/10 |
| symbols | 2649 | 52.7% | 52.889 ms | 8/10 |

## Agent Understanding

Measured from `standard` minified output.

| Component | Score |
| --- | ---: |
| syntax anchors | 10/10 (3/3) |
| delimiter structure | 10/10 |
| output health | 9/10 |
| context budget | 9/10 |
| symbol context | 10/10 |
| signals passed | 6/6 |

## Agent Observation By Output Level

Ratings are computed from the actual raw, standard, minify, and symbol outputs
for this language sample.

| Level | Bytes | Cut | Agent observation | Syntax anchors | Structure |
| --- | ---: | ---: | ---: | ---: | ---: |
| none | 5603 | 0% | 10/10 excellent | 10/10 | 10/10 |
| standard | 4017 | 28.3% | 9.7/10 excellent | 10/10 | 10/10 |
| minify | 4024 | 28.2% | 9.7/10 excellent | 10/10 | 10/10 |
| symbols | 2649 | 52.7% | 8/10 strong | 6.7/10 | 7.7/10 |

## Notes

- conservative text strategy.

## Before Excerpt

```cs
// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.

using System.ComponentModel;
using System.Diagnostics.CodeAnalysis;
using System.Runtime.CompilerServices;
using System.Runtime.Serialization;

namespace System
{
    /// <summary>
    /// The exception that is thrown when one of the arguments provided to a method is not valid.
    /// </summary>
    [Serializable]
    [TypeForwardedFrom("mscorlib, Version=4.0.0.0, Culture=neutral, PublicKeyToken=b77a5c561934e089")]
    public class ArgumentException : SystemException
    {
        private readonly string? _paramName;

        // Creates a new ArgumentException with its message
        // string set to the empty string.
        public ArgumentException()
            : base(SR.Arg_ArgumentException)
        {
            HResult = HResults.COR_E_ARGUMENT;
        }

        // Creates a new ArgumentException with its message
        // string set to message.
        //
        public ArgumentException(string? message)
            : base(message)
        {
            HResult = HResults.COR_E_ARGUMENT;
        }

        public ArgumentException(string? message, Exception

... [truncated 3803 chars] ...

oesNotReturn]
        private static void ThrowNullOrEmptyException(string? argument, string? paramName)
        {
            ArgumentNullException.ThrowIfNull(argument, paramName);
            throw new ArgumentException(SR.Argument_EmptyString, paramName);
        }

        [DoesNotReturn]
        private static void ThrowNullOrWhiteSpaceException(string? argument, string? paramName)
        {
            ArgumentNullException.ThrowIfNull(argument, paramName);
            throw new ArgumentException(SR.Argument_EmptyOrWhiteSpaceString, paramName);
        }
    }
}

```

## Content-View Excerpt

```cs
using System.ComponentModel;
using System.Diagnostics.CodeAnalysis;
using System.Runtime.CompilerServices;
using System.Runtime.Serialization;

namespace System
{

    [Serializable]
    [TypeForwardedFrom("mscorlib, Version=4.0.0.0, Culture=neutral, PublicKeyToken=b77a5c561934e089")]
    public class ArgumentException : SystemException
    {
        private readonly string? _paramName;

        public ArgumentException()
            : base(SR.Arg_ArgumentException)
        {
            HResult = HResults.COR_E_ARGUMENT;
        }

        public ArgumentException(string? message)
            : base(message)
        {
            HResult = HResults.COR_E_ARGUMENT;
        }

        public ArgumentException(string? message, Exception? innerException)
            : base(message, innerException)
        {
            HResult = HResults.COR_E_ARGUMENT;
        }

        public ArgumentException(string? message, string? paramName, Exception? innerException)
            : base(message, innerException)
        {
            _paramName = paramName;
            HResult = HResults.COR_E_ARGUMENT;
        }

        public ArgumentException(string? message, string? paramName)
            : base(message)
        {

... [truncated 2217 chars] ...

DoesNotReturn]
        private static void ThrowNullOrEmptyException(string? argument, string? paramName)
        {
            ArgumentNullException.ThrowIfNull(argument, paramName);
            throw new ArgumentException(SR.Argument_EmptyString, paramName);
        }

        [DoesNotReturn]
        private static void ThrowNullOrWhiteSpaceException(string? argument, string? paramName)
        {
            ArgumentNullException.ThrowIfNull(argument, paramName);
            throw new ArgumentException(SR.Argument_EmptyOrWhiteSpaceString, paramName);
        }
    }
}
```

## Apply Minification Excerpt

```cs


using System.ComponentModel;
using System.Diagnostics.CodeAnalysis;
using System.Runtime.CompilerServices;
using System.Runtime.Serialization;

namespace System
{


    [Serializable]
    [TypeForwardedFrom("mscorlib, Version=4.0.0.0, Culture=neutral, PublicKeyToken=b77a5c561934e089")]
    public class ArgumentException : SystemException
    {
        private readonly string? _paramName;


        public ArgumentException()
            : base(SR.Arg_ArgumentException)
        {
            HResult = HResults.COR_E_ARGUMENT;
        }


        public ArgumentException(string? message)
            : base(message)
        {
            HResult = HResults.COR_E_ARGUMENT;
        }

        public ArgumentException(string? message, Exception? innerException)
            : base(message, innerException)
        {
            HResult = HResults.COR_E_ARGUMENT;
        }

        public ArgumentException(string? message, string? paramName, Exception? innerException)
            : base(message, innerException)
        {
            _paramName = paramName;
            HResult = HResults.COR_E_ARGUMENT;
        }

        public ArgumentException(string? message, string? paramName)
            : base(message)
    

... [truncated 2224 chars] ...

DoesNotReturn]
        private static void ThrowNullOrEmptyException(string? argument, string? paramName)
        {
            ArgumentNullException.ThrowIfNull(argument, paramName);
            throw new ArgumentException(SR.Argument_EmptyString, paramName);
        }

        [DoesNotReturn]
        private static void ThrowNullOrWhiteSpaceException(string? argument, string? paramName)
        {
            ArgumentNullException.ThrowIfNull(argument, paramName);
            throw new ArgumentException(SR.Argument_EmptyOrWhiteSpaceString, paramName);
        }
    }
}
```

## Sync Minify Excerpt

```cs


using System.ComponentModel;
using System.Diagnostics.CodeAnalysis;
using System.Runtime.CompilerServices;
using System.Runtime.Serialization;

namespace System
{


    [Serializable]
    [TypeForwardedFrom("mscorlib, Version=4.0.0.0, Culture=neutral, PublicKeyToken=b77a5c561934e089")]
    public class ArgumentException : SystemException
    {
        private readonly string? _paramName;


        public ArgumentException()
            : base(SR.Arg_ArgumentException)
        {
            HResult = HResults.COR_E_ARGUMENT;
        }


        public ArgumentException(string? message)
            : base(message)
        {
            HResult = HResults.COR_E_ARGUMENT;
        }

        public ArgumentException(string? message, Exception? innerException)
            : base(message, innerException)
        {
            HResult = HResults.COR_E_ARGUMENT;
        }

        public ArgumentException(string? message, string? paramName, Exception? innerException)
            : base(message, innerException)
        {
            _paramName = paramName;
            HResult = HResults.COR_E_ARGUMENT;
        }

        public ArgumentException(string? message, string? paramName)
            : base(message)
    

... [truncated 2224 chars] ...

DoesNotReturn]
        private static void ThrowNullOrEmptyException(string? argument, string? paramName)
        {
            ArgumentNullException.ThrowIfNull(argument, paramName);
            throw new ArgumentException(SR.Argument_EmptyString, paramName);
        }

        [DoesNotReturn]
        private static void ThrowNullOrWhiteSpaceException(string? argument, string? paramName)
        {
            ArgumentNullException.ThrowIfNull(argument, paramName);
            throw new ArgumentException(SR.Argument_EmptyOrWhiteSpaceString, paramName);
        }
    }
}
```

## Async Minify Excerpt

```cs


using System.ComponentModel;
using System.Diagnostics.CodeAnalysis;
using System.Runtime.CompilerServices;
using System.Runtime.Serialization;

namespace System
{


    [Serializable]
    [TypeForwardedFrom("mscorlib, Version=4.0.0.0, Culture=neutral, PublicKeyToken=b77a5c561934e089")]
    public class ArgumentException : SystemException
    {
        private readonly string? _paramName;


        public ArgumentException()
            : base(SR.Arg_ArgumentException)
        {
            HResult = HResults.COR_E_ARGUMENT;
        }


        public ArgumentException(string? message)
            : base(message)
        {
            HResult = HResults.COR_E_ARGUMENT;
        }

        public ArgumentException(string? message, Exception? innerException)
            : base(message, innerException)
        {
            HResult = HResults.COR_E_ARGUMENT;
        }

        public ArgumentException(string? message, string? paramName, Exception? innerException)
            : base(message, innerException)
        {
            _paramName = paramName;
            HResult = HResults.COR_E_ARGUMENT;
        }

        public ArgumentException(string? message, string? paramName)
            : base(message)
    

... [truncated 2224 chars] ...

DoesNotReturn]
        private static void ThrowNullOrEmptyException(string? argument, string? paramName)
        {
            ArgumentNullException.ThrowIfNull(argument, paramName);
            throw new ArgumentException(SR.Argument_EmptyString, paramName);
        }

        [DoesNotReturn]
        private static void ThrowNullOrWhiteSpaceException(string? argument, string? paramName)
        {
            ArgumentNullException.ThrowIfNull(argument, paramName);
            throw new ArgumentException(SR.Argument_EmptyOrWhiteSpaceString, paramName);
        }
    }
}
```

## Symbols

```txt
  4| using System.ComponentModel;
  5| using System.Diagnostics.CodeAnalysis;
  6| using System.Runtime.CompilerServices;
  7| using System.Runtime.Serialization;
  9| namespace System
 10| {
 14|     [Serializable]
 15|     [TypeForwardedFrom("mscorlib, Version=4.0.0.0, Culture=neutral, PublicKeyToken=b77a5c561934e089")]
 16|     public class ArgumentException : SystemException
 17|     {
 18|         private readonly string? _paramName;
 22|         public ArgumentException()
 23|             : base(SR.Arg_ArgumentException)
 24|         {
 31|         public ArgumentException(string? message)
 32|             : base(message)
 33|         {
 37|         public ArgumentException(string? message, Exception? innerException)
 38|             : base(message, innerException)
 39|         {
 43|         public ArgumentException(string? message, string? paramName, Exception? innerException)
 44|             : base(message, innerException)
 45|         {
 50|         public ArgumentException(string? message, string? paramName)
 51|             : base(message)
 52|         {
 57|         [Obsolete(Obsoletions.LegacyFormatterImplMessage, DiagnosticId = Obsoletions.LegacyFormatterImplDiagId, UrlFormat = Obsoletions.SharedUrlFormat)]
 58|         [EditorBrowsable(EditorBrowsableState.Never)]
 59|         protected ArgumentException(SerializationInfo info, StreamingContext context)
 60|             : base(info, context)
 61|         {
 65|         [Obsolete(Obsoletions.LegacyFormatterImplMessage, DiagnosticId = Obsoletions.LegacyFormatterImplDiagId, UrlFormat = Obsoletions.SharedUrlFormat)]
 66|         [EditorBrowsable(EditorBrowsableState.Never)]
 67|         public override void GetObjectData(SerializationInfo info, StreamingContext context)
 68| 

... [truncated 49 chars] ...

sage
 74|         {
 75|             get
 76|             {
 87|         }
 89|         private void SetMessageField()
 90|         {
 97|         public virtual string? ParamName => _paramName;
104|         public static void ThrowIfNullOrEmpty([NotNull] string? argument, [CallerArgumentExpression(nameof(argument))] string? paramName = null)
105|         {
117|         public static void ThrowIfNullOrWhiteSpace([NotNull] string? argument, [CallerArgumentExpression(nameof(argument))] string? paramName = null)
118|         {
125|         [DoesNotReturn]
126|         private static void ThrowNullOrEmptyException(string? argument, string? paramName)
127|         {
132|         [DoesNotReturn]
133|         private static void ThrowNullOrWhiteSpaceException(string? argument, string? paramName)
134|         {
138|     }
139| }
```
