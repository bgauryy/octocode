"""Documented GitHub GET syntax without expanding endpoint or write scope."""
import shlex
import unittest

import test_pilot

pilot = test_pilot.pilot


class GithubSurface(unittest.TestCase):
    setUp = test_pilot.PilotControls.setUp
    tearDown = test_pilot.PilotControls.tearDown

    def audit(self, *args):
        return pilot.Policy("raw-tools", self.cli, [self.corpus], remote=True).audit(
            shlex.join(["gh", "api", *args]))

    @property
    def endpoint(self):
        return "repos/vercel/next.js/commits/" + pilot.COMMITS["nextjs"]

    def test_documented_headers_and_flag_order(self):
        for media in ("application/vnd.github+json", "application/json",
                      "application/vnd.github.raw+json", "application/vnd.github.v3.raw+json",
                      "application/vnd.github.raw", "application/vnd.github.html+json",
                      "application/vnd.github.object", "application/vnd.github.object+json",
                      "application/vnd.github.diff", "application/vnd.github.patch",
                      "application/vnd.github.sha"):
            with self.subTest(media=media):
                self.assertIsNone(self.audit("-H", "Accept: " + media, self.endpoint))
                self.assertIsNone(self.audit(self.endpoint, "--header=accept:" + media))
        for version in ("2022-11-28", "2026-03-10"):
            self.assertIsNone(self.audit("--header", "X-GitHub-Api-Version: " + version,
                                        self.endpoint, "-H", "Accept: application/vnd.github+json"))

    def test_only_representation_and_version_headers_are_allowed(self):
        for header in ("Authorization: Bearer anything", "Host: example.org",
                       "X-HTTP-Method-Override: DELETE", "Accept: */*", "Accept: text/html",
                       "Accept: application/vnd.github+json; charset=utf-8",
                       "Accept: application/vnd.github+json\r\nHost: example.org",
                       "X-GitHub-Api-Version: 2099-01-01", "Accept"):
            self.assertIsNotNone(self.audit(self.endpoint, "-H", header), header)
        self.assertIsNotNone(self.audit(self.endpoint, "-H", "Accept: application/json",
                                       "-H", "Accept: application/vnd.github.raw"))

    def test_shaping_aliases_and_get_leave_scope_checks_active(self):
        for args in (("--jq", ".sha"), ("-q", ".sha"), ("--jq=.sha",),
                     ("--method=GET",), ("-X", "GET")):
            self.assertIsNone(self.audit(*args, self.endpoint))
            self.assertIsNotNone(self.audit(*args, "repos/other/repo/commits/" + "a" * 40))
        for args in (("--hostname", "example.org"), ("-X", "POST"), ("--method=DELETE",),
                     ("-f", "x=y"), ("--input", "-"), ("--paginate",), ("--header",)):
            self.assertIsNotNone(self.audit(self.endpoint, *args))
        self.assertIsNotNone(self.audit(self.endpoint, self.endpoint))
        self.assertIsNotNone(self.audit("-H", "Accept: application/vnd.github.raw",
                                       "repos/vercel/next.js/contents/README.md?ref=canary"))
        local = pilot.Policy("raw-tools", self.cli, [self.corpus])
        self.assertIsNotNone(local.audit(shlex.join(["gh", "api", self.endpoint])))

    def test_generic_shaping_example_is_raw_only_and_auditable(self):
        context = "VERBATIM CORE CONTEXT\n"
        def prompt(arm):
            return pilot.prompt_for("A15", arm, "Question", self.cli,
                                    {"langchain": self.corpus, "nextjs": self.corpus},
                                    pilot.Budgets(), context=context, remote=True)
        raw = prompt("raw-tools")
        example = next(line for line in raw.splitlines() if line.startswith("gh api "))
        actual = example.replace("OWNER/REPO", "vercel/next.js").replace("FULL_SHA", pilot.COMMITS["nextjs"])
        self.assertIsNone(pilot.Policy("raw-tools", self.cli, [self.corpus], remote=True).audit(actual))
        self.assertIn("--jq", example)
        for answer_hint in ("parents", "message", "cacheHit", "config.ts"):
            self.assertNotIn(answer_hint, example)
        self.assertIn("upstream API response bytes", raw)
        octocode = prompt("octocode")
        self.assertIn(context, octocode)
        self.assertNotIn(example, octocode)

    def test_repository_metadata_is_discovery_within_the_same_scope(self):
        self.assertIsNone(self.audit("repos/vercel/next.js", "--jq", ".default_branch"))
        for endpoint in ("repos/other/repo", "repos/vercel/next.js?ref=canary",
                         "repos/vercel/next.js/issues", "repos/vercel/next.js/contents/x?ref=canary"):
            self.assertIsNotNone(self.audit(endpoint), endpoint)


if __name__ == "__main__":
    unittest.main()
