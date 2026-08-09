#!/usr/bin/env python3
"""Verify the signed release-authorization tag payload and its trusted signer."""

from __future__ import annotations

import argparse
import datetime
import json
import pathlib
import posixpath
import re
import subprocess
import sys
import tempfile


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
OPENPGP_FINGERPRINT = re.compile(r"[0-9A-F]{40}")


class AuthorizationError(ValueError):
    pass


def verify_signer(verification: str, allowed_signer: str) -> None:
    records: list[list[str]] = []
    for line in verification.splitlines():
        marker = "[GNUPG:] VALIDSIG "
        if line.startswith(marker):
            fields = line[len(marker) :].split()
            records.append(fields)
    if len(records) != 1:
        raise AuthorizationError("tag signature is absent, invalid, or from an untrusted signer")
    fields = records[0]
    if len(fields) not in {9, 10}:
        raise AuthorizationError("VALIDSIG status record has an invalid field count")
    if not OPENPGP_FINGERPRINT.fullmatch(fields[0]):
        raise AuthorizationError("VALIDSIG signing-key fingerprint is malformed")
    try:
        datetime.date.fromisoformat(fields[1])
    except ValueError as error:
        raise AuthorizationError("VALIDSIG signature date is malformed") from error
    if any(not field.isdigit() for field in fields[2:8]):
        raise AuthorizationError("VALIDSIG numeric status field is malformed")
    if not re.fullmatch(r"[0-9A-Fa-f]{2}", fields[8]):
        raise AuthorizationError("VALIDSIG signature class is malformed")
    if len(fields) == 10 and not OPENPGP_FINGERPRINT.fullmatch(fields[9]):
        raise AuthorizationError("VALIDSIG primary-key fingerprint is malformed")
    signing_key_fingerprint = fields[0]
    if signing_key_fingerprint != allowed_signer:
        raise AuthorizationError("signing key is not the pinned primary key")
    # GnuPG appends primary-key-fpr after the fixed VALIDSIG fields when available.
    # A primary-key signature must name the same pinned identity in both positions.
    if len(fields) == 10 and fields[9] != allowed_signer:
        raise AuthorizationError("VALIDSIG primary-key fingerprint conflicts with the pinned key")


def verify_release_workflow_wiring(workflow: str) -> None:
    required_fragments = {
        "dedicated fetched tag ref": 'verification_ref="refs/agent-workbench-release-tags/$TAG_NAME"',
        "remote tag fetch": '"refs/tags/$TAG_NAME:$verification_ref"',
        "annotated object-type capture": 'git cat-file -t "$verification_ref"',
        "signature verification": 'git verify-tag --raw "$verification_ref"',
        "signed payload capture": 'git cat-file tag "$verification_ref"',
        "authoritative authorization record": 'git notes --ref=refs/notes/agent-workbench-release show "$TARGET_COMMIT"',
        "shared verifier invocation": 'python3 .github/verify-release-authorization.py verify',
        "object-type verifier input": '--tag-object-type-file "$RUNNER_TEMP/release-tag-object-type.txt"',
        "signature verifier input": '--verification-file "$RUNNER_TEMP/release-signature.txt"',
        "payload verifier input": '--tag-file "$RUNNER_TEMP/release-tag.txt"',
        "authorization verifier input": '--authorization-record-file "$RUNNER_TEMP/release-authorization.txt"',
        "package authorization dependency": "  package:\n    needs: authorize",
        "publication authorization dependency": "  publish:\n    needs: [authorize, package]",
    }
    for label, fragment in required_fragments.items():
        if workflow.count(fragment) != 1:
            raise AuthorizationError(f"release workflow lacks exact {label} wiring")


def parse_message(message: str) -> dict[str, str]:
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


def parse_authorization(tag_object: str, target_commit: str) -> tuple[str, dict[str, str]]:
    try:
        header, signed = tag_object.split("\n\n", 1)
    except ValueError as error:
        raise AuthorizationError("annotated tag object has no signed message") from error
    object_lines = [line for line in header.splitlines() if line.startswith("object ")]
    if object_lines != [f"object {target_commit}"]:
        raise AuthorizationError("release authorization tag does not target the workflow commit")
    message = signed.split("-----BEGIN PGP SIGNATURE-----", 1)[0].strip()
    fields = parse_message(message)
    if fields["target-commit"] != target_commit:
        raise AuthorizationError("authorized commit differs from tag target")
    return message, fields


