# Performance Scan Pipeline Optimization

**Cycle ID:** `20260408032659`
**Started:** 2026-04-08 03:26:59
**Status:** 🟢 Completed
**Issue:** 20260408021037_performance-scan-pipeline-optimization

---

## 📝 What & Why

Reduce avoidable work in the common status and TUI scan paths. The current implementation repeatedly resolves paths while walking merged trees and performs left/right scans plus content fetches serially even when those operations are independent.

## 🎯 Goals

- Cut repeated tree lookups during `compute_status_from_trees()` without changing status semantics.
- Overlap independent scanner work so TUI diff-filter scans spend less wall-clock time waiting on I/O.
- Preserve current diff verification behavior with focused regression coverage around metadata and content-compare refinement.

## 📐 Design

### Files to Change

```text
src/
  service/status.rs - precompute file-path unions with side presence/metadata so the main status loop avoids repeated path re-resolution
  runtime/scanner.rs - run side scans and both-side content fetches concurrently where ownership and runtime constraints allow it
  cli/status.rs - verify no call-site adjustments are needed after status service changes
  cli/diff.rs - verify no call-site adjustments are needed after status service changes
  cli/merge.rs - verify no call-site adjustments are needed after status service changes
  cli/sync.rs - verify no call-site adjustments are needed after status service changes
```

### Key Points

- **Status computation hot path**: replace repeated `find_node*` calls per path with a traversal that carries the data needed to derive status in one pass, then reuse that path-level data for the later content-compare selection step.
- **Scanner parallelism**: split `run_scan()` into independent left/right worker tasks, each with its own blocking boundary/runtime ownership, then join them before building trees; after paths needing comparison are known, fetch left/right byte batches on separate workers and merge the results by path.
- **Behavior lock-in**: extend existing unit tests in `src/service/status.rs` and add scanner-focused tests if the refactor introduces new pure helpers.

## ✅ Tests

- [ ] `cargo test service::status -- --nocapture`
- [ ] Add coverage for mixed loaded/unloaded paths and unchanged metadata outcomes after the status traversal refactor.
- [ ] Add coverage for scanner content-compare orchestration or extracted helper logic if concurrency requires new seams.
- [ ] Verify `cli/status.rs`, `cli/diff.rs`, `cli/merge.rs`, and `cli/sync.rs` still compile unchanged against the status service API.

## 🔒 Security (if applicable)

- [ ] Preserve existing path handling and avoid changing trust boundaries.
- [ ] Keep content comparison byte-accurate for binary files.
- [ ] Ensure concurrent scan/content fetch logic still disconnects SSH clients cleanly on success and error paths.

## 📊 Progress

| Step | Status |
|------|--------|
| Tests | 🟢 |
| Implementation | 🟢 |
| Commit | ⚪ |

**Legend:** ⚪ Pending · 🟡 In Progress · 🟢 Done

---

**Next:** Cycle complete. Commit remains manual because direct commits on `main` are blocked by the `commit` skill.
