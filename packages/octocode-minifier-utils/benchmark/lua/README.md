# Lua (.lua)

Source sample: `lua/plenary-path.lua`

Strategy: `aggressive`

Agent rating: **8.5/10 (strong)**

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
| input | 23250 | - | - | - |
| content-view | 19630 | 15.6% | 12.172 ms | 8.5/10 |
| applyMinification | 16800 | 27.7% | 8.488 ms | 8.5/10 |
| sync minify | 16800 | 27.7% | 7.934 ms | 8.5/10 |
| async minify | 16800 | 27.7% | 8.089 ms | 8.5/10 |
| symbols | n/a | n/a | 0.015 ms | n/a |

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
| none | 23250 | 0% | 10/10 excellent | 10/10 | 10/10 |
| standard | 19630 | 15.6% | 9.5/10 excellent | 10/10 | 10/10 |
| minify | 16800 | 27.7% | 9.6/10 excellent | 10/10 | 10/10 |
| symbols | n/a | n/a | n/a | n/a | n/a |

## Notes

- aggressive text strategy.
- symbols are not implemented for this extension.

## Before Excerpt

```lua
--- Path.lua
---
--- Goal: Create objects that are extremely similar to Python's `Path` Objects.
--- Reference: https://docs.python.org/3/library/pathlib.html

local bit = require "plenary.bit"
local uv = vim.loop

local F = require "plenary.functional"

local S_IF = {
  -- S_IFDIR  = 0o040000  # directory
  DIR = 0x4000,
  -- S_IFREG  = 0o100000  # regular file
  REG = 0x8000,
}

local path = {}
path.home = vim.loop.os_homedir()

path.sep = (function()
  if jit then
    local os = string.lower(jit.os)
    if os ~= "windows" then
      return "/"
    else
      return "\\"
    end
  else
    return package.config:sub(1, 1)
  end
end)()

path.root = (function()
  if path.sep == "/" then
    return function()
      return "/"
    end
  else
    return function(base)
      base = base or vim.loop.cwd()
      return base:sub(1, 1) .. ":\\"
    end
  end
end)()

path.S_IF = S_IF

local band = function(reg, value)
  return bit.band(reg, value) == reg
end

local concat_paths = function(...)
  return table.concat({ ... }, path.sep)
end

local function is_root(pathname)
  if path.sep == "\\" then
    return string.match(pathname, "^[A-Z]:\\?$")
  end
  return pathname == "/"
end

local _split_by_separator = (funct

... [truncated 21450 chars] ...


    local read_chunk = assert(uv.fs_read(fd, length - #data, offset))
    if #read_chunk == 0 then
      break
    end
    data = data .. read_chunk
    offset = offset + #read_chunk
  end

  assert(uv.fs_close(fd))

  return data
end

function Path:find_upwards(filename)
  local folder = Path:new(self)
  local root = path.root(folder:absolute())

  while true do
    local p = folder:joinpath(filename)
    if p:exists() then
      return p
    end
    if folder:absolute() == root then
      break
    end
    folder = folder:parent()
  end
  return nil
end

return Path

```

## Content-View Excerpt

```lua
local bit = require "plenary.bit"
local uv = vim.loop

local F = require "plenary.functional"

local S_IF = {

  DIR = 0x4000,

  REG = 0x8000,
}

local path = {}
path.home = vim.loop.os_homedir()

path.sep = (function()
  if jit then
    local os = string.lower(jit.os)
    if os ~= "windows" then
      return "/"
    else
      return "\\"
    end
  else
    return package.config:sub(1, 1)
  end
end)()

path.root = (function()
  if path.sep == "/" then
    return function()
      return "/"
    end
  else
    return function(base)
      base = base or vim.loop.cwd()
      return base:sub(1, 1) .. ":\\"
    end
  end
end)()

path.S_IF = S_IF

local band = function(reg, value)
  return bit.band(reg, value) == reg
end

local concat_paths = function(...)
  return table.concat({ ... }, path.sep)
end

local function is_root(pathname)
  if path.sep == "\\" then
    return string.match(pathname, "^[A-Z]:\\?$")
  end
  return pathname == "/"
end

local _split_by_separator = (function()
  local formatted = string.format("([^%s]+)", path.sep)
  return function(filepath)
    local t = {}
    for str in string.gmatch(filepath, formatted) do
      table.insert(t, str)
    end
    return t
  end
end)()

local is_uri = 

... [truncated 17830 chars] ...

o
    local read_chunk = assert(uv.fs_read(fd, length - #data, offset))
    if #read_chunk == 0 then
      break
    end
    data = data .. read_chunk
    offset = offset + #read_chunk
  end

  assert(uv.fs_close(fd))

  return data
end

function Path:find_upwards(filename)
  local folder = Path:new(self)
  local root = path.root(folder:absolute())

  while true do
    local p = folder:joinpath(filename)
    if p:exists() then
      return p
    end
    if folder:absolute() == root then
      break
    end
    folder = folder:parent()
  end
  return nil
end

return Path
```

## Apply Minification Excerpt

