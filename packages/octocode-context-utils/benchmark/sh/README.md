# Shell (.sh)

Source sample: `sh/nvm.sh`

Strategy: `conservative`

| Tool | Bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| input | 156857 | - | - |
| content-view | 156306 | 0.4% | 0.997 ms |
| applyMinification | 156311 | 0.3% | 0.983 ms |
| sync minify | 156311 | 0.3% | 0.919 ms |
| async minify | 156311 | 0.3% | 0.985 ms |
| symbols | 4266 | 97.3% | 15.571 ms |

## Notes

- conservative text strategy.

## Before Excerpt

```sh
# Node Version Manager
# Implemented as a POSIX-compliant function
# Should work on sh, dash, bash, ksh, zsh
# To use source this file from your bash profile
#
# Implemented by Tim Caswell <tim@creationix.com>
# with much bash help from Matthew Ranney

# "local" warning, quote expansion warning, sed warning, `local` warning
# shellcheck disable=SC2039,SC2016,SC2001,SC3043
{ # this ensures the entire script is downloaded #

# shellcheck disable=SC3028
NVM_SCRIPT_SOURCE="$_"

nvm_is_zsh() {
  [ -n "${ZSH_VERSION-}" ]
}

nvm_stdout_is_terminal() {
  [ -t 1 ]
}

nvm_echo() {
  command printf %s\\n "$*" 2>/dev/null
}

nvm_echo_with_colors() {
  command printf %b\\n "$*" 2>/dev/null
}

nvm_cd() {
  \cd "$@"
}

nvm_err() {
  >&2 nvm_echo "$@"
}

nvm_err_with_colors() {
  >&2 nvm_echo_with_colors "$@"
}

nvm_grep() {
  GREP_OPTIONS='' command grep "$@"
}

nvm_has() {
  type "${1-}" >/dev/null 2>&1
}

nvm_has_non_aliased() {
  nvm_has "${1-}" && ! nvm_is_alias "${1-}"
}

nvm_is_alias() {
  # this is intentionally not "command alias" so it works in zsh.
  \alias "${1-}" >/dev/null 2>&1
}

nvm_command_info() {
  local COMMAND
  local INFO
  COMMAND="${1}"
  if type "${COMMAND}" | nvm_grep -q hashed; then
    INFO="$

... [truncated 155035 chars] ...

ERSION}" >/dev/null
      elif nvm_rc_version 3>/dev/null >/dev/null 2>&1; then
        nvm install >/dev/null
      else
        return 0
      fi
    ;;
    *)
      nvm_err 'Invalid auto mode supplied.'
      return 1
    ;;
  esac
}

nvm_process_parameters() {
  local NVM_AUTO_MODE
  NVM_AUTO_MODE='use'
  while [ "$#" -ne 0 ]; do
    case "$1" in
      --install) NVM_AUTO_MODE='install' ;;
      --no-use) NVM_AUTO_MODE='none' ;;
    esac
    shift
  done
  nvm_auto "${NVM_AUTO_MODE}"
}

nvm_process_parameters "$@"

} # this ensures the entire script is downloaded #

```

## Content-View Excerpt

