# C++ Header (.hpp)

Source sample: `hpp/fmt-color.hpp`

Strategy: `conservative`

| Tool | Bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| input | 25322 | - | - |
| content-view | 15620 | 38.3% | 0.812 ms |
| applyMinification | 15638 | 38.2% | 0.768 ms |
| sync minify | 15638 | 38.2% | 0.785 ms |
| async minify | 15638 | 38.2% | 0.8 ms |
| symbols | 15359 | 39.3% | 12.098 ms |

## Notes

- conservative text strategy.

## Before Excerpt

```cpp
// Formatting library for C++ - color support
//
// Copyright (c) 2018 - present, Victor Zverovich and {fmt} contributors
// All rights reserved.
//
// For the license information refer to format.h.

#ifndef FMT_COLOR_H_
#define FMT_COLOR_H_

#include "format.h"

FMT_BEGIN_NAMESPACE
FMT_BEGIN_EXPORT

enum class color : uint32_t {
  alice_blue = 0xF0F8FF,               // rgb(240,248,255)
  antique_white = 0xFAEBD7,            // rgb(250,235,215)
  aqua = 0x00FFFF,                     // rgb(0,255,255)
  aquamarine = 0x7FFFD4,               // rgb(127,255,212)
  azure = 0xF0FFFF,                    // rgb(240,255,255)
  beige = 0xF5F5DC,                    // rgb(245,245,220)
  bisque = 0xFFE4C4,                   // rgb(255,228,196)
  black = 0x000000,                    // rgb(0,0,0)
  blanched_almond = 0xFFEBCD,          // rgb(255,235,205)
  blue = 0x0000FF,                     // rgb(0,0,255)
  blue_violet = 0x8A2BE2,              // rgb(138,43,226)
  brown = 0xA52A2A,                    // rgb(165,42,42)
  burly_wood = 0xDEB887,               // rgb(222,184,135)
  cadet_blue = 0x5F9EA0,               // rgb(95,158,160)
  chartreuse = 0x7FFF00,               // rgb(127,255,0)
  chocolate = 0xD2691E,

... [truncated 23290 chars] ...

};

/**
 * Returns an argument that will be formatted using ANSI escape sequences,
 * to be used in a formatting function.
 *
 * **Example**:
 *
 *     fmt::print("Elapsed time: {0:.2f} seconds",
 *                fmt::styled(1.23, fmt::fg(fmt::color::green) |
 *                                  fmt::bg(fmt::color::blue)));
 */
template <typename T>
FMT_CONSTEXPR auto styled(const T& value, text_style ts)
    -> detail::styled_arg<remove_cvref_t<T>> {
  return detail::styled_arg<remove_cvref_t<T>>{value, ts};
}

FMT_END_EXPORT
FMT_END_NAMESPACE

#endif  // FMT_COLOR_H_

```

## Content-View Excerpt

```cpp
#ifndef FMT_COLOR_H_
#define FMT_COLOR_H_

#include "format.h"

FMT_BEGIN_NAMESPACE
FMT_BEGIN_EXPORT

enum class color : uint32_t {
  alice_blue = 0xF0F8FF,
  antique_white = 0xFAEBD7,
  aqua = 0x00FFFF,
  aquamarine = 0x7FFFD4,
  azure = 0xF0FFFF,
  beige = 0xF5F5DC,
  bisque = 0xFFE4C4,
  black = 0x000000,
  blanched_almond = 0xFFEBCD,
  blue = 0x0000FF,
  blue_violet = 0x8A2BE2,
  brown = 0xA52A2A,
  burly_wood = 0xDEB887,
  cadet_blue = 0x5F9EA0,
  chartreuse = 0x7FFF00,
  chocolate = 0xD2691E,
  coral = 0xFF7F50,
  cornflower_blue = 0x6495ED,
  cornsilk = 0xFFF8DC,
  crimson = 0xDC143C,
  cyan = 0x00FFFF,
  dark_blue = 0x00008B,
  dark_cyan = 0x008B8B,
  dark_golden_rod = 0xB8860B,
  dark_gray = 0xA9A9A9,
  dark_green = 0x006400,
  dark_khaki = 0xBDB76B,
  dark_magenta = 0x8B008B,
  dark_olive_green = 0x556B2F,
  dark_orange = 0xFF8C00,
  dark_orchid = 0x9932CC,
  dark_red = 0x8B0000,
  dark_salmon = 0xE9967A,
  dark_sea_green = 0x8FBC8F,
  dark_slate_blue = 0x483D8B,
  dark_slate_gray = 0x2F4F4F,
  dark_turquoise = 0x00CED1,
  dark_violet = 0x9400D3,
  deep_pink = 0xFF1493,
  deep_sky_blue = 0x00BFFF,
  dim_gray = 0x696969,
  dodger_blue = 0x1E90FF,
  fire_brick = 0xB22222,
  floral_white = 0xFFFAF0

... [truncated 13820 chars] ...

d_color<Char>(ts.get_background());
      out = detail::copy<Char>(background.begin(), background.end(), out);
    }
    out = formatter<T, Char>::format(arg.value, ctx);
    if (has_style) {
      auto reset_color = string_view("\x1b[0m");
      out = detail::copy<Char>(reset_color.begin(), reset_color.end(), out);
    }
    return out;
  }
};

template <typename T>
FMT_CONSTEXPR auto styled(const T& value, text_style ts)
    -> detail::styled_arg<remove_cvref_t<T>> {
  return detail::styled_arg<remove_cvref_t<T>>{value, ts};
}

FMT_END_EXPORT
FMT_END_NAMESPACE

#endif
```

