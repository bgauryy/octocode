"""Normalize result evidence without confusing batched queries with returned hits.

Unknown payloads retain null counts. Partial/error queries preserve their returned
evidence, but cannot establish complete results or expected absence.
"""

from __future__ import annotations

import json
import re


SCHEMA_ERROR = re.compile(r"schema|unknown field|invalid_type|validation|unrecognized (argument|key)", re.I)
RG_TYPES = {"begin", "match", "content", "context", "end", "summary"}


def _count(value):
    return value if type(value) is int and value >= 0 else None


def _partial(value):
    if not isinstance(value, dict):
        return False
    if any(value.get(key) is True for key in ("partial", "isPartial", "truncated", "capped", "hasMore")):
        return True
    if value.get("terminalLimit") or value.get("status") == "partial":
        return True
    files = value.get("files")
    if isinstance(files, list) and any(_partial(file) for file in files):
        return True
    # Only inspect control metadata; matching source code can itself contain
    # fields named error, partial, etc. and must never affect classification.
    return any(_partial(value.get(key)) for key in ("meta", "diagnostics", "pagination", "stats", "responsePagination"))


def _returned(data):
    if isinstance(data, list):
        return len(data)
    if not isinstance(data, dict):
        return None
    files = data.get("files")
    if isinstance(files, list):
        if all(isinstance(item, dict) and isinstance(item.get("matches"), list) for item in files):
            return sum(len(item["matches"]) for item in files)
        for field in ("totalOccurrences", "totalMatchedLines"):
            if files and all(isinstance(item, dict) and _count(item.get(field)) is not None for item in files):
                return sum(item[field] for item in files)
        if all(isinstance(item, str) or isinstance(item, dict) and "path" in item and "matches" not in item for item in files):
            return len(files)
        return None
    for key in ("results", "items", "matches", "entries", "locations", "references", "symbols", "callers", "callees"):
        if isinstance(data.get(key), list):
            return len(data[key])
    for key in ("result_count", "resultCount", "count", "returnedMatchRows"):
        count = _count(data.get(key))
        if count is not None:
            return count
    # A fetch returns a single content result, even for a zero-byte file.
    if isinstance(data.get("path"), str) and isinstance(data.get("content"), str):
        return 1
    return None


def _result_unit(data):
    if isinstance(data, list):
        return "items"
    if not isinstance(data, dict):
        return None
    stats = data.get("stats")
    structural = data.get("searchEngine") == "structural" or isinstance(stats, dict) and "totalStructuralMatches" in stats
    files = data.get("files")
    if isinstance(files, list):
        if files and all(isinstance(item, dict) and isinstance(item.get("matches"), list) for item in files):
            return "structural_matches" if structural else "match_rows"
        for field, unit in (("totalOccurrences", "occurrences"), ("totalMatchedLines", "matched_lines")):
            if files and all(isinstance(item, dict) and _count(item.get(field)) is not None for item in files):
                return unit
        return "files" if files and _returned(data) is not None else None
    if isinstance(data.get("matches"), list) or _count(data.get("returnedMatchRows")) is not None:
        return "structural_matches" if structural else "match_rows"
    if isinstance(data.get("entries"), list):
        return "entries"
    return "items" if _returned(data) is not None else None


def _total(data, returned, complete, unit):
    if isinstance(data, dict):
        # Match rows, files, and count-only aggregates are different units.
        # In particular matchedLines/totalOccurrences cannot total a files view.
        fields = {
            "match_rows": (("pagination", "totalMatches"),),
            "structural_matches": (("stats", "totalStructuralMatches"), ("pagination", "totalMatches")),
            "files": (("pagination", "totalFiles"),),
            "occurrences": (("stats", "totalOccurrences"), ("pagination", "totalMatches")),
            "matched_lines": (("stats", "matchedLines"), ("pagination", "totalMatches")),
            "entries": (("pagination", "totalEntries"),),
            "items": (("pagination", "totalItems"),),
        }
        for container_name, key in fields.get(unit, ()):
            container = data.get(container_name)
            if isinstance(container, dict):
                count = _count(container.get(key))
                if count is not None:
                    return count
    return returned if complete else None