def verify_tag_object_type(tag_object_type: str) -> None:
    if tag_object_type.strip() != "tag":
        raise AuthorizationError("release authorization ref is not an immutable annotated tag object")


def verify(
    tag_object: str,
    verification: str,
    authorization_record: str,
    target_commit: str,
    allowed_signer: str,
    tag_object_type: str = "tag",
) -> None:
    verify_tag_object_type(tag_object_type)
    verify_signer(verification, allowed_signer)
    message, signed_fields = parse_authorization(tag_object, target_commit)
    record = authorization_record.strip()
    record_fields = parse_message(record)
    if record_fields["target-commit"] != target_commit:
        raise AuthorizationError("authoritative Workbench record names another commit")
    if message != record or signed_fields != record_fields:
        raise AuthorizationError("signed authorization differs from the authoritative Workbench record")


def workbench_query(executable: str, project: str, operation: str, payload: dict[str, object]) -> dict[str, object]:
    result = subprocess.run(
        [executable, "--project", project, operation],
        input=json.dumps(payload),
        check=True,
        text=True,
        capture_output=True,
    )
    return json.loads(result.stdout)


def reviewed_checkout_targets(
    implementation_inspection: dict[str, object], entry_id: str
) -> list[str]:
    review = implementation_inspection.get("review", {}).get("payload", {}).get("review", {}).get("value", {})
    manifest = review.get("targetManifest")
    if review.get("targetManifestVersion") != 3 or not isinstance(manifest, list):
        raise AuthorizationError(f"{entry_id} has no current implementation target manifest")
    targets: list[str] = []
    for component in manifest:
        if not isinstance(component, dict) or component.get("kind") != "implementation_target":
            continue
        target = component.get("id")
        if not isinstance(target, str) or not target:
            raise AuthorizationError(f"{entry_id} contains an invalid implementation target")
        if target in targets:
            raise AuthorizationError(f"{entry_id} contains a duplicate implementation target")
        targets.append(target)
    if not targets:
        raise AuthorizationError(f"{entry_id} contains no reviewed implementation targets")
    return targets


def target_path(target: str) -> str:
    kind, separator, configured = target.partition(":")
    if not separator or kind not in {"file", "tree"}:
        raise AuthorizationError(f"unsupported reviewed checkout target: {target}")
    if not configured or configured == "." or "\\" in configured or configured.startswith("/"):
        raise AuthorizationError(f"reviewed checkout target is not a project-relative path: {target}")
    normalized = posixpath.normpath(configured)
    if normalized != configured or normalized == ".." or normalized.startswith("../"):
        raise AuthorizationError(f"reviewed checkout target escapes the project: {target}")
    return configured


def verify_target_checkout(project: str, target_commit: str, targets: list[str]) -> None:
    root = pathlib.Path(project).resolve()
    def git(*arguments: str, check: bool = True) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["git", *arguments], cwd=root, check=check, text=True, capture_output=True
        )
    if git("rev-parse", "HEAD").stdout.strip() != target_commit:
        raise AuthorizationError("release target differs from the current checkout")
    for target in targets:
        relative = target_path(target)
        pathspec = f":(literal){relative}"
        if git("diff", "--cached", "--quiet", target_commit, "--", pathspec, check=False).returncode != 0:
            raise AuthorizationError(f"staged reviewed target differs from the target commit: {target}")
        if git("diff", "--quiet", "--", pathspec, check=False).returncode != 0:
            raise AuthorizationError(f"tracked reviewed target differs from the target commit: {target}")
        if git("ls-files", "--others", "--exclude-standard", "--", pathspec).stdout.strip():
            raise AuthorizationError(f"untracked content exists in reviewed target: {target}")
        if git("ls-files", "--others", "--ignored", "--exclude-standard", "--", pathspec).stdout.strip():
            raise AuthorizationError(f"ignored content exists in reviewed target: {target}")