## Apply Minification Excerpt

```cpp


#ifndef FMT_COLOR_H_
#define FMT_COLOR_H_

#include "format.h"

FMT_BEGIN_NAMESPACE
FMT_BEGIN_EXPORT

enum class color : uint32_t {
  alice_blue = 0xF0F8FF,
  antique_white = 0xFAEBD7,
  aqua = 0x00FFFF,
  aquamarine = 0x7FFFD4,
  azure = 0xF0FFFF,
  beige = 0xF5F5DC,
  bisque = 0xFFE4C4,
  black = 0x000000,
  blanched_almond = 0xFFEBCD,
  blue = 0x0000FF,
  blue_violet = 0x8A2BE2,
  brown = 0xA52A2A,
  burly_wood = 0xDEB887,
  cadet_blue = 0x5F9EA0,
  chartreuse = 0x7FFF00,
  chocolate = 0xD2691E,
  coral = 0xFF7F50,
  cornflower_blue = 0x6495ED,
  cornsilk = 0xFFF8DC,
  crimson = 0xDC143C,
  cyan = 0x00FFFF,
  dark_blue = 0x00008B,
  dark_cyan = 0x008B8B,
  dark_golden_rod = 0xB8860B,
  dark_gray = 0xA9A9A9,
  dark_green = 0x006400,
  dark_khaki = 0xBDB76B,
  dark_magenta = 0x8B008B,
  dark_olive_green = 0x556B2F,
  dark_orange = 0xFF8C00,
  dark_orchid = 0x9932CC,
  dark_red = 0x8B0000,
  dark_salmon = 0xE9967A,
  dark_sea_green = 0x8FBC8F,
  dark_slate_blue = 0x483D8B,
  dark_slate_gray = 0x2F4F4F,
  dark_turquoise = 0x00CED1,
  dark_violet = 0x9400D3,
  deep_pink = 0xFF1493,
  deep_sky_blue = 0x00BFFF,
  dim_gray = 0x696969,
  dodger_blue = 0x1E90FF,
  fire_brick = 0xB22222,
  floral_white = 0xFFFA

... [truncated 13838 chars] ...

_color<Char>(ts.get_background());
      out = detail::copy<Char>(background.begin(), background.end(), out);
    }
    out = formatter<T, Char>::format(arg.value, ctx);
    if (has_style) {
      auto reset_color = string_view("\x1b[0m");
      out = detail::copy<Char>(reset_color.begin(), reset_color.end(), out);
    }
    return out;
  }
};


template <typename T>
FMT_CONSTEXPR auto styled(const T& value, text_style ts)
    -> detail::styled_arg<remove_cvref_t<T>> {
  return detail::styled_arg<remove_cvref_t<T>>{value, ts};
}

FMT_END_EXPORT
FMT_END_NAMESPACE

#endif
```

## Sync Minify Excerpt

