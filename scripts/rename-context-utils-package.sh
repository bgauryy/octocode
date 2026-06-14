#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

old_word="minifier"
new_word="context"

old_base="octocode-${old_word}-utils"
new_base="octocode-${new_word}-utils"
old_scope="@octocodeai/${old_base}"
new_scope="@octocodeai/${new_base}"
old_crate="octocode_${old_word}_utils"
new_crate="octocode_${new_word}_utils"
old_word_upper="$(printf '%s' "$old_word" | tr '[:lower:]' '[:upper:]')"
new_word_upper="$(printf '%s' "$new_word" | tr '[:lower:]' '[:upper:]')"
old_env="OCTOCODE_${old_word_upper}_NATIVE_PATH"
new_env="OCTOCODE_${new_word_upper}_NATIVE_PATH"
old_runtime="runtime/${old_word}"
new_runtime="runtime/${new_word}-utils"
old_bundle="bundle-${old_word}"
new_bundle="bundle-${new_word}-utils"

skip_path() {
  case "$1" in
    node_modules/*|*/node_modules/*|\
    dist/*|*/dist/*|\
    out/*|*/out/*|\
    coverage/*|*/coverage/*|\
    target/*|*/target/*|\
    .git/*|*/.git/*|\
    .yarn/cache/*|*/.yarn/cache/*|\
    *.node)
      return 0
      ;;
  esac
  return 1
}

rewrite_file() {
  local file="$1"

  [[ -f "$file" ]] || return 0
  skip_path "$file" && return 0
  LC_ALL=C grep -Iq . "$file" || return 0

  if LC_ALL=C grep -q \
    -e "$old_scope" \
    -e "$old_base" \
    -e "$old_crate" \
    -e "$old_env" \
    -e "$old_runtime" \
    -e "$old_bundle" \
    "$file"; then
    sed -i.bak \
      -e "s#${old_scope}#${new_scope}#g" \
      -e "s#${old_base}#${new_base}#g" \
      -e "s#${old_crate}#${new_crate}#g" \
      -e "s#${old_env}#${new_env}#g" \
      -e "s#${old_runtime}#${new_runtime}#g" \
      -e "s#${old_bundle}#${new_bundle}#g" \
      "$file"
    rm -f "$file.bak"
  fi
}

while IFS= read -r -d '' file; do
  rewrite_file "$file"
done < <(git ls-files -z)

if [[ -f "packages/octocode-mcp/scripts/${old_bundle}.mjs" ]]; then
  mv "packages/octocode-mcp/scripts/${old_bundle}.mjs" \
    "packages/octocode-mcp/scripts/${new_bundle}.mjs"
fi

if [[ -d "packages/${old_base}" ]]; then
  if [[ -e "packages/${new_base}" ]]; then
    printf 'Refusing to rename: packages/%s already exists.\n' "$new_base" >&2
    exit 1
  fi
  mv "packages/${old_base}" "packages/${new_base}"
fi

if [[ -d "packages/${new_base}" ]]; then
  while IFS= read -r -d '' path; do
    dir="$(dirname "$path")"
    name="$(basename "$path")"
    mv "$path" "${dir}/${name//$old_base/$new_base}"
  done < <(find "packages/${new_base}" -depth -name "*${old_base}*" -print0)
fi

printf 'Renamed %s to %s.\n' "$old_scope" "$new_scope"
