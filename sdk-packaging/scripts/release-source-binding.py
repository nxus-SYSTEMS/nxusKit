#!/usr/bin/env python3
"""Resolve and recheck the immutable source for a private SDK release."""

from __future__ import annotations

import os
import re
import subprocess
import sys
from pathlib import Path


RELEASE_TAG = re.compile(r"sdk-v[0-9]+\.[0-9]+\.[0-9]+")


class BindingError(Exception):
    """The requested release source cannot be bound safely."""


def environment(name: str) -> str:
    return os.environ.get(name, "")


def git(repository: Path, *arguments: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", "-C", str(repository), *arguments],
        text=True,
        capture_output=True,
        check=False,
    )


def resolve_commit(repository: Path, source_ref: str, description: str) -> str:
    result = git(
        repository,
        "rev-parse",
        "--verify",
        "--end-of-options",
        f"{source_ref}^{{commit}}",
    )
    candidate_sha = result.stdout.strip()
    if result.returncode != 0 or not re.fullmatch(r"[0-9a-fA-F]{40}|[0-9a-fA-F]{64}", candidate_sha):
        raise BindingError(f"cannot resolve {description}: {source_ref!r}")
    return candidate_sha.lower()


def remote_tag_commit(repository: Path, tag: str) -> str | None:
    tag_ref = f"refs/tags/{tag}"
    peeled_ref = f"{tag_ref}^{{}}"
    result = git(repository, "ls-remote", "--tags", "origin", tag_ref, peeled_ref)
    if result.returncode != 0:
        raise BindingError(f"cannot inspect remote tag {tag!r}: {result.stderr.strip()}")

    refs: dict[str, str] = {}
    for line in result.stdout.splitlines():
        fields = line.split("\t", 1)
        if len(fields) != 2 or fields[1] not in (tag_ref, peeled_ref):
            raise BindingError(f"cannot inspect remote tag {tag!r}: malformed lookup result")
        object_id, ref = fields
        if re.fullmatch(r"[0-9a-fA-F]{40}|[0-9a-fA-F]{64}", object_id) is None:
            raise BindingError(f"cannot inspect remote tag {tag!r}: malformed object id")
        refs[ref] = object_id.lower()

    if not refs:
        return None
    if tag_ref not in refs:
        raise BindingError(f"cannot inspect remote tag {tag!r}: missing direct tag ref")
    return refs.get(peeled_ref, refs[tag_ref])


def check_tag_binding(repository: Path, tag: str, candidate_sha: str) -> None:
    tag_sha = remote_tag_commit(repository, tag)
    if tag_sha is None:
        return
    if tag_sha != candidate_sha:
        raise BindingError(
            f"existing tag {tag!r} resolves to {tag_sha}, which does not match candidate {candidate_sha}"
        )


def select_release_tag() -> str:
    event_name = environment("NXUSKIT_EVENT_NAME")
    source_input = environment("NXUSKIT_SOURCE_INPUT")
    release_tag = (
        environment("NXUSKIT_EVENT_REF_NAME")
        if event_name == "push"
        else environment("NXUSKIT_RELEASE_TAG_INPUT")
    )

    if event_name == "workflow_dispatch" and environment("NXUSKIT_CREATE_RELEASE") == "true":
        if not source_input:
            raise BindingError("create_release=true requires an explicit candidate ref or SHA")
        if not release_tag:
            source_tag = source_input.removeprefix("refs/tags/")
            if RELEASE_TAG.fullmatch(source_tag):
                release_tag = source_tag
            else:
                raise BindingError(
                    "create_release=true requires release_tag=sdk-vX.Y.Z when the candidate is a branch or SHA"
                )

    if release_tag and RELEASE_TAG.fullmatch(release_tag) is None:
        raise BindingError("release tag must match sdk-vX.Y.Z exactly")
    return release_tag


def write_outputs(**values: str) -> None:
    output_path = environment("GITHUB_OUTPUT")
    if not output_path:
        raise BindingError("GITHUB_OUTPUT is required")
    with Path(output_path).open("a", encoding="utf-8") as output:
        for name, value in values.items():
            output.write(f"{name}={value}\n")


def verify_binding(repository: Path, candidate_sha: str, release_tag: str) -> None:
    if not release_tag:
        return
    check_tag_binding(repository, release_tag, candidate_sha)
    check_tag_binding(repository, f"{release_tag}-pro", candidate_sha)


def resolve(repository: Path) -> None:
    release_tag = select_release_tag()
    candidate_sha = resolve_commit(
        repository, environment("NXUSKIT_SOURCE_REF"), "source ref"
    )
    verify_binding(repository, candidate_sha, release_tag)
    release_version = release_tag.removeprefix("sdk-v") if release_tag else ""
    write_outputs(
        candidate_sha=candidate_sha,
        release_tag=release_tag,
        release_version=release_version,
        pro_release_tag=f"{release_tag}-pro" if release_tag else "",
    )


def recheck(repository: Path) -> None:
    release_tag = environment("NXUSKIT_RELEASE_TAG_INPUT")
    if RELEASE_TAG.fullmatch(release_tag) is None:
        raise BindingError("release tag must match sdk-vX.Y.Z exactly")
    candidate_sha = resolve_commit(
        repository, environment("NXUSKIT_CANDIDATE_SHA"), "candidate SHA"
    )
    verify_binding(repository, candidate_sha, release_tag)
    write_outputs(candidate_sha=candidate_sha)


def main() -> int:
    if len(sys.argv) != 2 or sys.argv[1] not in ("resolve", "recheck"):
        raise BindingError("usage: release-source-binding.py resolve|recheck")
    repository = Path(environment("NXUSKIT_REPOSITORY") or ".")
    if sys.argv[1] == "resolve":
        resolve(repository)
    else:
        recheck(repository)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except BindingError as error:
        print(f"ERROR: {error}", file=sys.stderr)
        raise SystemExit(1)