def clean_review_facts(
    inspection: dict[str, object],
    entry_id: str,
    expected_purpose: str,
    expected_work_id: str,
    expected_target: str,
) -> tuple[str, str]:
    review = inspection.get("review", {}).get("payload", {}).get("review", {}).get("value", {})
    if review.get("context") != "fresh" or review.get("purpose") != expected_purpose:
        raise AuthorizationError(f"{entry_id} is not a fresh {expected_purpose} Review")
    if inspection.get("workId") != expected_work_id:
        raise AuthorizationError(f"{entry_id} is bound to another Work")
    if inspection.get("targetCurrent") is not True:
        raise AuthorizationError(f"{entry_id} does not cover the current target")
    if review.get("target") != expected_target or review.get("targetSourceId") != expected_target.split(":", 1)[1]:
        raise AuthorizationError(f"{entry_id} is bound to another immutable target")
    reviewer = review.get("reviewerAgentRun")
    producers = review.get("producerAgentRuns")
    if not isinstance(reviewer, str) or not reviewer or not isinstance(producers, list) or reviewer in producers:
        raise AuthorizationError(f"{entry_id} is not an independent Review")
    lineage = inspection.get("lineage")
    if not isinstance(lineage, list) or any("finding" in item.get("payload", {}) for item in lineage):
        raise AuthorizationError(f"{entry_id} contains a Finding")
    conclusions = [
        item for item in lineage
        if item.get("payload", {}).get("reviewConclusion", {}).get("value", {}).get("reviewEntryId") == entry_id
    ]
    if len(conclusions) != 1:
        raise AuthorizationError(f"{entry_id} does not have exactly one conclusion")
    conclusion = conclusions[0]
    value = conclusion["payload"]["reviewConclusion"]["value"]
    if value.get("clean") is not True:
        raise AuthorizationError(f"{entry_id} is not clean")
    target_snapshot = review.get("targetSnapshot")
    if not isinstance(target_snapshot, str) or not DIGEST.fullmatch(target_snapshot):
        raise AuthorizationError(f"{entry_id} has no immutable target snapshot")
    if inspection.get("currentTargetSnapshot") != target_snapshot:
        raise AuthorizationError(f"{entry_id} current target snapshot differs")
    return conclusion["id"], target_snapshot


def authorization_record_from_facts(
    ready: dict[str, object],
    design_inspection: dict[str, object],
    implementation_inspection: dict[str, object],
    target_commit: str,
    design_review_entry_id: str,
    implementation_review_entry_id: str,
) -> str:
    if not re.fullmatch(r"[0-9a-f]{40}", target_commit):
        raise AuthorizationError("target commit is not a full lowercase SHA-1")
    context = ready.get("context", {}).get("focused", {})
    if ready.get("ready") is not True:
        raise AuthorizationError("Workbench is not ready")
    for gap in ("claimGaps", "criterionGaps", "designSourceGaps", "unfinishedRequiredTasks", "unresolvedAcceptedFindings"):
        if context.get(gap) != []:
            raise AuthorizationError(f"Workbench ready result contains {gap}")
    work_id = context.get("work", {}).get("id")
    design_id = context.get("design", {}).get("id")
    revision = ready.get("stateRevision")
    digest = ready.get("digest")
    if not isinstance(work_id, str) or not isinstance(design_id, str) or not isinstance(revision, int) or not isinstance(digest, str) or not DIGEST.fullmatch(digest):
        raise AuthorizationError("Workbench ready result lacks canonical identity")
    design_conclusion, design_snapshot = clean_review_facts(
        design_inspection, design_review_entry_id, "design", work_id, f"design:{design_id}"
    )
    implementation_conclusion, implementation_snapshot = clean_review_facts(
        implementation_inspection, implementation_review_entry_id, "implementation", work_id, f"work:{work_id}"
    )
    return "\n".join([
        HEADER,
        f"work-id: {work_id}",
        f"target-commit: {target_commit}",
        f"ready-state-revision: {revision}",
        f"ready-digest: {digest}",
        f"design-review-entry-id: {design_review_entry_id}",
        f"design-review-conclusion-entry-id: {design_conclusion}",
        f"design-review-target-snapshot: {design_snapshot}",
        "design-review-clean: true",
        f"implementation-review-entry-id: {implementation_review_entry_id}",
        f"implementation-review-conclusion-entry-id: {implementation_conclusion}",
        f"implementation-review-target-snapshot: {implementation_snapshot}",
        "implementation-review-clean: true",
    ])


