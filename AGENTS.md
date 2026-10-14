# Agent Instructions

## Core

- Serve the stated goal; do not widen the requested scope.
- Distinguish what is confirmed from what is inferred and what is unverified.
- After changing something, verify it by a means appropriate to the change.
- Do not perform irreversible, destructive, or externally visible actions without approval.
- Apply the project's own instructions where they are more specific than these.

## Rule Routing

| When | Read |
|---|---|
| Always | ba0918-design, ba0918-placement, ba0918-readability, ba0918-secrets |
| design | ba0918-reuse |
| implement | ba0918-tdd |
| review | ba0918-verification |
| commit | ba0918-commit |
| release | ba0918-release |
| delegate | ba0918-delegation |
| diff-review | ba0918-diff-review |
| writing or revising an IR document under `docs/ir/`, or acting on a `kotowari check` finding | kotowari |
| deciding where a new request starts (use this, not ba0918-using-workflow) | kotowari-using-workflow |

Refer to each rule by its skill name. Read every rule that applies before starting the work it
governs. A rule once read stays in force for the rest of the context: read it again only after
the context has been compacted or cleared, or when the rule itself has changed. On a delegated
task, a rule the delegation prompt names as already inlined is in force from that prompt — do
not read it again; read every other rule this table routes to the work as usual.

## Project Context

Project-specific context — what this repository is, how to build and test it, and the
conventions that apply only here — lives in `PROJECT.md`. Read it before making changes.