```lua
local bit = require "plenary.bit" local uv = vim.loop local F = require "plenary.functional" local S_IF ={DIR = 0x4000,REG = 0x8000,}local path ={}path.home = vim.loop.os_homedir() path.sep = (function() if jit then local os = string.lower(jit.os) if os ~= "windows" then return "/" else return "\\" end else return package.config:sub(1,1) end end)() path.root = (function() if path.sep == "/" then return function() return "/" end else return function(base) base = base or vim.loop.cwd() return base:sub(1,1) .. ":\\" end end end)() path.S_IF = S_IF local band = function(reg,value) return bit.band(reg,value) == reg end local concat_paths = function(...) return table.concat({...},path.sep) end local function is_root(pathname) if path.sep == "\\" then return string.match(pathname,"^[A-Z]:\\?$") end return pathname == "/" end local _split_by_separator = (function() local formatted = string.format("([^%s]+)",path.sep) return function(filepath) local t ={}for str in string.gmatch(filepath,formatted) do table.insert(t,str) end return t end end)() local is_uri = function(filename) return string.match(filename,"^%a[%w+-.]*://") ~= nil end local is_absolute = function(filename,sep) if sep == "\\" then return string.mat

... [truncated 15000 chars] ...

+ offset if offset < 0 then offset = 0 end end local data = "" while #data < length do local read_chunk = assert(uv.fs_read(fd,length - #data,offset)) if #read_chunk == 0 then break end data = data .. read_chunk offset = offset + #read_chunk end assert(uv.fs_close(fd)) return data end function Path:find_upwards(filename) local folder = Path:new(self) local root = path.root(folder:absolute()) while true do local p = folder:joinpath(filename) if p:exists() then return p end if folder:absolute() == root then break end folder = folder:parent() end return nil end return Path
```

## Sync Minify Excerpt

```lua
local bit = require "plenary.bit" local uv = vim.loop local F = require "plenary.functional" local S_IF ={DIR = 0x4000,REG = 0x8000,}local path ={}path.home = vim.loop.os_homedir() path.sep = (function() if jit then local os = string.lower(jit.os) if os ~= "windows" then return "/" else return "\\" end else return package.config:sub(1,1) end end)() path.root = (function() if path.sep == "/" then return function() return "/" end else return function(base) base = base or vim.loop.cwd() return base:sub(1,1) .. ":\\" end end end)() path.S_IF = S_IF local band = function(reg,value) return bit.band(reg,value) == reg end local concat_paths = function(...) return table.concat({...},path.sep) end local function is_root(pathname) if path.sep == "\\" then return string.match(pathname,"^[A-Z]:\\?$") end return pathname == "/" end local _split_by_separator = (function() local formatted = string.format("([^%s]+)",path.sep) return function(filepath) local t ={}for str in string.gmatch(filepath,formatted) do table.insert(t,str) end return t end end)() local is_uri = function(filename) return string.match(filename,"^%a[%w+-.]*://") ~= nil end local is_absolute = function(filename,sep) if sep == "\\" then return string.mat

... [truncated 15000 chars] ...

+ offset if offset < 0 then offset = 0 end end local data = "" while #data < length do local read_chunk = assert(uv.fs_read(fd,length - #data,offset)) if #read_chunk == 0 then break end data = data .. read_chunk offset = offset + #read_chunk end assert(uv.fs_close(fd)) return data end function Path:find_upwards(filename) local folder = Path:new(self) local root = path.root(folder:absolute()) while true do local p = folder:joinpath(filename) if p:exists() then return p end if folder:absolute() == root then break end folder = folder:parent() end return nil end return Path
```

## Async Minify Excerpt

```lua
local bit = require "plenary.bit" local uv = vim.loop local F = require "plenary.functional" local S_IF ={DIR = 0x4000,REG = 0x8000,}local path ={}path.home = vim.loop.os_homedir() path.sep = (function() if jit then local os = string.lower(jit.os) if os ~= "windows" then return "/" else return "\\" end else return package.config:sub(1,1) end end)() path.root = (function() if path.sep == "/" then return function() return "/" end else return function(base) base = base or vim.loop.cwd() return base:sub(1,1) .. ":\\" end end end)() path.S_IF = S_IF local band = function(reg,value) return bit.band(reg,value) == reg end local concat_paths = function(...) return table.concat({...},path.sep) end local function is_root(pathname) if path.sep == "\\" then return string.match(pathname,"^[A-Z]:\\?$") end return pathname == "/" end local _split_by_separator = (function() local formatted = string.format("([^%s]+)",path.sep) return function(filepath) local t ={}for str in string.gmatch(filepath,formatted) do table.insert(t,str) end return t end end)() local is_uri = function(filename) return string.match(filename,"^%a[%w+-.]*://") ~= nil end local is_absolute = function(filename,sep) if sep == "\\" then return string.mat

... [truncated 15000 chars] ...

+ offset if offset < 0 then offset = 0 end end local data = "" while #data < length do local read_chunk = assert(uv.fs_read(fd,length - #data,offset)) if #read_chunk == 0 then break end data = data .. read_chunk offset = offset + #read_chunk end assert(uv.fs_close(fd)) return data end function Path:find_upwards(filename) local folder = Path:new(self) local root = path.root(folder:absolute()) while true do local p = folder:joinpath(filename) if p:exists() then return p end if folder:absolute() == root then break end folder = folder:parent() end return nil end return Path
```

## Symbols

```txt
No symbols returned for this sample.
```