def _query(row, index):
    data = row.get("data")
    error = row.get("error") or (data.get("error") if isinstance(data, dict) else None)
    status = row.get("status")
    partial = _partial(row) or _partial(data)
    returned = _returned(data)
    unit = _result_unit(data)
    if error or status == "error":
        status = "error"
    elif status == "empty":
        if returned not in (None, 0):
            status = "unknown"
        else:
            returned = 0
    elif status not in (None, "success", "partial") or returned is None:
        status = "unknown"
    else:
        status = "partial" if partial else ("empty" if returned == 0 else "success")
    complete = status in {"success", "empty"} and not partial
    return {
        "index": row.get("index", index), "status": status,
        "returned_result_count": returned,
        "result_unit": unit,
        "total_result_count": None if status in {"error", "unknown"} else _total(data, returned, complete, unit),
        "complete": complete, "partial": partial,
        "error_kind": ("schema-invalid" if SCHEMA_ERROR.search(json.dumps(error)) else "runtime-failed") if status == "error" else None,
    }


def _sum_known(values):
    return sum(values) if all(value is not None for value in values) else None


def _native_query(row, position):
    row = row if isinstance(row, dict) else {}
    query = _query(row, position)
    # A missing/invalid native identity must not silently become a successful
    # generic result or an invented query index. Retain the observed payload
    # counts/status and its response position while rejecting completeness.
    query["response_position"] = position
    query["index"] = row.get("index")
    query["malformed"] = _count(query["index"]) is None
    if query["malformed"]:
        query["complete"] = False
    return query


def _summary(kind, queries, *, partial=False):
    complete = bool(queries) and all(query["complete"] for query in queries) and not partial
    units = {query["result_unit"] for query in queries if query["result_unit"] is not None}
    mixed_units = len(units) > 1
    return {
        "format": kind, "query_count": len(queries) if kind == "octocode" else None,
        "result_unit": next(iter(units)) if len(units) == 1 else None,
        "mixed_result_units": mixed_units,
        "returned_result_count": None if mixed_units else _sum_known([query["returned_result_count"] for query in queries]),
        "total_result_count": None if mixed_units else _sum_known([query["total_result_count"] for query in queries]),
        "complete": complete, "partial": partial or any(query["partial"] for query in queries),
        "queries": queries,
    }


def summarize_response(raw: bytes) -> dict[str, object]:
    unknown = {"format": "unknown", "query_count": None, "returned_result_count": None,
               "total_result_count": None, "result_unit": None, "mixed_result_units": False,
               "complete": False, "partial": False, "queries": []}
    try:
        text = raw.decode("utf-8")
        value = json.loads(text)
    except (UnicodeDecodeError, json.JSONDecodeError):
        try:
            values = [json.loads(line) for line in raw.decode("utf-8").splitlines() if line.strip()]
        except (UnicodeDecodeError, json.JSONDecodeError):
            return unknown
        if not values or not all(isinstance(item, dict) and item.get("type") in RG_TYPES for item in values):
            return unknown
        value = values
        kind = "rg-json"
    else:
        kind = "generic-json"
        if isinstance(value, dict) and value.get("type") in RG_TYPES:
            value, kind = [value], "rg-json"
    if kind == "rg-json":
        count = sum(item.get("type") in {"match", "content"} for item in value)
        return _summary(kind, [_query({"data": {"count": count}}, 0)])
    if isinstance(value, dict):
        rows = value.get("results")
        if isinstance(rows, list) and any(isinstance(row, dict) and any(
            key in row for key in ("index", "data", "meta", "status")
        ) for row in rows):
            queries = [_native_query(row, index) for index, row in enumerate(rows)]
            return _summary("octocode", queries, partial=_partial(value))
        if isinstance(value.get("response"), dict):
            response = value["response"]
            if "error" in response:
                return _summary("lsp-json", [_query({"status": "error", "error": response["error"]}, 0)])
            if "result" not in response:
                return unknown
            result = response["result"]
            if result is None:
                result = []
            elif isinstance(result, dict) and ("uri" in result and "range" in result or "targetUri" in result):
                result = [result]
            return _summary("lsp-json", [_query({"data": result}, 0)])
        data = value if _returned(value) is not None else value.get("data", value)
        return _summary(kind, [_query({**value, "data": data}, 0)])
    if isinstance(value, list):
        return _summary(kind, [_query({"data": value}, 0)])
    return unknown


def response_failure(summary):
    failures = [query["error_kind"] for query in summary["queries"] if query["error_kind"]]
    if "schema-invalid" in failures:
        return "schema-invalid"
    if failures or summary["partial"] or summary["mixed_result_units"] or any(
        query["status"] == "unknown" or query.get("malformed") for query in summary["queries"]
    ):
        return "runtime-failed"
    return None
