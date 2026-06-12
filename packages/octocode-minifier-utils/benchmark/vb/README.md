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
| content-view | 81820 | 10.1% | 15.88 ms | 7/10 |
| applyMinification | 81865 | 10.1% | 15.435 ms | 7/10 |
| sync minify | 81865 | 10.1% | 15.683 ms | 7/10 |
| async minify | 81865 | 10.1% | 15.697 ms | 7/10 |
| symbols | 101799 | -11.8% | 4.893 ms | n/a |

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
| minify | 81865 | 10.1% | 9.5/10 excellent | 10/10 | 10/10 |
| symbols | 101799 | -11.8% | 9/10 excellent | 10/10 | 10/10 |

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
              "-n '$'"

... [truncated 80065 chars] ...

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
              "-n '$'"

... [truncated 80065 chars] ...

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
              "-n '$'"

... [truncated 80065 chars] ...

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
   1| ' Licensed to the .NET Foundation under one or more agreements.
   2| ' The .NET Foundation licenses this file to you under the MIT license.
   4| Imports System
   5| Imports System.Diagnostics.CodeAnalysis
   6| Imports System.Globalization
   7| Imports System.Runtime.Versioning
   8| Imports System.Text
   9| Imports Microsoft.VisualBasic.CompilerServices
  10| Imports Microsoft.VisualBasic.CompilerServices.ExceptionUtils
  11| Imports Microsoft.VisualBasic.CompilerServices.Utils
  13| Namespace Global.Microsoft.VisualBasic
  15|     Friend NotInheritable Class FormatInfoHolder
  16|         Implements IFormatProvider
  18|         Friend Sub New(ByVal nfi As NumberFormatInfo)
  19|             MyBase.New()
  20|             Me.nfi = nfi
  21|         End Sub
  23|         Private nfi As NumberFormatInfo
  25|         Private Function GetFormat(ByVal service As Type) As Object Implements IFormatProvider.GetFormat
  26|             If service Is GetType(NumberFormatInfo) Then
  27|                 Return nfi
  28|             End If
  29|             Throw New ArgumentException(SR.InternalError_VisualBasicRuntime)
  30|         End Function
  32|     End Class
  34|     Public Module Strings
  35|         'Positive format strings
  36|         '0      $n
  37|         '1      n$
  38|         '2      $ n
  39|         '3      n $
  40|         Private ReadOnly CurrencyPositiveFormatStrings() As String = {"'$'n", "n'$'", "'$' n", "n '$'"} 'Note, we wrap the $ in the literal symbol to avoid misinterpretation when using the escape character \ as a currency mark
  42|         'The negative currency pattern needs to be selected based
  43|         '  on the criteria provided for parens
  44|         'nfi.CurrencyPositivePattern
  45| 

... [truncated 99199 chars] ...

  sDest = New String(" "c, length)
2227|                 lenDest = UnsafeNativeMethods.LCMapString(lcid, dwMapFlags, sSrc, length, sDest, length)
2228|                 Return sDest
2229|             End If
2231|         End Function
2232| #End If
2234|         Private Sub ValidateTriState(ByVal Param As TriState)
2235|             If (Param <> vbTrue) AndAlso (Param <> vbFalse) AndAlso (Param <> vbUseDefault) Then
2236|                 Throw VbMakeException(vbErrors.IllegalFuncCall)
2237|             End If
2238|         End Sub
2240|         Private Function IsArrayEmpty(ByVal array As System.Array) As Boolean
2241|             If array Is Nothing Then
2242|                 Return True
2243|             End If
2244|             Return (array.Length = 0)
2245|         End Function
2246|     End Module
2247| End Namespace
```
