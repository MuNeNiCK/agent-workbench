# Lean and assurance

Read this page when deciding whether a project property should use Lean, auditing a proof receipt, or
diagnosing why a receipt became stale. Lean is one evidence mechanism inside Workbench, not the
product's workflow or a promise that the whole project is formally verified.

## When Lean is useful

Select Lean for a precise design property whose meaning can be represented as a proposition and
witness. Use command or artifact evidence for deployment, performance, UX, third-party behavior,
filesystem observations, and other external facts.

The coding agent constructs the mapping and proof input. The user does not have to install Lean or
translate every requirement into a theorem.

## Claim contents

Each selected claim binds:

- one exact design statement and its text-to-proposition mapping;
- a proposition and witness declaration;
- explicit allowed kernel assumptions;
- a proof root and declared source inputs;
- a configured preparation check; and
- the product's one fully qualified pinned toolchain identity.

The mapping remains visible because Lean cannot prove that natural-language intent was translated
completely or correctly.

## What happens during proof execution

Workbench derives the complete source and package-configuration closure, then protects every normal
non-toolchain Lake output involved in the proof. It rebuilds those inputs without cache, removes the
fresh compiled outputs from normal lookup paths, and runs a generated checker whose import path can
see only the isolated fresh outputs plus the pinned toolchain.

The checker verifies the witness at the declared proposition and compares its actual kernel axiom
dependencies with the explicitly declared assumptions. The configured project check runs separately;
an exit-zero command alone cannot substitute for kernel acceptance.

Source/configuration identity is checked before build, after build, and after kernel/configured
checking. Existing build outputs are restored after successful and ordinarily failing operations.
Concurrent public proof operations in one project are serialized before baseline capture, preventing
overlapping Lake output layouts from being active together.

## Receipt currentness

A receipt counts only while all of these remain current:

- exact claim structure and statement text;
- proposition, witness, assumptions, mapping, and check definition;
- proof root and ordered source/configuration closure;
- pinned toolchain identity;
- focused Work and accepted DesignRevision binding.

Absolute temporary-directory prefixes are not part of the source identity, so relocating an otherwise
identical project and proof tree does not invalidate it. Relative paths, contents, configuration,
dependencies, or claim inputs do.

## What kernel acceptance establishes

The named witness has the named proposition under exactly the declared kernel assumptions for the
current declared source input.

## What it does not establish

Lean does not establish:

- that natural language completely captures user intent;
- that the text-to-proposition mapping is complete;
- that assumptions or external observations are true in the world;
- that SQLite, filesystem, process, network, repository, or command behavior is truthful;
- that an agent or reviewer is unbiased; or
- that the project is globally minimal or free of AI slop.

Workbench keeps these boundaries explicit and uses current external evidence where theorem proving
would be dishonest. It applies the same claim mechanism to selected propositions about its own
production decision functions.