```sh
{

NVM_SCRIPT_SOURCE="$_"

nvm_is_zsh() {
  [ -n "${ZSH_VERSION-}" ]
}

nvm_stdout_is_terminal() {
  [ -t 1 ]
}

nvm_echo() {
  command printf %s\\n "$*" 2>/dev/null
}

nvm_echo_with_colors() {
  command printf %b\\n "$*" 2>/dev/null
}

nvm_cd() {
  \cd "$@"
}

nvm_err() {
  >&2 nvm_echo "$@"
}

nvm_err_with_colors() {
  >&2 nvm_echo_with_colors "$@"
}

nvm_grep() {
  GREP_OPTIONS='' command grep "$@"
}

nvm_has() {
  type "${1-}" >/dev/null 2>&1
}

nvm_has_non_aliased() {
  nvm_has "${1-}" && ! nvm_is_alias "${1-}"
}

nvm_is_alias() {

  \alias "${1-}" >/dev/null 2>&1
}

nvm_command_info() {
  local COMMAND
  local INFO
  COMMAND="${1}"
  if type "${COMMAND}" | nvm_grep -q hashed; then
    INFO="$(type "${COMMAND}" | command sed -E 's/\(|\)//g' | command awk '{print $4}')"
  elif type "${COMMAND}" | nvm_grep -q aliased; then

    INFO="$(which "${COMMAND}") ($(type "${COMMAND}" | command awk '{ $1=$2=$3=$4="" ;print }' | command sed -e 's/^\ *//g' -Ee "s/\`|'//g"))"
  elif type "${COMMAND}" | nvm_grep -q "^${COMMAND} is an alias for"; then
    # shellcheck disable=SC2230
    INFO="$(which "${COMMAND}") ($(type "${COMMAND}" | command awk '{ $1=$2=$3=$4=$5="" ;print }' | command sed 's/^\ *//g'))"
  elif t

... [truncated 154484 chars] ...

VERSION}" >/dev/null
      elif nvm_rc_version 3>/dev/null >/dev/null 2>&1; then
        nvm install >/dev/null
      else
        return 0
      fi
    ;;
    *)
      nvm_err 'Invalid auto mode supplied.'
      return 1
    ;;
  esac
}

nvm_process_parameters() {
  local NVM_AUTO_MODE
  NVM_AUTO_MODE='use'
  while [ "$#" -ne 0 ]; do
    case "$1" in
      --install) NVM_AUTO_MODE='install' ;;
      --no-use) NVM_AUTO_MODE='none' ;;
    esac
    shift
  done
  nvm_auto "${NVM_AUTO_MODE}"
}

nvm_process_parameters "$@"

} # this ensures the entire script is downloaded #
```

## Apply Minification Excerpt

```sh


{


NVM_SCRIPT_SOURCE="$_"

nvm_is_zsh() {
  [ -n "${ZSH_VERSION-}" ]
}

nvm_stdout_is_terminal() {
  [ -t 1 ]
}

nvm_echo() {
  command printf %s\\n "$*" 2>/dev/null
}

nvm_echo_with_colors() {
  command printf %b\\n "$*" 2>/dev/null
}

nvm_cd() {
  \cd "$@"
}

nvm_err() {
  >&2 nvm_echo "$@"
}

nvm_err_with_colors() {
  >&2 nvm_echo_with_colors "$@"
}

nvm_grep() {
  GREP_OPTIONS='' command grep "$@"
}

nvm_has() {
  type "${1-}" >/dev/null 2>&1
}

nvm_has_non_aliased() {
  nvm_has "${1-}" && ! nvm_is_alias "${1-}"
}

nvm_is_alias() {

  \alias "${1-}" >/dev/null 2>&1
}

nvm_command_info() {
  local COMMAND
  local INFO
  COMMAND="${1}"
  if type "${COMMAND}" | nvm_grep -q hashed; then
    INFO="$(type "${COMMAND}" | command sed -E 's/\(|\)//g' | command awk '{print $4}')"
  elif type "${COMMAND}" | nvm_grep -q aliased; then

    INFO="$(which "${COMMAND}") ($(type "${COMMAND}" | command awk '{ $1=$2=$3=$4="" ;print }' | command sed -e 's/^\ *//g' -Ee "s/\`|'//g"))"
  elif type "${COMMAND}" | nvm_grep -q "^${COMMAND} is an alias for"; then
    # shellcheck disable=SC2230
    INFO="$(which "${COMMAND}") ($(type "${COMMAND}" | command awk '{ $1=$2=$3=$4=$5="" ;print }' | command sed 's/^\ *//g'))"
  eli

... [truncated 154489 chars] ...

VERSION}" >/dev/null
      elif nvm_rc_version 3>/dev/null >/dev/null 2>&1; then
        nvm install >/dev/null
      else
        return 0
      fi
    ;;
    *)
      nvm_err 'Invalid auto mode supplied.'
      return 1
    ;;
  esac
}

nvm_process_parameters() {
  local NVM_AUTO_MODE
  NVM_AUTO_MODE='use'
  while [ "$#" -ne 0 ]; do
    case "$1" in
      --install) NVM_AUTO_MODE='install' ;;
      --no-use) NVM_AUTO_MODE='none' ;;
    esac
    shift
  done
  nvm_auto "${NVM_AUTO_MODE}"
}

nvm_process_parameters "$@"

} # this ensures the entire script is downloaded #
```

## Sync Minify Excerpt

