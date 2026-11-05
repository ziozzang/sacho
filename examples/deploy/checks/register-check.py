#!/usr/bin/env python3
"""Validate, test and create or update one check, matched by its name.

    register-check.py --url http://127.0.0.1:8080 --admin-email admin@example.com lua.json

Talks to Sacho's private address with the administrator identity header, so it
runs on the Sacho machine (or wherever the upstream is reachable). A public proxy
replaces that header and refuses this request; that is correct. Omit --admin-email
in no-auth mode. "target_file" is replaced by the content of that file, read
relative to the JSON file. Re-running keeps the monitor's ID and history.

--no-test skips the live test run. An outage result stops the script, because it
usually means a wrong target; a target that is not ready yet is the exception, so
--allow-failing-test saves anyway. A degraded result is reported and saved.
"""
import argparse
import json
import pathlib
import sys
import urllib.error
import urllib.request


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--url", default="http://127.0.0.1:8080")
    parser.add_argument("--admin-email")
    parser.add_argument("--email-header", default="X-Forwarded-Email")
    parser.add_argument("--no-test", action="store_true")
    parser.add_argument("--allow-failing-test", action="store_true")
    parser.add_argument("spec", type=pathlib.Path)
    args = parser.parse_args()

    def api(method, path, body=None):
        headers = {"Content-Type": "application/json", "X-Sacho-Request": "1"}
        if args.admin_email:
            headers[args.email_header] = args.admin_email
        data = None if body is None else json.dumps(body).encode()
        request = urllib.request.Request(args.url.rstrip("/") + path, data, headers, method=method)
        try:
            with urllib.request.urlopen(request, timeout=330) as response:
                return json.load(response)
        except urllib.error.HTTPError as error:
            sys.exit(f"{method} {path}: HTTP {error.code}: {error.read().decode(errors='replace')}")

    spec = json.loads(args.spec.read_text())
    if "target_file" in spec:
        spec["target"] = (args.spec.parent / spec.pop("target_file")).read_text()

    check = api("POST", "/api/admin/validate", spec)
    if not check.get("valid"):
        sys.exit(f"invalid specification: {check.get('errors')}")
    for warning in check.get("warnings", []):
        print("warning:", warning)

    # Push checks have nothing to run; they wait for reports.
    if spec.get("kind") != "push" and not spec.get("worker") and not args.no_test:
        result = api("POST", "/api/admin/test", spec)
        observation = result.get("observation", result)
        print("test:", observation.get("status"), observation.get("detail") or "")
        if observation.get("status") == "outage" and not args.allow_failing_test:
            sys.exit("the test did not pass; nothing was saved")

    existing = [item for item in api("GET", "/api/admin/items") if item["spec"]["name"] == spec["name"]]
    if len(existing) > 1:
        sys.exit(f"several monitors are named {spec['name']!r}; resolve this by hand")
    if existing:
        api("PUT", "/api/admin/items/" + existing[0]["id"], spec)
        print("updated", existing[0]["id"])
    else:
        print("created", api("POST", "/api/admin/items", spec)["id"])


if __name__ == "__main__":
    main()
