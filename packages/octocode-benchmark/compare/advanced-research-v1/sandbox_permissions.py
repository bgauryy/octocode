"""Read-only research permissions and their pre-model execution probe.

Named profiles do not compose with --sandbox; see
https://learn.chatgpt.com/docs/permissions . Network destinations remain constrained
by the command audit, not a proxy. Filesystem write denial is enforced by the OS.
"""
import json
from pathlib import Path
import subprocess
import uuid

PROFILE = "octocode-benchmark-readonly-net"
PROBE_URL = "https://api.github.com/repos/vercel/next.js"
PROBE = r"""
const fs = require('node:fs');
const https = require('node:https');
const result = {};
try {
  fs.writeFileSync(process.argv[1], 'unexpected sandbox write\n', {flag: 'wx'});
  result.write = {allowed: true};
} catch (error) {
  result.write = {allowed: false, errorCode: error.code};
}
const request = https.get('https://api.github.com/repos/vercel/next.js', {
  headers: {'User-Agent': 'octocode-benchmark-permission-preflight', Accept: 'application/vnd.github+json'}
}, response => {
  let body = '';
  response.setEncoding('utf8');
  response.on('data', chunk => {
    body += chunk;
    if (Buffer.byteLength(body) > 65536) request.destroy(new Error('probe_output_limit'));
  });
  response.on('end', () => {
    try { result.network = {status: response.statusCode, repository: JSON.parse(body).full_name}; }
    catch (error) { result.network = {status: response.statusCode, error: 'invalid_metadata_json'}; }
    console.log(JSON.stringify(result));
  });
});
request.setTimeout(10000, () => request.destroy(new Error('probe_timeout')));
request.on('error', error => {
  result.network = {error: error.code || error.message};
  console.log(JSON.stringify(result));
});
"""


def permission_args(remote):
    if not remote:
        return ["--sandbox", "read-only"]
    return ["-c", f'default_permissions="{PROFILE}"',
            "-c", f'permissions.{PROFILE}.extends=":read-only"',
            "-c", f'permissions.{PROFILE}.network.enabled=true']


def preflight_permissions(remote, directory):
    permissions = permission_args(remote)
    receipt = {"passed": True, "permissionArgs": permissions, "networkProbe": "not_requested"}
    if not remote:
        return receipt
    target = Path(directory).resolve() / ("sandbox-write-probe-" + uuid.uuid4().hex)
    argv = ["codex", "sandbox", "-P", PROFILE, *permissions, "-C", "/tmp", "--",
            "node", "-e", PROBE, str(target)]
    receipt.update(passed=False, networkProbe="required", argv=argv, target=str(target),
                   url=PROBE_URL, networkEnforcement="command audit; no proxy")
    try:
        result = subprocess.run(argv, capture_output=True, text=True, timeout=20)
        receipt.update(exitCode=result.returncode, stdout=result.stdout, stderr=result.stderr)
        observed = json.loads(result.stdout)
        receipt["observed"] = observed
        network, write = observed.get("network", {}), observed.get("write", {})
        receipt["passed"] = (result.returncode == 0 and network.get("status") == 200
                             and network.get("repository") == "vercel/next.js"
                             and write.get("allowed") is False
                             and write.get("errorCode") in {"EPERM", "EACCES", "EROFS"}
                             and not target.exists())
    except (OSError, ValueError, AttributeError, subprocess.TimeoutExpired) as error:
        receipt["error"] = type(error).__name__ + ":" + str(error)
    finally:
        if target.exists():
            # Only this invocation's exclusive-create probe can occupy this path.
            target.unlink()
    return receipt