```sh


{


NVM_SCRIPT_SOURCE="$_"

nvm_is_zsh() {
  [ -n "${ZSH_VERSION-}" ]
}

nvm_stdout_is_terminal() {
  [ -t 1 ]
}

nvm_echo() {
  command printf %s\\n "$*" 2>/dev/null
}

nvm_echo_with_colors() {
  command printf %b\\n "$*" 2>/dev/null
}

nvm_cd() {
  \cd "$@"
}

nvm_err() {
  >&2 nvm_echo "$@"
}

nvm_err_with_colors() {
  >&2 nvm_echo_with_colors "$@"
}

nvm_grep() {
  GREP_OPTIONS='' command grep "$@"
}

nvm_has() {
  type "${1-}" >/dev/null 2>&1
}

nvm_has_non_aliased() {
  nvm_has "${1-}" && ! nvm_is_alias "${1-}"
}

nvm_is_alias() {

  \alias "${1-}" >/dev/null 2>&1
}

nvm_command_info() {
  local COMMAND
  local INFO
  COMMAND="${1}"
  if type "${COMMAND}" | nvm_grep -q hashed; then
    INFO="$(type "${COMMAND}" | command sed -E 's/\(|\)//g' | command awk '{print $4}')"
  elif type "${COMMAND}" | nvm_grep -q aliased; then

    INFO="$(which "${COMMAND}") ($(type "${COMMAND}" | command awk '{ $1=$2=$3=$4="" ;print }' | command sed -e 's/^\ *//g' -Ee "s/\`|'//g"))"
  elif type "${COMMAND}" | nvm_grep -q "^${COMMAND} is an alias for"; then
    # shellcheck disable=SC2230
    INFO="$(which "${COMMAND}") ($(type "${COMMAND}" | command awk '{ $1=$2=$3=$4=$5="" ;print }' | command sed 's/^\ *//g'))"
  eli

... [truncated 154489 chars] ...

VERSION}" >/dev/null
      elif nvm_rc_version 3>/dev/null >/dev/null 2>&1; then
        nvm install >/dev/null
      else
        return 0
      fi
    ;;
    *)
      nvm_err 'Invalid auto mode supplied.'
      return 1
    ;;
  esac
}

nvm_process_parameters() {
  local NVM_AUTO_MODE
  NVM_AUTO_MODE='use'
  while [ "$#" -ne 0 ]; do
    case "$1" in
      --install) NVM_AUTO_MODE='install' ;;
      --no-use) NVM_AUTO_MODE='none' ;;
    esac
    shift
  done
  nvm_auto "${NVM_AUTO_MODE}"
}

nvm_process_parameters "$@"

} # this ensures the entire script is downloaded #
```

## Async Minify Excerpt

```sh


{


NVM_SCRIPT_SOURCE="$_"

nvm_is_zsh() {
  [ -n "${ZSH_VERSION-}" ]
}

nvm_stdout_is_terminal() {
  [ -t 1 ]
}

nvm_echo() {
  command printf %s\\n "$*" 2>/dev/null
}

nvm_echo_with_colors() {
  command printf %b\\n "$*" 2>/dev/null
}

nvm_cd() {
  \cd "$@"
}

nvm_err() {
  >&2 nvm_echo "$@"
}

nvm_err_with_colors() {
  >&2 nvm_echo_with_colors "$@"
}

nvm_grep() {
  GREP_OPTIONS='' command grep "$@"
}

nvm_has() {
  type "${1-}" >/dev/null 2>&1
}

nvm_has_non_aliased() {
  nvm_has "${1-}" && ! nvm_is_alias "${1-}"
}

nvm_is_alias() {

  \alias "${1-}" >/dev/null 2>&1
}

nvm_command_info() {
  local COMMAND
  local INFO
  COMMAND="${1}"
  if type "${COMMAND}" | nvm_grep -q hashed; then
    INFO="$(type "${COMMAND}" | command sed -E 's/\(|\)//g' | command awk '{print $4}')"
  elif type "${COMMAND}" | nvm_grep -q aliased; then

    INFO="$(which "${COMMAND}") ($(type "${COMMAND}" | command awk '{ $1=$2=$3=$4="" ;print }' | command sed -e 's/^\ *//g' -Ee "s/\`|'//g"))"
  elif type "${COMMAND}" | nvm_grep -q "^${COMMAND} is an alias for"; then
    # shellcheck disable=SC2230
    INFO="$(which "${COMMAND}") ($(type "${COMMAND}" | command awk '{ $1=$2=$3=$4=$5="" ;print }' | command sed 's/^\ *//g'))"
  eli

... [truncated 154489 chars] ...

VERSION}" >/dev/null
      elif nvm_rc_version 3>/dev/null >/dev/null 2>&1; then
        nvm install >/dev/null
      else
        return 0
      fi
    ;;
    *)
      nvm_err 'Invalid auto mode supplied.'
      return 1
    ;;
  esac
}

nvm_process_parameters() {
  local NVM_AUTO_MODE
  NVM_AUTO_MODE='use'
  while [ "$#" -ne 0 ]; do
    case "$1" in
      --install) NVM_AUTO_MODE='install' ;;
      --no-use) NVM_AUTO_MODE='none' ;;
    esac
    shift
  done
  nvm_auto "${NVM_AUTO_MODE}"
}

nvm_process_parameters "$@"

} # this ensures the entire script is downloaded #
```

