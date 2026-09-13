#!/usr/bin/env python3
"""Seal, verify, and materialize the Terra v3 private evaluation bundle."""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import re
import subprocess
import tempfile
from pathlib import Path
from typing import Any


EXPECTED_COMMITS = {
    "langchain-ai/langchain": "67ee6cb63dd9ae7f3a4dfedc3095652bce15a125",
    "vercel/next.js": "d155ba9ebfffe4742efefda8d68c2e0e8e490924",
}
LANE_COUNTS = {
    "lexical": 4,
    "structural": 4,
    "semantic": 4,
    "indexed": 3,
    "codemod-preview": 2,
    "end-to-end": 3,
}
OPAQUE_ID = re.compile(r"^t3h-[0-9a-f]{16}$")
HEX64 = re.compile(r"^[0-9a-f]{64}$")
PUBLIC_CASE_FIELDS = {"id", "lane", "repositories", "envelopeDigest"}


class ValidationError(ValueError):
    pass


class LifecycleError(ValidationError):
    pass


def canonical_bytes(value: object) -> bytes:
    return json.dumps(
        value, ensure_ascii=False, sort_keys=True, separators=(",", ":")
    ).encode("utf-8")


def digest(value: object) -> str:
    return hashlib.sha256(canonical_bytes(value)).hexdigest()


