# Codebase Review Follow-up: Security Boundary Hardening

**Cycle ID:** `20260408021006`
**Started:** 2026-04-08 02:10:06
**Status:** 🟡 In Progress

---

## 📝 What & Why

Address the highest-priority findings from the 2026-04-08 codebase review. The immediate goal is to close trust-boundary bugs that can cause path escape, silent host-key trust, request-content leakage, and incorrect subtree merging before taking on lower-priority performance and cleanup work.

## 🎯 Goals

- Eliminate local and runtime path-boundary escapes in file and subtree handling.
- Restore secure host-key verification behavior and stop leaking request payloads into debug logs.
- Fix partial-tree merge correctness so valid branches are not silently dropped.

## 📐 Design

### Files to Change

```text
src/
  merge/executor.rs - Harden local root validation and canonical root handling before file I/O.
  runtime/side_io.rs - Reject absolute subpaths and re-check resolved paths under root.
  ssh/host_key_verifier.rs - Remove silent auto-accept fallback from interactive Ask mode.
  agent/server.rs - Replace raw request debug logging with redacted summaries.
  cli/merge.rs - Deduplicate partial tree roots by stable full path rather than basename.
tests/
  add or extend regression coverage for path escape, host-key behavior, request redaction, and tree merge correctness.
```

### Key Points

- **Canonical trust boundaries**: Any path derived from user or config input must be resolved against a canonical root, or rejected before use if the root cannot establish a stable boundary.
- **No implicit SSH trust downgrade**: TUI `Ask` behavior must not silently become TOFU/auto-accept. Either explicit confirmation or a hard failure is preferable to invisible trust.
- **Safe observability**: Logging should preserve operation type and path context without serializing raw request payloads, file bytes, or secrets.
- **Correct tree identity**: Partial tree merge logic must key by full relative path, not basename, to avoid dropping distinct branches with the same leaf name.

## ✅ Tests

- [ ] Add regression coverage for relative `root_dir` path-escape attempts in local file I/O.
- [ ] Add tests for rejecting absolute subpaths outside the configured project root.
- [ ] Add tests for host-key `Ask` mode to ensure unknown keys are not silently trusted.
- [ ] Add tests or assertions for redacted agent request logging summaries.
- [ ] Add partial-tree merge tests with duplicate basenames in different directories.

## 🔒 Security (if applicable)

- [ ] Canonicalize and validate all local path boundaries before file access.
- [ ] Prevent silent trust downgrade for unknown SSH host keys.
- [ ] Ensure logs never contain raw file contents or secret-bearing request bodies.

## 📊 Progress

| Step | Status |
|------|--------|
| Tests | 🟢 |
| Implementation | 🟢 |
| Commit | ⚪ |

**Legend:** ⚪ Pending · 🟡 In Progress · 🟢 Done

---

**Next:** Write tests → Implement → Commit with `claude-skills:commit`