## Symbols

```txt
  11| { # this ensures the entire script is downloaded #
  14| NVM_SCRIPT_SOURCE="$_"
  16| nvm_is_zsh() {
  20| nvm_stdout_is_terminal() {
  24| nvm_echo() {
  28| nvm_echo_with_colors() {
  32| nvm_cd() {
  36| nvm_err() {
  40| nvm_err_with_colors() {
  44| nvm_grep() {
  48| nvm_has() {
  52| nvm_has_non_aliased() {
  56| nvm_is_alias() {
  61| nvm_command_info() {
  81| nvm_has_colors() {
  89| nvm_curl_libz_support() {
  93| nvm_curl_use_compression() {
  97| nvm_get_latest() {
 121| nvm_download() {
 176| nvm_sanitize_auth_header() {
 181| nvm_has_system_node() {
 185| nvm_has_system_iojs() {
 189| nvm_is_version_installed() {
 204| nvm_print_npm_version() {
 214| nvm_install_latest_npm() {
 455| if [ -z "${NVM_CD_FLAGS-}" ]; then
 456|   export NVM_CD_FLAGS=''
 457| fi
 458| if nvm_is_zsh; then
 459|   NVM_CD_FLAGS="-q"
 460| fi
 463| if [ -z "${NVM_DIR-}" ]; then
 465|   if [ -n "${BASH_SOURCE-}" ]; then
 466|     NVM_SCRIPT_SOURCE="${BASH_SOURCE}"
 467|   fi
 469|   NVM_DIR="$(nvm_cd ${NVM_CD_FLAGS} "$(dirname "${NVM_SCRIPT_SOURCE:-$0}")" >/dev/null && \pwd)"
 470|   export NVM_DIR
 471| else
 473|   case $NVM_DIR in
 474|     *[!/]*/)
 475|       NVM_DIR="${NVM_DIR%"${NVM_DIR##*[!/]}"}"
 476|       export NVM_DIR
 477|       nvm_err "Warning: \$NVM_DIR should not have trailing slashes"
 478|     ;;
 479|   esac
 480| fi
 481| unset NVM_SCRIPT_SOURCE 2>/dev/null
 483| nvm_tree_contains_path() {
 506| nvm_find_project_dir() {
 516| nvm_find_up() {
 525| nvm_find_nvmrc() {
 533| nvm_nvmrc_invalid_msg() {
 551| nvm_process_nvmrc() {
 620| nvm_rc_version() {
 647| nvm_clang_version() {
 651| nvm_curl_version() {
 655| nvm_version_greater() {
 670| nvm_version_greater_than_or_equal_to() {
 684| nvm_version_dir() {
 699| nvm_alias_pat

... [truncated 1666 chars] ...

all() {
2643| nvm_get_make_jobs() {
2686| nvm_install_source() {
2794| nvm_use_if_needed() {
2801| nvm_install_npm_if_needed() {
2821| nvm_match_version() {
2839| nvm_npm_global_modules() {
2854| nvm_npmrc_bad_news_bears() {
2863| nvm_die_on_prefix() {
2991| nvm_iojs_version_has_solaris_binary() {
3008| nvm_node_version_has_solaris_binary() {
3027| nvm_has_solaris_binary() {
3038| nvm_sanitize_path() {
3050| nvm_is_natural_num() {
3063| nvm_write_nvmrc() {
3081| nvm_check_file_permissions() {
3100| nvm_cache_dir() {
3107| nvm_ls_cached() {
3132| nvm_offline_version() {
3155| nvm() {
4697| nvm_get_default_packages() {
4718| nvm_install_default_packages() {
4734| nvm_supports_xz() {
4788| nvm_auto() {
4831| nvm_process_parameters() {
4844| nvm_process_parameters "$@"
4846| } # this ensures the entire script is downloaded #
```
