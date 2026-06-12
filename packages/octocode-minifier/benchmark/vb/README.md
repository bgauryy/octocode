# Visual Basic (.vb)

Source sample: `vb/00-dotnet-strings.vb`

Strategy: `conservative`

Agent rating: **7/10 (good)**

Agent understanding from minified output: **9.5/10 (excellent)**

Artifacts:

- `raw/source.excerpt.txt`
- `minified/content-view.excerpt.txt`
- `minified/apply-minification.excerpt.txt`
- `minified/minify-content-sync.excerpt.txt`
- `minified/minify-content-async.excerpt.txt`
- `symbol/signatures.txt`

| Tool | Bytes | Cut | Time | Rating |
| --- | ---: | ---: | ---: | ---: |
| input | 91031 | - | - | - |
| content-view | 81820 | 10.1% | 55.924 ms | 7/10 |
| applyMinification | 81820 | 10.1% | 42.594 ms | 7/10 |
| sync minify | 81820 | 10.1% | 38.515 ms | 7/10 |
| async minify | 81820 | 10.1% | 46.032 ms | 7/10 |
| symbols | n/a | n/a | 0.012 ms | n/a |

## Agent Understanding

Measured from `standard` minified output.

| Component | Score |
| --- | ---: |
| syntax anchors | 10/10 (3/3) |
| delimiter structure | 10/10 |
| output health | 10/10 |
| context budget | 8/10 |
| symbol context | 7/10 |
| signals passed | 6/6 |

## Agent Observation By Output Level

Ratings are computed from the actual raw, standard, minify, and symbol outputs
for this language sample.

| Level | Bytes | Cut | Agent observation | Syntax anchors | Structure |
| --- | ---: | ---: | ---: | ---: | ---: |
| none | 91031 | 0% | 10/10 excellent | 10/10 | 10/10 |
| standard | 81820 | 10.1% | 9.5/10 excellent | 10/10 | 10/10 |
| minify | 81820 | 10.1% | 9.5/10 excellent | 10/10 | 10/10 |
| symbols | n/a | n/a | n/a | n/a | n/a |

## Notes

- conservative text strategy.
- symbols are not implemented for this extension.

## Before Excerpt

```vb
' Licensed to the .NET Foundation under one or more agreements.
' The .NET Foundation licenses this file to you under the MIT license.

Imports System
Imports System.Diagnostics.CodeAnalysis
Imports System.Globalization
Imports System.Runtime.Versioning
Imports System.Text
Imports Microsoft.VisualBasic.CompilerServices
Imports Microsoft.VisualBasic.CompilerServices.ExceptionUtils
Imports Microsoft.VisualBasic.CompilerServices.Utils

Namespace Global.Microsoft.VisualBasic

    Friend NotInheritable Class FormatInfoHolder
        Implements IFormatProvider

        Friend Sub New(ByVal nfi As NumberFormatInfo)
            MyBase.New()
            Me.nfi = nfi
        End Sub

        Private nfi As NumberFormatInfo

        Private Function GetFormat(ByVal service As Type) As Object Implements IFormatProvider.GetFormat
            If service Is GetType(NumberFormatInfo) Then
                Return nfi
            End If
            Throw New ArgumentException(SR.InternalError_VisualBasicRuntime)
        End Function

    End Class

    Public Module Strings
        'Positive format strings
        '0      $n  
        '1      n$ 
        '2      $ n  
        '3      n $ 
        Private ReadOnly CurrencyPo

... [truncated 89231 chars] ...

    Return sDest
            End If

        End Function
#End If

        Private Sub ValidateTriState(ByVal Param As TriState)
            If (Param <> vbTrue) AndAlso (Param <> vbFalse) AndAlso (Param <> vbUseDefault) Then
                Throw VbMakeException(vbErrors.IllegalFuncCall)
            End If
        End Sub

        Private Function IsArrayEmpty(ByVal array As System.Array) As Boolean
            If array Is Nothing Then
                Return True
            End If
            Return (array.Length = 0)
        End Function
    End Module
End Namespace

```