```cpp


#ifndef FMT_COLOR_H_
#define FMT_COLOR_H_

#include "format.h"

FMT_BEGIN_NAMESPACE
FMT_BEGIN_EXPORT

enum class color : uint32_t {
  alice_blue = 0xF0F8FF,
  antique_white = 0xFAEBD7,
  aqua = 0x00FFFF,
  aquamarine = 0x7FFFD4,
  azure = 0xF0FFFF,
  beige = 0xF5F5DC,
  bisque = 0xFFE4C4,
  black = 0x000000,
  blanched_almond = 0xFFEBCD,
  blue = 0x0000FF,
  blue_violet = 0x8A2BE2,
  brown = 0xA52A2A,
  burly_wood = 0xDEB887,
  cadet_blue = 0x5F9EA0,
  chartreuse = 0x7FFF00,
  chocolate = 0xD2691E,
  coral = 0xFF7F50,
  cornflower_blue = 0x6495ED,
  cornsilk = 0xFFF8DC,
  crimson = 0xDC143C,
  cyan = 0x00FFFF,
  dark_blue = 0x00008B,
  dark_cyan = 0x008B8B,
  dark_golden_rod = 0xB8860B,
  dark_gray = 0xA9A9A9,
  dark_green = 0x006400,
  dark_khaki = 0xBDB76B,
  dark_magenta = 0x8B008B,
  dark_olive_green = 0x556B2F,
  dark_orange = 0xFF8C00,
  dark_orchid = 0x9932CC,
  dark_red = 0x8B0000,
  dark_salmon = 0xE9967A,
  dark_sea_green = 0x8FBC8F,
  dark_slate_blue = 0x483D8B,
  dark_slate_gray = 0x2F4F4F,
  dark_turquoise = 0x00CED1,
  dark_violet = 0x9400D3,
  deep_pink = 0xFF1493,
  deep_sky_blue = 0x00BFFF,
  dim_gray = 0x696969,
  dodger_blue = 0x1E90FF,
  fire_brick = 0xB22222,
  floral_white = 0xFFFA

... [truncated 13838 chars] ...

_color<Char>(ts.get_background());
      out = detail::copy<Char>(background.begin(), background.end(), out);
    }
    out = formatter<T, Char>::format(arg.value, ctx);
    if (has_style) {
      auto reset_color = string_view("\x1b[0m");
      out = detail::copy<Char>(reset_color.begin(), reset_color.end(), out);
    }
    return out;
  }
};


template <typename T>
FMT_CONSTEXPR auto styled(const T& value, text_style ts)
    -> detail::styled_arg<remove_cvref_t<T>> {
  return detail::styled_arg<remove_cvref_t<T>>{value, ts};
}

FMT_END_EXPORT
FMT_END_NAMESPACE

#endif
```

## Async Minify Excerpt

```cpp


#ifndef FMT_COLOR_H_
#define FMT_COLOR_H_

#include "format.h"

FMT_BEGIN_NAMESPACE
FMT_BEGIN_EXPORT

enum class color : uint32_t {
  alice_blue = 0xF0F8FF,
  antique_white = 0xFAEBD7,
  aqua = 0x00FFFF,
  aquamarine = 0x7FFFD4,
  azure = 0xF0FFFF,
  beige = 0xF5F5DC,
  bisque = 0xFFE4C4,
  black = 0x000000,
  blanched_almond = 0xFFEBCD,
  blue = 0x0000FF,
  blue_violet = 0x8A2BE2,
  brown = 0xA52A2A,
  burly_wood = 0xDEB887,
  cadet_blue = 0x5F9EA0,
  chartreuse = 0x7FFF00,
  chocolate = 0xD2691E,
  coral = 0xFF7F50,
  cornflower_blue = 0x6495ED,
  cornsilk = 0xFFF8DC,
  crimson = 0xDC143C,
  cyan = 0x00FFFF,
  dark_blue = 0x00008B,
  dark_cyan = 0x008B8B,
  dark_golden_rod = 0xB8860B,
  dark_gray = 0xA9A9A9,
  dark_green = 0x006400,
  dark_khaki = 0xBDB76B,
  dark_magenta = 0x8B008B,
  dark_olive_green = 0x556B2F,
  dark_orange = 0xFF8C00,
  dark_orchid = 0x9932CC,
  dark_red = 0x8B0000,
  dark_salmon = 0xE9967A,
  dark_sea_green = 0x8FBC8F,
  dark_slate_blue = 0x483D8B,
  dark_slate_gray = 0x2F4F4F,
  dark_turquoise = 0x00CED1,
  dark_violet = 0x9400D3,
  deep_pink = 0xFF1493,
  deep_sky_blue = 0x00BFFF,
  dim_gray = 0x696969,
  dodger_blue = 0x1E90FF,
  fire_brick = 0xB22222,
  floral_white = 0xFFFA

... [truncated 13838 chars] ...

_color<Char>(ts.get_background());
      out = detail::copy<Char>(background.begin(), background.end(), out);
    }
    out = formatter<T, Char>::format(arg.value, ctx);
    if (has_style) {
      auto reset_color = string_view("\x1b[0m");
      out = detail::copy<Char>(reset_color.begin(), reset_color.end(), out);
    }
    return out;
  }
};


template <typename T>
FMT_CONSTEXPR auto styled(const T& value, text_style ts)
    -> detail::styled_arg<remove_cvref_t<T>> {
  return detail::styled_arg<remove_cvref_t<T>>{value, ts};
}

FMT_END_EXPORT
FMT_END_NAMESPACE

#endif
```

