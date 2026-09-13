from __future__ import annotations

import copy
import json
import subprocess
import tempfile
import unittest
from pathlib import Path

from curator_bundle import (
    LifecycleError,
    ValidationError,
    materialize_bundle,
    seal_bundle,
    verify_public_artifact,
)


LANES = (
    "lexical",
    "structural",
    "semantic",
    "indexed",
    "codemod-preview",
    "end-to-end",
)
COMMITS = {
    "langchain-ai/langchain": "67ee6cb63dd9ae7f3a4dfedc3095652bce15a125",
    "vercel/next.js": "d155ba9ebfffe4742efefda8d68c2e0e8e490924",
}


def source_fixture() -> dict[str, object]:
    lane_sequence = (
        ["lexical"] * 4
        + ["structural"] * 4
        + ["semantic"] * 4
        + ["indexed"] * 3
        + ["codemod-preview"] * 2
        + ["end-to-end"] * 3
    )
    cases = []
    for index, lane in enumerate(lane_sequence):
        repo = "langchain-ai/langchain" if index % 2 == 0 else "vercel/next.js"
        cases.append(
            {
                "id": f"t3h-{index + 1:016x}",
                "lane": lane,
                "repositories": [repo],
                "prompt": f"Private fixture prompt {index}",
                "anchor": {
                    "kind": "exact-result-set",
                    "oracle": "fixture",
                    "answer": [{"path": f"source-{index}.txt", "line": index + 1}],
                },
            }
        )
    return {
        "schemaVersion": 1,
        "suiteVersion": "terra-v3-private-v1",
        "repositories": [
            {"repo": repo, "commit": commit} for repo, commit in COMMITS.items()
        ],
        "normalizationVersion": "terra-private-envelope-v1",
        "lifecycle": {
            "curated": "complete",
            "sealed": "pending",
            "ready": "pending",
            "executed": "not-executed",
        },
        "cases": cases,
    }


class CuratorBundleTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.key = self.root / "curator-key.pem"
        self.cert = self.root / "curator-cert.pem"
        subprocess.run(
            [
                "openssl",
                "req",
                "-x509",
                "-newkey",
                "rsa:2048",
                "-keyout",
                str(self.key),
                "-out",
                str(self.cert),
                "-sha256",
                "-days",
                "1",
                "-nodes",
                "-subj",
                "/CN=terra-v3-test-curator",
            ],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        self.manifest = self.root / "private-manifest.json"
        self.bundle = self.root / "private-envelopes.cms"

    def tearDown(self) -> None:
        self.temp.cleanup()

    def seal(self, source: dict[str, object] | None = None) -> dict[str, object]:
        return seal_bundle(
            source or source_fixture(),
            manifest_path=self.manifest,
            bundle_path=self.bundle,
            certificate_path=self.cert,
            signing_key_path=self.key,
        )

    def test_seal_emits_hash_only_signed_manifest_with_explicit_lifecycle(self) -> None:
        manifest = self.seal()

        self.assertEqual(manifest["status"], "sealed")
        self.assertFalse(manifest["materializationRequired"])
        self.assertEqual(
            manifest["lifecycle"],
            {
                "curated": "complete",
                "sealed": "complete",
                "ready": "private-suite-ready",
                "executed": "not-executed",
            },
        )
        self.assertEqual(len(manifest["cases"]), 20)
        self.assertTrue(all(set(case) == {"id", "lane", "repositories", "envelopeDigest"} for case in manifest["cases"]))
        serialized = self.manifest.read_text(encoding="utf-8")
        self.assertNotIn("Private fixture prompt", serialized)
        self.assertNotIn('"anchor"', serialized)
        self.assertTrue(verify_public_artifact(self.manifest, self.bundle, self.cert)["valid"])

    def test_materialize_requires_private_key_and_checks_every_commitment(self) -> None:
        source = source_fixture()
        self.seal(source)
        output = self.root / "materialized.json"

        materialized = materialize_bundle(
            manifest_path=self.manifest,
            bundle_path=self.bundle,
            certificate_path=self.cert,
            private_key_path=self.key,
            output_path=output,
        )

        self.assertEqual(materialized, source)
        self.assertEqual(json.loads(output.read_text(encoding="utf-8")), source)

    def test_public_verification_rejects_ciphertext_and_manifest_tampering(self) -> None:
        self.seal()
        original = self.bundle.read_bytes()
        self.bundle.write_bytes(original[:-1] + bytes([original[-1] ^ 1]))
        self.assertFalse(verify_public_artifact(self.manifest, self.bundle, self.cert)["valid"])

        self.seal()
        manifest = json.loads(self.manifest.read_text(encoding="utf-8"))
        manifest["cases"][0]["lane"] = "semantic"
        self.manifest.write_text(json.dumps(manifest), encoding="utf-8")
        self.assertFalse(verify_public_artifact(self.manifest, self.bundle, self.cert)["valid"])

    def test_seal_rejects_executed_or_non_opaque_curator_sources(self) -> None:
        executed = source_fixture()
        executed["lifecycle"]["executed"] = "complete"
        with self.assertRaises(LifecycleError):
            self.seal(executed)

        public_ids = source_fixture()
        public_ids["cases"][0]["id"] = "s01"
        with self.assertRaises(ValidationError):
            self.seal(public_ids)

    def test_materialization_rejects_commitment_drift_even_with_valid_reseal(self) -> None:
        source = source_fixture()
        self.seal(source)
        manifest = json.loads(self.manifest.read_text(encoding="utf-8"))
        manifest["cases"][0]["envelopeDigest"] = "0" * 64
        unsigned = copy.deepcopy(manifest)
        unsigned.pop("signature")
        unsigned_path = self.root / "unsigned.json"
        unsigned_path.write_text(
            json.dumps(unsigned, ensure_ascii=False, sort_keys=True, separators=(",", ":")) + "\n",
            encoding="utf-8",
        )
        signature_path = self.root / "signature.bin"
        subprocess.run(
            ["openssl", "dgst", "-sha256", "-sign", str(self.key), "-out", str(signature_path), str(unsigned_path)],
            check=True,
        )
        import base64

        manifest["signature"]["value"] = base64.b64encode(signature_path.read_bytes()).decode("ascii")
        self.manifest.write_text(json.dumps(manifest), encoding="utf-8")
        with self.assertRaises(ValidationError):
            materialize_bundle(
                manifest_path=self.manifest,
                bundle_path=self.bundle,
                certificate_path=self.cert,
                private_key_path=self.key,
                output_path=self.root / "out.json",
            )


if __name__ == "__main__":
    unittest.main()
