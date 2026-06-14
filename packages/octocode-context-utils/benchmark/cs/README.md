# C# (.cs)

Source sample: `cs/00-dotnet-argument-exception.cs`

Strategy: `conservative`

| Tool | Bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| input | 5603 | - | - |
| content-view | 4017 | 28.3% | 0.175 ms |
| applyMinification | 4024 | 28.2% | 0.17 ms |
| sync minify | 4024 | 28.2% | 0.162 ms |
| async minify | 4024 | 28.2% | 0.166 ms |
| symbols | 2649 | 52.7% | 16.736 ms |

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