## Symbols

```txt
  8| #ifndef FMT_COLOR_H_
  9| #define FMT_COLOR_H_
 11| #include "format.h"
 13| FMT_BEGIN_NAMESPACE
 14| FMT_BEGIN_EXPORT
 16| enum class color : uint32_t {
 17|   alice_blue = 0xF0F8FF,               // rgb(240,248,255)
 18|   antique_white = 0xFAEBD7,            // rgb(250,235,215)
 19|   aqua = 0x00FFFF,                     // rgb(0,255,255)
 20|   aquamarine = 0x7FFFD4,               // rgb(127,255,212)
 21|   azure = 0xF0FFFF,                    // rgb(240,255,255)
 22|   beige = 0xF5F5DC,                    // rgb(245,245,220)
 23|   bisque = 0xFFE4C4,                   // rgb(255,228,196)
 24|   black = 0x000000,                    // rgb(0,0,0)
 25|   blanched_almond = 0xFFEBCD,          // rgb(255,235,205)
 26|   blue = 0x0000FF,                     // rgb(0,0,255)
 27|   blue_violet = 0x8A2BE2,              // rgb(138,43,226)
 28|   brown = 0xA52A2A,                    // rgb(165,42,42)
 29|   burly_wood = 0xDEB887,               // rgb(222,184,135)
 30|   cadet_blue = 0x5F9EA0,               // rgb(95,158,160)
 31|   chartreuse = 0x7FFF00,               // rgb(127,255,0)
 32|   chocolate = 0xD2691E,                // rgb(210,105,30)
 33|   coral = 0xFF7F50,                    // rgb(255,127,80)
 34|   cornflower_blue = 0x6495ED,          // rgb(100,149,237)
 35|   cornsilk = 0xFFF8DC,                 // rgb(255,248,220)
 36|   crimson = 0xDC143C,                  // rgb(220,20,60)
 37|   cyan = 0x00FFFF,                     // rgb(0,255,255)
 38|   dark_blue = 0x00008B,                // rgb(0,0,139)
 39|   dark_cyan = 0x008B8B,                // rgb(0,139,139)
 40|   dark_golden_rod = 0xB8860B,          // rgb(184,134,11)
 41|   dark_gray = 0xA9A9A9,                // rgb(169,169,169)
 42|   dark_green = 0x006400,

... [truncated 12759 chars] ...

rmat_args args)
596|     -> OutputIt {
612| template <typename OutputIt, typename... T,
613|           FMT_ENABLE_IF(detail::is_output_iterator<OutputIt, char>::value)>
614| inline auto format_to(OutputIt out, text_style ts, format_string<T...> fmt,
615|                       T&&... args) -> OutputIt {
619| template <typename T, typename Char>
620| struct formatter<detail::styled_arg<T>, Char> : formatter<T, Char> {
621|   template <typename FormatContext>
622|   FMT_CONSTEXPR auto format(const detail::styled_arg<T>& arg,
623|                             FormatContext& ctx) const -> decltype(ctx.out()) {
652| };
664| template <typename T>
665| FMT_CONSTEXPR auto styled(const T& value, text_style ts)
666|     -> detail::styled_arg<remove_cvref_t<T>> {
670| FMT_END_EXPORT
671| FMT_END_NAMESPACE
673| #endif  // FMT_COLOR_H_
```