def prepare_record(
    executable: str,
    project: str,
    target_commit: str,
    design_review_entry_id: str,
    implementation_review_entry_id: str,
) -> str:
    ready = workbench_query(executable, project, "ready", {})
    design_inspection = workbench_query(
        executable, project, "review inspect", {"id": design_review_entry_id}
    )
    implementation_inspection = workbench_query(
        executable, project, "review inspect", {"id": implementation_review_entry_id}
    )
    record = authorization_record_from_facts(
        ready,
        design_inspection,
        implementation_inspection,
        target_commit,
        design_review_entry_id,
        implementation_review_entry_id,
    )
    verify_target_checkout(
        project,
        target_commit,
        reviewed_checkout_targets(implementation_inspection, implementation_review_entry_id),
    )
    return record


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
    record = tag.split("\n\n", 1)[1].split("-----BEGIN PGP SIGNATURE-----", 1)[0].strip()
    verify(tag, valid, record, commit, signer)
    expect_rejected(
        "checkout-local lightweight tag substitution",
        lambda: verify(tag, valid, record, commit, signer, "commit"),
    )
    expect_rejected("unsigned", lambda: verify(tag, "", record, commit, signer))
    expect_rejected("bad signature", lambda: verify(tag, "[GNUPG:] BADSIG fixture", record, commit, signer))
    expect_rejected("untrusted signer", lambda: verify(tag, valid.replace(signer, "0" * 40), record, commit, signer))
    subkey = "2" * 40
    expect_rejected("signing subkey", lambda: verify(tag, valid.replace(signer, subkey, 1), record, commit, signer))
    expect_rejected("duplicate VALIDSIG", lambda: verify(tag, valid + "\n" + valid, record, commit, signer))
    expect_rejected("conflicting primary", lambda: verify(tag, valid[: valid.rfind(signer)] + "3" * 40, record, commit, signer))
    expect_rejected("truncated VALIDSIG", lambda: verify(tag, f"[GNUPG:] VALIDSIG {signer}", record, commit, signer))
    expect_rejected("malformed VALIDSIG date", lambda: verify(tag, valid.replace("2026-08-08", "not-a-date"), record, commit, signer))
    expect_rejected("malformed VALIDSIG timestamp", lambda: verify(tag, valid.replace("1786147200", "timestamp"), record, commit, signer))
    expect_rejected("malformed VALIDSIG class", lambda: verify(tag, valid.replace(" 00 ", " xyz "), record, commit, signer))
    expect_rejected("extra VALIDSIG field", lambda: verify(tag, valid + " unexpected", record, commit, signer))
    expect_rejected("target commit", lambda: verify(tag, valid, record, "2" * 40, signer))
    for field in sorted(REQUIRED_FIELDS):
        value = parse_message(record)[field]
        expect_rejected(
            f"authoritative record mismatch: {field}",
            lambda field=field, value=value: verify(
                tag, valid, record.replace(f"{field}: {value}", f"{field}: mismatched", 1), commit, signer
            ),
        )
    expect_rejected("finding-bearing review", lambda: verify(tag.replace("design-review-clean: true", "design-review-clean: false"), valid, record, commit, signer))
    expect_rejected("invalid ready digest", lambda: verify(tag.replace("ready-digest: blake3:", "ready-digest: invalid-"), valid, record, commit, signer))
    expect_rejected("missing field", lambda: verify(tag.replace("work-id: work-release\n", ""), valid, record, commit, signer))
    expect_rejected("extra field", lambda: verify(tag.replace("work-id: work-release", "work-id: work-release\nextra: value"), valid, record, commit, signer))
    expect_rejected("tampered unsigned payload", lambda: verify(tag.replace("review-design", "review-tampered", 1).replace("-----BEGIN PGP SIGNATURE-----", ""), "", record, commit, signer))

    workflow_path = pathlib.Path(__file__).with_name("workflows") / "release.yml"
    workflow = workflow_path.read_text()
    verify_release_workflow_wiring(workflow)
    for fragment in (
        'verification_ref="refs/agent-workbench-release-tags/$TAG_NAME"',
        '"refs/tags/$TAG_NAME:$verification_ref"',
        'git cat-file -t "$verification_ref"',
        'git verify-tag --raw "$verification_ref"',
        'git cat-file tag "$verification_ref"',
        '--authorization-record-file "$RUNNER_TEMP/release-authorization.txt"',
        "  package:\n    needs: authorize",
        "  publish:\n    needs: [authorize, package]",
    ):
        expect_rejected(
            f"release workflow wiring without {fragment}",
            lambda fragment=fragment: verify_release_workflow_wiring(workflow.replace(fragment, "", 1)),
        )

    digest = "blake3:" + "b" * 64
    ready = {
        "ready": True,
        "stateRevision": 42,
        "digest": digest,
        "context": {"focused": {
            "work": {"id": "work-release"},
            "design": {"id": "design-release"},
            "claimGaps": [],
            "criterionGaps": [],
            "designSourceGaps": [],
            "unfinishedRequiredTasks": [],
            "unresolvedAcceptedFindings": [],
        }},
    }

    def review_fixture(entry_id: str, purpose: str, target: str) -> dict[str, object]:
        manifest = []
        manifest_version = 0
        if purpose == "implementation":
            manifest_version = 3
            manifest = [{
                "kind": "implementation_target",
                "id": "tree:source",
                "snapshot": digest,
                "producerAgentRuns": ["producer"],
            }, {
                "kind": "implementation_target",
                "id": "tree:build-input",
                "snapshot": digest,
                "producerAgentRuns": ["producer"],
            }]
        return {
            "workId": "work-release",
            "currentTargetSnapshot": digest,
            "targetCurrent": True,
            "review": {"payload": {"review": {"value": {
                "context": "fresh",
                "purpose": purpose,
                "reviewerAgentRun": f"reviewer-{purpose}",
                "producerAgentRuns": ["producer"],
                "target": target,
                "targetSourceId": target.split(":", 1)[1],
                "targetSnapshot": digest,
                "targetManifestVersion": manifest_version,
                "targetManifest": manifest,
            }}}},
            "lineage": [{
                "id": f"conclusion-{purpose}",
                "payload": {"reviewConclusion": {"value": {
                    "reviewEntryId": entry_id,
                    "clean": True,
                }}},
            }],
        }

    design_review = review_fixture("review-design", "design", "design:design-release")
    implementation_review = review_fixture(
        "review-implementation", "implementation", "work:work-release"
    )
    authorization_record_from_facts(
        ready, design_review, implementation_review, commit,
        "review-design", "review-implementation",
    )
    import copy
    invalid_ready = copy.deepcopy(ready)
    invalid_ready["ready"] = False
    expect_rejected("non-ready Work", lambda: authorization_record_from_facts(
        invalid_ready, design_review, implementation_review, commit,
        "review-design", "review-implementation",
    ))
    wrong_design = copy.deepcopy(design_review)
    wrong_design["review"]["payload"]["review"]["value"]["target"] = "design:other"
    expect_rejected("wrong Design target", lambda: authorization_record_from_facts(
        ready, wrong_design, implementation_review, commit,
        "review-design", "review-implementation",
    ))
    wrong_work = copy.deepcopy(implementation_review)
    wrong_work["workId"] = "work-other"
    expect_rejected("wrong Work binding", lambda: authorization_record_from_facts(
        ready, design_review, wrong_work, commit,
        "review-design", "review-implementation",
    ))
    stale_review = copy.deepcopy(implementation_review)
    stale_review["targetCurrent"] = False
    expect_rejected("stale Review target", lambda: authorization_record_from_facts(
        ready, design_review, stale_review, commit,
        "review-design", "review-implementation",
    ))
    dependent_review = copy.deepcopy(design_review)
    dependent_review["review"]["payload"]["review"]["value"]["producerAgentRuns"] = ["reviewer-design"]
    expect_rejected("non-independent Review", lambda: authorization_record_from_facts(
        ready, dependent_review, implementation_review, commit,
        "review-design", "review-implementation",
    ))
    finding_review = copy.deepcopy(implementation_review)
    finding_review["lineage"].insert(0, {"id": "finding", "payload": {"finding": {}}})
    expect_rejected("Review containing a Finding", lambda: authorization_record_from_facts(
        ready, design_review, finding_review, commit,
        "review-design", "review-implementation",
    ))
    with tempfile.TemporaryDirectory() as directory:
        root = pathlib.Path(directory)
        subprocess.run(["git", "init", "-q"], cwd=root, check=True)
        subprocess.run(["git", "config", "user.email", "fixture@example.invalid"], cwd=root, check=True)
        subprocess.run(["git", "config", "user.name", "fixture"], cwd=root, check=True)
        (root / ".gitignore").write_text("*.ignored\n")
        (root / "source").mkdir()
        (root / "source" / "tracked.txt").write_text("current\n")
        (root / "build-input").mkdir()
        (root / "build-input" / "tracked.txt").write_text("current\n")
        (root / "unrelated").mkdir()
        (root / "unrelated" / "tracked.txt").write_text("unrelated\n")
        subprocess.run(["git", "add", "."], cwd=root, check=True)
        subprocess.run(["git", "commit", "-qm", "fixture"], cwd=root, check=True)
        fixture_commit = subprocess.run(
            ["git", "rev-parse", "HEAD"], cwd=root, check=True, text=True, capture_output=True
        ).stdout.strip()
        targets = reviewed_checkout_targets(implementation_review, "review-implementation")
        verify_target_checkout(str(root), fixture_commit, targets)

        (root / "unrelated" / "untracked.txt").write_text("not reviewed\n")
        (root / "unrelated" / "injected.ignored").write_text("not reviewed\n")
        verify_target_checkout(str(root), fixture_commit, targets)

        tracked = root / "source" / "tracked.txt"
        tracked.write_text("changed\n")
        expect_rejected(
            "tracked reviewed target divergence",
            lambda: verify_target_checkout(str(root), fixture_commit, targets),
        )
        tracked.write_text("current\n")

        tracked.write_text("staged\n")
        subprocess.run(["git", "add", "source/tracked.txt"], cwd=root, check=True)
        expect_rejected(
            "staged reviewed target divergence",
            lambda: verify_target_checkout(str(root), fixture_commit, targets),
        )
        tracked.write_text("current\n")
        subprocess.run(["git", "add", "source/tracked.txt"], cwd=root, check=True)

        untracked = root / "source" / "untracked.txt"
        untracked.write_text("injected\n")
        expect_rejected(
            "untracked reviewed target divergence",
            lambda: verify_target_checkout(str(root), fixture_commit, targets),
        )
        untracked.unlink()

        ignored = root / "build-input" / "injected.ignored"
        ignored.write_text("injected\n")
        expect_rejected(
            "ignored reviewed target divergence",
            lambda: verify_target_checkout(str(root), fixture_commit, targets),
        )
        ignored.unlink()

        invalid_manifest = copy.deepcopy(implementation_review)
        invalid_manifest["review"]["payload"]["review"]["value"]["targetManifest"] = []
        expect_rejected(
            "missing reviewed target set",
            lambda: reviewed_checkout_targets(invalid_manifest, "review-implementation"),
        )