## Content-View Excerpt

```vb
Imports System
Imports System.Diagnostics.CodeAnalysis
Imports System.Globalization
Imports System.Runtime.Versioning
Imports System.Text
Imports Microsoft.VisualBasic.CompilerServices
Imports Microsoft.VisualBasic.CompilerServices.ExceptionUtils
Imports Microsoft.VisualBasic.CompilerServices.Utils

Namespace Global.Microsoft.VisualBasic

    Friend NotInheritable Class FormatInfoHolder
        Implements IFormatProvider

        Friend Sub New(ByVal nfi As NumberFormatInfo)
            MyBase.New()
            Me.nfi = nfi
        End Sub

        Private nfi As NumberFormatInfo

        Private Function GetFormat(ByVal service As Type) As Object Implements IFormatProvider.GetFormat
            If service Is GetType(NumberFormatInfo) Then
                Return nfi
            End If
            Throw New ArgumentException(SR.InternalError_VisualBasicRuntime)
        End Function

    End Class

    Public Module Strings

        Private ReadOnly CurrencyPositiveFormatStrings() As String = {"'$'n", "n'$'", "'$' n", "n '$'"}

        Private ReadOnly CurrencyNegativeFormatStrings() As String =
            {"('$'n)", "-'$'n", "'$'-n", "'$'n-", "(n'$')", "-n'$'", "n-'$'", "n'$'-",
              "-n '$'", "-

... [truncated 80020 chars] ...

     Return sDest
            End If

        End Function
#End If

        Private Sub ValidateTriState(ByVal Param As TriState)
            If (Param <> vbTrue) AndAlso (Param <> vbFalse) AndAlso (Param <> vbUseDefault) Then
                Throw VbMakeException(vbErrors.IllegalFuncCall)
            End If
        End Sub

        Private Function IsArrayEmpty(ByVal array As System.Array) As Boolean
            If array Is Nothing Then
                Return True
            End If
            Return (array.Length = 0)
        End Function
    End Module
End Namespace
```

## Apply Minification Excerpt

```vb
Imports System
Imports System.Diagnostics.CodeAnalysis
Imports System.Globalization
Imports System.Runtime.Versioning
Imports System.Text
Imports Microsoft.VisualBasic.CompilerServices
Imports Microsoft.VisualBasic.CompilerServices.ExceptionUtils
Imports Microsoft.VisualBasic.CompilerServices.Utils

Namespace Global.Microsoft.VisualBasic

    Friend NotInheritable Class FormatInfoHolder
        Implements IFormatProvider

        Friend Sub New(ByVal nfi As NumberFormatInfo)
            MyBase.New()
            Me.nfi = nfi
        End Sub

        Private nfi As NumberFormatInfo

        Private Function GetFormat(ByVal service As Type) As Object Implements IFormatProvider.GetFormat
            If service Is GetType(NumberFormatInfo) Then
                Return nfi
            End If
            Throw New ArgumentException(SR.InternalError_VisualBasicRuntime)
        End Function

    End Class

    Public Module Strings

        Private ReadOnly CurrencyPositiveFormatStrings() As String = {"'$'n", "n'$'", "'$' n", "n '$'"}

        Private ReadOnly CurrencyNegativeFormatStrings() As String =
            {"('$'n)", "-'$'n", "'$'-n", "'$'n-", "(n'$')", "-n'$'", "n-'$'", "n'$'-",
              "-n '$'", "-

... [truncated 80020 chars] ...

     Return sDest
            End If

        End Function
#End If

        Private Sub ValidateTriState(ByVal Param As TriState)
            If (Param <> vbTrue) AndAlso (Param <> vbFalse) AndAlso (Param <> vbUseDefault) Then
                Throw VbMakeException(vbErrors.IllegalFuncCall)
            End If
        End Sub

        Private Function IsArrayEmpty(ByVal array As System.Array) As Boolean
            If array Is Nothing Then
                Return True
            End If
            Return (array.Length = 0)
        End Function
    End Module
End Namespace
```

## Sync Minify Excerpt

