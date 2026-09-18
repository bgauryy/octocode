"""Fail-closed structural validation for a single blinded quality review.

This validates the review's relationship to its supplied packet.  It does not
determine whether a judge's source-grounded behavioral assessment is correct.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re


SCHEMA_VERSION = 1
BEHAVIORS = frozenset({"correct", "contradicted", "omitted", "uncertain"})
CITATIONS = frozenset({"decisive", "nearby-but-insufficient", "wrong", "absent"})
TOP_LEVEL_FIELDS = frozenset({
    "schemaVersion", "packetId", "case", "packetSha256", "answerSha256", "answers",
})
ANSWER_FIELDS = frozenset({
    "atoms", "factsCorrect", "decisivelyCitedCorrect", "outOf", "unsupportedClaims",
})
ATOM_FIELDS = frozenset({
    "number", "behavior", "citation", "explanation", "sourceAnchors", "answerQuotes",
    "sourceEvidence",
})
ANSWER_ANCHOR = re.compile(r"^answer:\s*(.+):(\d+)(?:-(\d+))?$")
ANSWER_CITATION = re.compile(r"([A-Za-z0-9_.\-/]+):(\d+)(?:[-–](\d+))?")
COMMA_RANGE = re.compile(r"`?\s*,\s*`?(\d+)(?:[-–](\d+))?")
SHORT_CODE_CITATION = re.compile(r"`:(\d+)(?:[-–](\d+))?`")
COMMIT_ANCHOR = re.compile(r"^answer:\s*commit:([0-9a-fA-F]{40})$")
COMMIT_URL = re.compile(r"https://[A-Za-z0-9.:-]+/[^\s<>`\"()]+/commit/[0-9a-fA-F]{40}(?![A-Za-z0-9])")


class GradeValidationError(ValueError):
    """A review cannot safely be aggregated from its supplied packet."""


def sha256_text(value):
    if not isinstance(value, str):
        raise TypeError("sha256_text requires text")
    return hashlib.sha256(value.encode("utf-8")).hexdigest()


def sha256_json(value):
    """Hash JSON independent of whitespace and object key ordering."""
    canonical = json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
    return sha256_text(canonical)


def _add(errors, condition, message):
    if not condition:
        errors.append(message)


def _integer(value):
    return type(value) is int


def _range_end(first, last):
    if last is None:
        return first
    end = int(last)
    if end >= first:
        return end
    first_text = str(first)
    if len(last) >= len(first_text):
        return None
    expanded = int(first_text[:len(first_text) - len(last)] + last)
    return expanded if expanded >= first else None


def _citations(answer):
    values = []
    for match in ANSWER_CITATION.finditer(answer):
        path, first, last = match.groups()
        first = int(first)
        end = _range_end(first, last)
        if end is None:
            continue
        values.append((path, first, end))
        offset = match.end()
        while continuation := COMMA_RANGE.match(answer, offset):
            first_text, last = continuation.groups()
            first = int(first_text)
            end = _range_end(first, last)
            if end is None:
                break
            values.append((path, first, end))
            offset = continuation.end()
    for block in re.split(r"\n\s*\n", answer):
        full_citations = list(ANSWER_CITATION.finditer(block))
        files = {match.group(1) for match in full_citations}
        for short in SHORT_CODE_CITATION.finditer(block):
            preceding = [match for match in full_citations if match.end() <= short.start()]
            same_line = [match for match in preceding
                         if "\n" not in block[match.end():short.start()]]
            owners = same_line or (preceding if len(files) == 1 else [])
            if not owners:
                continue
            first, last = short.groups()
            first = int(first)
            end = _range_end(first, last)
            if end is not None:
                values.append((owners[-1].group(1), first, end))
    return values


def _normalized_relative_path(value):
    return value.replace("\\", "/").strip("/")


def _anchor_is_cited(anchor, answer):
    commit = COMMIT_ANCHOR.fullmatch(anchor)
    if commit:
        return re.search(r"(?<![A-Za-z0-9])" + re.escape(commit.group(1)) +
                         r"(?![A-Za-z0-9])", answer) is not None
    if anchor.startswith("answer:"):
        target = anchor.removeprefix("answer:").strip()
        if COMMIT_URL.fullmatch(target):
            return any(match.group() == target for match in COMMIT_URL.finditer(answer))
    match = ANSWER_ANCHOR.fullmatch(anchor)
    if not match:
        return False
    path, first, last = match.groups()
    first, last = int(first), int(last or first)
    if last < first:
        return False
    path = _normalized_relative_path(path)
    citations = _citations(answer)
    if "/" not in path:
        same_name = {_normalized_relative_path(cited_path) for cited_path, _, _ in citations
                     if Path(cited_path).name == path}
        if len(same_name) != 1:
            return False
    return any(
        (path == _normalized_relative_path(cited_path) or
         _normalized_relative_path(cited_path).endswith("/" + path)) and
        cited_first <= first <= last <= cited_last
        for cited_path, cited_first, cited_last in citations
    )


def _is_string_list(value):
    return isinstance(value, list) and all(isinstance(item, str) and item for item in value)


def _is_member(value, options):
    return isinstance(value, str) and value in options


def validate_review(packet, grade):
    """Return deterministic integrity errors for exactly one packet and one grade."""
    errors = []
    if not isinstance(packet, dict):
        return ["packet must be an object"]
    if not isinstance(grade, dict):
        return ["grade must be an object"]

    _add(errors, set(grade) <= TOP_LEVEL_FIELDS, "grade has unknown top-level fields")
    _add(errors, _integer(grade.get("schemaVersion")) and grade["schemaVersion"] == SCHEMA_VERSION,
         "unsupported schemaVersion")
    _add(errors, isinstance(packet.get("packetId"), str) and packet["packetId"], "packet.packetId is required")
    _add(errors, isinstance(packet.get("case"), str) and packet["case"], "packet.case is required")
    _add(errors, grade.get("packetId") == packet.get("packetId"), "packetId does not match packet")
    _add(errors, grade.get("case") == packet.get("case"), "case does not match packet")
    _add(errors, grade.get("packetSha256") == sha256_json(packet), "packetSha256 does not match packet")

    answers = packet.get("answers")
    grade_answers = grade.get("answers")
    if not isinstance(answers, dict) or not answers or not all(isinstance(key, str) and isinstance(value, str)
                                                                for key, value in answers.items()):
        return errors + ["packet.answers must be a non-empty text map"]
    _add(errors, isinstance(grade_answers, dict) and set(grade_answers) == set(answers),
         "grade answers must match packet labels exactly")
    answer_hashes = grade.get("answerSha256")
    _add(errors, isinstance(answer_hashes, dict) and set(answer_hashes) == set(answers),
         "answerSha256 labels must match packet labels exactly")
    if isinstance(answer_hashes, dict):
        for label, answer in answers.items():
            _add(errors, answer_hashes.get(label) == sha256_text(answer),
                 f"answerSha256.{label} does not match packet answer")

    out_of = packet.get("outOf")
    _add(errors, _integer(out_of) and out_of > 0, "packet.outOf must be a positive integer")
    if not isinstance(grade_answers, dict) or not _integer(out_of) or out_of <= 0:
        return errors

    for label, answer in answers.items():
        value = grade_answers.get(label)
        prefix = f"answers.{label}"
        if not isinstance(value, dict):
            errors.append(f"{prefix} must be an object")
            continue
        _add(errors, set(value) <= ANSWER_FIELDS, f"{prefix} has unknown fields")
        _add(errors, _integer(value.get("outOf")) and value["outOf"] == out_of,
             f"{prefix}.outOf does not match packet")
        _add(errors, _integer(value.get("factsCorrect")), f"{prefix}.factsCorrect must be an integer")
        _add(errors, _integer(value.get("decisivelyCitedCorrect")),
             f"{prefix}.decisivelyCitedCorrect must be an integer")
        atoms = value.get("atoms")
        if not isinstance(atoms, list):
            errors.append(f"{prefix}.atoms must be a list")
            continue
        numbers = [atom.get("number") for atom in atoms if isinstance(atom, dict)]
        valid_numbers = (len(numbers) == len(atoms) and
                         all(isinstance(number, int) and not isinstance(number, bool) for number in numbers))
        _add(errors, valid_numbers and len(atoms) == out_of and
             sorted(numbers) == list(range(1, out_of + 1)),
             f"{prefix}.atoms numbers must be exactly 1..outOf")
        facts = decisive = 0
        for atom in atoms:
            atom_prefix = f"{prefix}.atoms"
            if not isinstance(atom, dict):
                errors.append(f"{atom_prefix} entries must be objects")
                continue
            _add(errors, set(atom) <= ATOM_FIELDS, f"{atom_prefix} has unknown fields")
            behavior, citation = atom.get("behavior"), atom.get("citation")
            _add(errors, _is_member(behavior, BEHAVIORS), f"{atom_prefix}.behavior is invalid")
            _add(errors, _is_member(citation, CITATIONS), f"{atom_prefix}.citation is invalid")
            _add(errors, isinstance(atom.get("explanation"), str) and atom["explanation"].strip(),
                 f"{atom_prefix}.explanation must be non-empty")
            _add(errors, "sourceEvidence" not in atom or
                 (isinstance(atom["sourceEvidence"], str) and atom["sourceEvidence"].strip()),
                 f"{atom_prefix}.sourceEvidence must be non-empty text when supplied")
            quotes = atom.get("answerQuotes")
            anchors = atom.get("sourceAnchors")
            _add(errors, _is_string_list(quotes) or quotes == [], f"{atom_prefix}.answerQuotes must be text")
            _add(errors, _is_string_list(anchors) or anchors == [], f"{atom_prefix}.sourceAnchors must be text")
            if _is_string_list(quotes):
                _add(errors, len(quotes) == len(set(quotes)), f"{atom_prefix}.answerQuotes must not repeat")
                for quote in quotes:
                    _add(errors, quote in answer, f"{atom_prefix}.answerQuotes must occur exactly in answer")
            if behavior == "correct" or behavior == "contradicted":
                _add(errors, isinstance(quotes, list) and bool(quotes),
                     f"{atom_prefix} needs an exact answer quote for {behavior}")
            answer_anchors = ([anchor for anchor in anchors if anchor.startswith("answer:")]
                              if _is_string_list(anchors) else [])
            if citation == "absent":
                _add(errors, not answer_anchors, f"{atom_prefix} marks citation absent but supplies answer anchor")
            if citation == "decisive":
                _add(errors, bool(answer_anchors), f"{atom_prefix} marks citation decisive without answer anchor")
            for anchor in answer_anchors:
                _add(errors, _anchor_is_cited(anchor, answer),
                     f"{atom_prefix} answer anchor is not supported by an answer citation: {anchor}")
            if behavior == "correct":
                facts += 1
                decisive += citation == "decisive"
        _add(errors, value.get("factsCorrect") == facts, f"{prefix}.factsCorrect does not equal correct atoms")
        _add(errors, value.get("decisivelyCitedCorrect") == decisive,
             f"{prefix}.decisivelyCitedCorrect does not equal correct decisive atoms")
        claims = value.get("unsupportedClaims")
        _add(errors, _is_string_list(claims) or claims == [], f"{prefix}.unsupportedClaims must be text")
        if _is_string_list(claims):
            for claim in claims:
                _add(errors, claim in answer, f"{prefix}.unsupportedClaims must occur exactly in answer")
    return errors


def validate_or_raise(packet, grade):
    errors = validate_review(packet, grade)
    if errors:
        raise GradeValidationError("\n".join(errors))


def main(argv=None):
    parser = argparse.ArgumentParser(description="Validate one blinded grade against one frozen packet.")
    parser.add_argument("--packet", required=True, type=Path)
    parser.add_argument("--grade", required=True, type=Path)
    args = parser.parse_args(argv)
    try:
        packet = json.loads(args.packet.read_text(encoding="utf-8"))
        grade = json.loads(args.grade.read_text(encoding="utf-8"))
        validate_or_raise(packet, grade)
    except (OSError, json.JSONDecodeError, GradeValidationError, TypeError) as error:
        print(f"grade validation failed: {error}")
        return 1
    print(json.dumps({"valid": True, "packetId": packet["packetId"], "case": packet["case"]}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