def main() -> None:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)
    verify_parser = subparsers.add_parser("verify")
    verify_parser.add_argument("--tag-file", required=True)
    verify_parser.add_argument("--tag-object-type-file", required=True)
    verify_parser.add_argument("--verification-file", required=True)
    verify_parser.add_argument("--authorization-record-file", required=True)
    verify_parser.add_argument("--target-commit", required=True)
    verify_parser.add_argument("--allowed-signer", required=True)
    prepare_parser = subparsers.add_parser("prepare")
    prepare_parser.add_argument("--workbench", required=True)
    prepare_parser.add_argument("--project", required=True)
    prepare_parser.add_argument("--target-commit", required=True)
    prepare_parser.add_argument("--design-review-entry-id", required=True)
    prepare_parser.add_argument("--implementation-review-entry-id", required=True)
    subparsers.add_parser("self-test")
    arguments = parser.parse_args()
    if arguments.command == "self-test":
        self_test()
        return
    if arguments.command == "prepare":
        print(prepare_record(
            arguments.workbench,
            arguments.project,
            arguments.target_commit,
            arguments.design_review_entry_id,
            arguments.implementation_review_entry_id,
        ))
        return
    verify(
        pathlib.Path(arguments.tag_file).read_text(),
        pathlib.Path(arguments.verification_file).read_text(),
        pathlib.Path(arguments.authorization_record_file).read_text(),
        arguments.target_commit,
        arguments.allowed_signer,
        pathlib.Path(arguments.tag_object_type_file).read_text(),
    )


if __name__ == "__main__":
    try:
        main()
    except (AuthorizationError, AssertionError) as error:
        print(error, file=sys.stderr)
        raise SystemExit(1)