```vb
Imports System
Imports System.Diagnostics.CodeAnalysis
Imports System.Globalization
Imports System.Runtime.Versioning
Imports System.Text
Imports Microsoft.VisualBasic.CompilerServices
Imports Microsoft.VisualBasic.CompilerServices.ExceptionUtils
Imports Microsoft.VisualBasic.CompilerServices.Utils

Namespace Global.Microsoft.VisualBasic

    Friend NotInheritable Class FormatInfoHolder
        Implements IFormatProvider

        Friend Sub New(ByVal nfi As NumberFormatInfo)
            MyBase.New()
            Me.nfi = nfi
        End Sub

        Private nfi As NumberFormatInfo

        Private Function GetFormat(ByVal service As Type) As Object Implements IFormatProvider.GetFormat
            If service Is GetType(NumberFormatInfo) Then
                Return nfi
            End If
            Throw New ArgumentException(SR.InternalError_VisualBasicRuntime)
        End Function

    End Class

    Public Module Strings

        Private ReadOnly CurrencyPositiveFormatStrings() As String = {"'$'n", "n'$'", "'$' n", "n '$'"}

        Private ReadOnly CurrencyNegativeFormatStrings() As String =
            {"('$'n)", "-'$'n", "'$'-n", "'$'n-", "(n'$')", "-n'$'", "n-'$'", "n'$'-",
              "-n '$'", "-

... [truncated 80020 chars] ...

     Return sDest
            End If

        End Function
#End If

        Private Sub ValidateTriState(ByVal Param As TriState)
            If (Param <> vbTrue) AndAlso (Param <> vbFalse) AndAlso (Param <> vbUseDefault) Then
                Throw VbMakeException(vbErrors.IllegalFuncCall)
            End If
        End Sub

        Private Function IsArrayEmpty(ByVal array As System.Array) As Boolean
            If array Is Nothing Then
                Return True
            End If
            Return (array.Length = 0)
        End Function
    End Module
End Namespace
```

## Async Minify Excerpt

```vb
Imports System
Imports System.Diagnostics.CodeAnalysis
Imports System.Globalization
Imports System.Runtime.Versioning
Imports System.Text
Imports Microsoft.VisualBasic.CompilerServices
Imports Microsoft.VisualBasic.CompilerServices.ExceptionUtils
Imports Microsoft.VisualBasic.CompilerServices.Utils

Namespace Global.Microsoft.VisualBasic

    Friend NotInheritable Class FormatInfoHolder
        Implements IFormatProvider

        Friend Sub New(ByVal nfi As NumberFormatInfo)
            MyBase.New()
            Me.nfi = nfi
        End Sub

        Private nfi As NumberFormatInfo

        Private Function GetFormat(ByVal service As Type) As Object Implements IFormatProvider.GetFormat
            If service Is GetType(NumberFormatInfo) Then
                Return nfi
            End If
            Throw New ArgumentException(SR.InternalError_VisualBasicRuntime)
        End Function

    End Class

    Public Module Strings

        Private ReadOnly CurrencyPositiveFormatStrings() As String = {"'$'n", "n'$'", "'$' n", "n '$'"}

        Private ReadOnly CurrencyNegativeFormatStrings() As String =
            {"('$'n)", "-'$'n", "'$'-n", "'$'n-", "(n'$')", "-n'$'", "n-'$'", "n'$'-",
              "-n '$'", "-

... [truncated 80020 chars] ...

     Return sDest
            End If

        End Function
#End If

        Private Sub ValidateTriState(ByVal Param As TriState)
            If (Param <> vbTrue) AndAlso (Param <> vbFalse) AndAlso (Param <> vbUseDefault) Then
                Throw VbMakeException(vbErrors.IllegalFuncCall)
            End If
        End Sub

        Private Function IsArrayEmpty(ByVal array As System.Array) As Boolean
            If array Is Nothing Then
                Return True
            End If
            Return (array.Length = 0)
        End Function
    End Module
End Namespace
```

## Symbols

```txt
No symbols returned for this sample.
```