def _run(argv: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(argv, check=True, text=True, capture_output=True)


def _validate_source(source: dict[str, Any]) -> list[dict[str, Any]]:
    if source.get("schemaVersion") != 1:
        raise ValidationError("private source schemaVersion must be 1")
    lifecycle = source.get("lifecycle")
    if not isinstance(lifecycle, dict):
        raise LifecycleError("private source lifecycle is required")
    if lifecycle.get("curated") != "complete":
        raise LifecycleError("private source must be curated before sealing")
    if lifecycle.get("sealed") != "pending" or lifecycle.get("ready") != "pending":
        raise LifecycleError("source must enter sealing with sealed/ready pending")
    if lifecycle.get("executed") != "not-executed":
        raise LifecycleError("curation artifacts may not claim benchmark execution")

    repositories = source.get("repositories")
    actual_commits = {
        item.get("repo"): item.get("commit")
        for item in repositories or []
        if isinstance(item, dict)
    }
    if actual_commits != EXPECTED_COMMITS:
        raise ValidationError("private source is not bound to both pinned repositories")

    cases = source.get("cases")
    if not isinstance(cases, list) or len(cases) != 20:
        raise ValidationError("private source must contain exactly 20 cases")
    ids: set[str] = set()
    lane_counts = {lane: 0 for lane in LANE_COUNTS}
    covered_repositories: set[str] = set()
    for index, case in enumerate(cases):
        if not isinstance(case, dict):
            raise ValidationError(f"case {index} must be an object")
        case_id = case.get("id")
        if not isinstance(case_id, str) or not OPAQUE_ID.fullmatch(case_id):
            raise ValidationError(f"case {index} lacks an opaque sealed id")
        if case_id in ids:
            raise ValidationError(f"duplicate private case id {case_id}")
        ids.add(case_id)
        lane = case.get("lane")
        if lane not in lane_counts:
            raise ValidationError(f"case {case_id} has invalid lane")
        lane_counts[lane] += 1
        repos = case.get("repositories")
        if (
            not isinstance(repos, list)
            or not repos
            or len(repos) != len(set(repos))
            or any(repo not in EXPECTED_COMMITS for repo in repos)
        ):
            raise ValidationError(f"case {case_id} has invalid repositories")
        covered_repositories.update(repos)
        if not isinstance(case.get("prompt"), str) or not case["prompt"].strip():
            raise ValidationError(f"case {case_id} lacks a prompt")
        anchor = case.get("anchor")
        if not isinstance(anchor, dict) or anchor.get("kind") not in {
            "exact-result-set",
            "fact-set",
            "patch-hash",
        }:
            raise ValidationError(f"case {case_id} lacks a deterministic anchor")
    if lane_counts != LANE_COUNTS:
        raise ValidationError(f"private lane distribution mismatch: {lane_counts}")
    if covered_repositories != set(EXPECTED_COMMITS):
        raise ValidationError("private source must cover both pinned repositories")
    return cases


def _manifest_unsigned(manifest: dict[str, Any]) -> dict[str, Any]:
    unsigned = dict(manifest)
    unsigned.pop("signature", None)
    return unsigned


def _public_key_from_certificate(certificate_path: Path, output_path: Path) -> None:
    _run(
        [
            "openssl",
            "x509",
            "-in",
            str(certificate_path),
            "-pubkey",
            "-noout",
            "-out",
            str(output_path),
        ]
    )


def _sign(value: dict[str, Any], key_path: Path) -> str:
    with tempfile.TemporaryDirectory() as temp:
        input_path = Path(temp) / "manifest.json"
        signature_path = Path(temp) / "manifest.sig"
        input_path.write_bytes(canonical_bytes(value) + b"\n")
        _run(
            [
                "openssl",
                "dgst",
                "-sha256",
                "-sign",
                str(key_path),
                "-out",
                str(signature_path),
                str(input_path),
            ]
        )
        return base64.b64encode(signature_path.read_bytes()).decode("ascii")


def _verify_signature(manifest: dict[str, Any], certificate_path: Path) -> bool:
    signature = manifest.get("signature")
    if not isinstance(signature, dict) or signature.get("algorithm") != "rsa-sha256":
        return False
    try:
        signature_bytes = base64.b64decode(signature.get("value", ""), validate=True)
    except (TypeError, ValueError):
        return False
    with tempfile.TemporaryDirectory() as temp:
        input_path = Path(temp) / "manifest.json"
        signature_path = Path(temp) / "manifest.sig"
        public_key_path = Path(temp) / "public.pem"
        input_path.write_bytes(canonical_bytes(_manifest_unsigned(manifest)) + b"\n")
        signature_path.write_bytes(signature_bytes)
        try:
            _public_key_from_certificate(certificate_path, public_key_path)
            _run(
                [
                    "openssl",
                    "dgst",
                    "-sha256",
                    "-verify",
                    str(public_key_path),
                    "-signature",
                    str(signature_path),
                    str(input_path),
                ]
            )
        except subprocess.CalledProcessError:
            return False
    return True


def seal_bundle(
    source: dict[str, Any],
    *,
    manifest_path: Path,
    bundle_path: Path,
    certificate_path: Path,
    signing_key_path: Path,
) -> dict[str, Any]:
    cases = _validate_source(source)
    source_bytes = canonical_bytes(source) + b"\n"
    manifest_path.parent.mkdir(parents=True, exist_ok=True)
    bundle_path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as temp:
        source_path = Path(temp) / "source.json"
        source_path.write_bytes(source_bytes)
        _run(
            [
                "openssl",
                "cms",
                "-encrypt",
                "-binary",
                "-aes-256-cbc",
                "-in",
                str(source_path),
                "-outform",
                "DER",
                "-out",
                str(bundle_path),
                str(certificate_path),
            ]
        )

    manifest: dict[str, Any] = {
        "version": 4,
        "split": "private",
        "status": "sealed",
        "materializationRequired": False,
        "suiteVersion": source["suiteVersion"],
        "normalizationVersion": source["normalizationVersion"],
        "repositories": source["repositories"],
        "lifecycle": {
            "curated": "complete",
            "sealed": "complete",
            "ready": "private-suite-ready",
            "executed": "not-executed",
        },
        "commitment": {
            "algorithm": "sha256",
            "canonicalization": "utf8-json-sort-keys-compact-lf-v1",
        },
        "bundle": {
            "path": bundle_path.name,
            "format": "openssl-cms-enveloped-data-der",
            "cipher": "aes-256-cbc",
            "digest": hashlib.sha256(bundle_path.read_bytes()).hexdigest(),
        },
        "note": (
            "Hash-only manifest for an independently curated encrypted bundle. "
            "Private-suite readiness does not imply overall campaign readiness or execution."
        ),
        "cases": [
            {
                "id": case["id"],
                "lane": case["lane"],
                "repositories": case["repositories"],
                "envelopeDigest": digest(case),
            }
            for case in cases
        ],
    }
    manifest["signature"] = {
        "algorithm": "rsa-sha256",
        "value": _sign(manifest, signing_key_path),
    }
    manifest_path.write_bytes(canonical_bytes(manifest) + b"\n")
    return manifest


def _validate_public_shape(manifest: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    lifecycle = manifest.get("lifecycle")
    expected_lifecycle = {
        "curated": "complete",
        "sealed": "complete",
        "ready": "private-suite-ready",
        "executed": "not-executed",
    }
    if manifest.get("status") != "sealed" or manifest.get("materializationRequired") is not False:
        errors.append("manifest is not sealed and materialized")
    if lifecycle != expected_lifecycle:
        errors.append("manifest lifecycle does not distinguish curated/sealed/ready/executed")
    cases = manifest.get("cases")
    if not isinstance(cases, list) or len(cases) != 20:
        errors.append("manifest must contain 20 private cases")
        return errors
    for index, case in enumerate(cases):
        if not isinstance(case, dict) or set(case) != PUBLIC_CASE_FIELDS:
            errors.append(f"private manifest case {index} leaks or omits fields")
            continue
        if not OPAQUE_ID.fullmatch(str(case.get("id", ""))):
            errors.append(f"private manifest case {index} has non-opaque id")
        if not HEX64.fullmatch(str(case.get("envelopeDigest", ""))):
            errors.append(f"private manifest case {index} has invalid commitment")
    return errors


def verify_public_artifact(
    manifest_path: Path, bundle_path: Path, certificate_path: Path
) -> dict[str, Any]:
    errors: list[str] = []
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        return {"valid": False, "errors": [str(error)]}
    errors.extend(_validate_public_shape(manifest))
    bundle = manifest.get("bundle")
    if not isinstance(bundle, dict) or bundle.get("digest") != hashlib.sha256(
        bundle_path.read_bytes()
    ).hexdigest():
        errors.append("encrypted bundle digest mismatch")
    if not _verify_signature(manifest, certificate_path):
        errors.append("curator signature verification failed")
    return {
        "valid": not errors,
        "errors": errors,
        "lifecycle": manifest.get("lifecycle"),
        "caseCount": len(manifest.get("cases", [])),
        "privateManifestDigest": digest(manifest),
    }


def materialize_bundle(
    *,
    manifest_path: Path,
    bundle_path: Path,
    certificate_path: Path,
    private_key_path: Path,
    output_path: Path,
) -> dict[str, Any]:
    public_result = verify_public_artifact(manifest_path, bundle_path, certificate_path)
    if not public_result["valid"]:
        raise ValidationError("; ".join(public_result["errors"]))
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    with tempfile.TemporaryDirectory() as temp:
        plaintext_path = Path(temp) / "private.json"
        try:
            _run(
                [
                    "openssl",
                    "cms",
                    "-decrypt",
                    "-binary",
                    "-inform",
                    "DER",
                    "-in",
                    str(bundle_path),
                    "-recip",
                    str(certificate_path),
                    "-inkey",
                    str(private_key_path),
                    "-out",
                    str(plaintext_path),
                ]
            )
        except subprocess.CalledProcessError as error:
            raise ValidationError("private bundle decryption failed") from error
        try:
            source = json.loads(plaintext_path.read_text(encoding="utf-8"))
        except json.JSONDecodeError as error:
            raise ValidationError("private bundle plaintext is not valid JSON") from error
    cases = _validate_source(source)
    commitments = {case["id"]: digest(case) for case in cases}
    manifest_commitments = {
        case["id"]: case["envelopeDigest"] for case in manifest["cases"]
    }
    if commitments != manifest_commitments:
        raise ValidationError("materialized case commitments do not match signed manifest")
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_bytes(canonical_bytes(source) + b"\n")
    return source


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    seal = subparsers.add_parser("seal")
    seal.add_argument("--source", type=Path, required=True)
    seal.add_argument("--manifest", type=Path, required=True)
    seal.add_argument("--bundle", type=Path, required=True)
    seal.add_argument("--certificate", type=Path, required=True)
    seal.add_argument("--signing-key", type=Path, required=True)
    verify = subparsers.add_parser("verify")
    verify.add_argument("--manifest", type=Path, required=True)
    verify.add_argument("--bundle", type=Path, required=True)
    verify.add_argument("--certificate", type=Path, required=True)
    materialize = subparsers.add_parser("materialize")
    materialize.add_argument("--manifest", type=Path, required=True)
    materialize.add_argument("--bundle", type=Path, required=True)
    materialize.add_argument("--certificate", type=Path, required=True)
    materialize.add_argument("--private-key", type=Path, required=True)
    materialize.add_argument("--output", type=Path, required=True)
    return parser


def main() -> int:
    args = _parser().parse_args()
    if args.command == "seal":
        source = json.loads(args.source.read_text(encoding="utf-8"))
        result = seal_bundle(
            source,
            manifest_path=args.manifest,
            bundle_path=args.bundle,
            certificate_path=args.certificate,
            signing_key_path=args.signing_key,
        )
        output = {"valid": True, "caseCount": len(result["cases"]), "lifecycle": result["lifecycle"]}
    elif args.command == "verify":
        output = verify_public_artifact(args.manifest, args.bundle, args.certificate)
    else:
        source = materialize_bundle(
            manifest_path=args.manifest,
            bundle_path=args.bundle,
            certificate_path=args.certificate,
            private_key_path=args.private_key,
            output_path=args.output,
        )
        output = {
            "valid": True,
            "caseCount": len(source["cases"]),
            "output": str(args.output),
            "executed": False,
        }
    print(json.dumps(output, sort_keys=True))
    return 0 if output.get("valid") else 1


if __name__ == "__main__":
    raise SystemExit(main())
