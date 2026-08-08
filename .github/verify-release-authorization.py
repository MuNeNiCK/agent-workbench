#!/usr/bin/env python3
"""Verify the signed release-authorization tag payload and its trusted signer."""

from __future__ import annotations

import argparse
import pathlib
import re
import sys


HEADER = "agent-workbench release authorization v1"
REQUIRED_FIELDS = {
    "work-id",
    "target-commit",
    "ready-state-revision",
    "ready-digest",
    "design-review-entry-id",
    "design-review-conclusion-entry-id",
    "design-review-target-snapshot",
    "design-review-clean",
    "implementation-review-entry-id",
    "implementation-review-conclusion-entry-id",
    "implementation-review-target-snapshot",
    "implementation-review-clean",
}
DIGEST = re.compile(r"blake3:[0-9a-f]{64}")


class AuthorizationError(ValueError):
    pass


def verify_signer(verification: str, allowed_signer: str) -> None:
    records: list[list[str]] = []
    for line in verification.splitlines():
        marker = "[GNUPG:] VALIDSIG "
        if line.startswith(marker):
            fields = line[len(marker) :].split()
            if fields:
                records.append(fields)
    if len(records) != 1:
        raise AuthorizationError("tag signature is absent, invalid, or from an untrusted signer")
    fields = records[0]
    signing_key_fingerprint = fields[0]
    if signing_key_fingerprint != allowed_signer:
        raise AuthorizationError("signing key is not the pinned primary key")
    # GnuPG appends primary-key-fpr after the fixed VALIDSIG fields when available.
    # A primary-key signature must name the same pinned identity in both positions.
    if len(fields) >= 10 and fields[-1] != allowed_signer:
        raise AuthorizationError("VALIDSIG primary-key fingerprint conflicts with the pinned key")


def parse_authorization(tag_object: str, target_commit: str) -> dict[str, str]:
    try:
        header, signed = tag_object.split("\n\n", 1)
    except ValueError as error:
        raise AuthorizationError("annotated tag object has no signed message") from error
    object_lines = [line for line in header.splitlines() if line.startswith("object ")]
    if object_lines != [f"object {target_commit}"]:
        raise AuthorizationError("release authorization tag does not target the workflow commit")
    message = signed.split("-----BEGIN PGP SIGNATURE-----", 1)[0].strip()
    lines = message.splitlines()
    if not lines or lines[0] != HEADER:
        raise AuthorizationError("missing release authorization v1 header")
    fields: dict[str, str] = {}
    for line in lines[1:]:
        if not line.strip():
            continue
        key, separator, value = line.partition(": ")
        if not separator or key in fields or not value:
            raise AuthorizationError(f"invalid release authorization field: {line!r}")
        fields[key] = value
    if set(fields) != REQUIRED_FIELDS:
        raise AuthorizationError(f"release authorization fields differ: {sorted(fields)}")
    if fields["target-commit"] != target_commit:
        raise AuthorizationError("authorized commit differs from tag target")
    if fields["design-review-clean"] != "true" or fields["implementation-review-clean"] != "true":
        raise AuthorizationError("release authorization does not attest both zero-Finding Reviews")
    if not fields["ready-state-revision"].isdigit():
        raise AuthorizationError("ready state revision is not numeric")
    for name in (
        "ready-digest",
        "design-review-target-snapshot",
        "implementation-review-target-snapshot",
    ):
        if not DIGEST.fullmatch(fields[name]):
            raise AuthorizationError(f"release authorization contains an invalid {name}")
    return fields


def verify(tag_object: str, verification: str, target_commit: str, allowed_signer: str) -> None:
    verify_signer(verification, allowed_signer)
    parse_authorization(tag_object, target_commit)


def canonical_fixture(commit: str) -> str:
    digest = "blake3:" + "a" * 64
    message = "\n".join(
        [
            HEADER,
            "work-id: work-release",
            f"target-commit: {commit}",
            "ready-state-revision: 42",
            f"ready-digest: {digest}",
            "design-review-entry-id: review-design",
            "design-review-conclusion-entry-id: conclusion-design",
            f"design-review-target-snapshot: {digest}",
            "design-review-clean: true",
            "implementation-review-entry-id: review-implementation",
            "implementation-review-conclusion-entry-id: conclusion-implementation",
            f"implementation-review-target-snapshot: {digest}",
            "implementation-review-clean: true",
        ]
    )
    return f"object {commit}\ntype commit\ntag v-test\ntagger fixture\n\n{message}\n-----BEGIN PGP SIGNATURE-----\nfixture"


def expect_rejected(label: str, action) -> None:
    try:
        action()
    except AuthorizationError:
        return
    raise AssertionError(f"invalid release authorization fixture was accepted: {label}")


def self_test() -> None:
    commit = "1" * 40
    signer = "90D71F220DD653AA1C66FA23F8195A7A5BD1D5AF"
    tag = canonical_fixture(commit)
    valid = f"[GNUPG:] VALIDSIG {signer} 2026-08-08 1786147200 0 4 0 1 10 00 {signer}"
    verify(tag, valid, commit, signer)
    expect_rejected("unsigned", lambda: verify(tag, "", commit, signer))
    expect_rejected("bad signature", lambda: verify(tag, "[GNUPG:] BADSIG fixture", commit, signer))
    expect_rejected("untrusted signer", lambda: verify(tag, valid.replace(signer, "0" * 40), commit, signer))
    subkey = "2" * 40
    expect_rejected("signing subkey", lambda: verify(tag, valid.replace(signer, subkey, 1), commit, signer))
    expect_rejected("duplicate VALIDSIG", lambda: verify(tag, valid + "\n" + valid, commit, signer))
    expect_rejected("conflicting primary", lambda: verify(tag, valid[: valid.rfind(signer)] + "3" * 40, commit, signer))
    expect_rejected("target commit", lambda: verify(tag, valid, "2" * 40, signer))
    expect_rejected("finding-bearing review", lambda: verify(tag.replace("design-review-clean: true", "design-review-clean: false"), valid, commit, signer))
    expect_rejected("invalid ready digest", lambda: verify(tag.replace("ready-digest: blake3:", "ready-digest: invalid-"), valid, commit, signer))
    expect_rejected("missing field", lambda: verify(tag.replace("work-id: work-release\n", ""), valid, commit, signer))
    expect_rejected("extra field", lambda: verify(tag.replace("work-id: work-release", "work-id: work-release\nextra: value"), valid, commit, signer))
    expect_rejected("tampered unsigned payload", lambda: verify(tag.replace("review-design", "review-tampered", 1).replace("-----BEGIN PGP SIGNATURE-----", ""), "", commit, signer))


def main() -> None:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)
    verify_parser = subparsers.add_parser("verify")
    verify_parser.add_argument("--tag-file", required=True)
    verify_parser.add_argument("--verification-file", required=True)
    verify_parser.add_argument("--target-commit", required=True)
    verify_parser.add_argument("--allowed-signer", required=True)
    subparsers.add_parser("self-test")
    arguments = parser.parse_args()
    if arguments.command == "self-test":
        self_test()
        return
    verify(
        pathlib.Path(arguments.tag_file).read_text(),
        pathlib.Path(arguments.verification_file).read_text(),
        arguments.target_commit,
        arguments.allowed_signer,
    )


if __name__ == "__main__":
    try:
        main()
    except (AuthorizationError, AssertionError) as error:
        print(error, file=sys.stderr)
        raise SystemExit(1)
