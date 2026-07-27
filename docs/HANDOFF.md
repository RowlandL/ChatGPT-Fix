# Handoff

This handoff captures the current state for the next Codex session working on `D:\project\ChatGPT-Fix`.

## Where This Stopped

The review and archive phase is complete.

Completed:

- Created independent child repository at `D:\project\ChatGPT-Fix`.
- Registered `ChatGPT-Fix` in `D:\project\workspace.yaml`.
- Copied prior bilingual NTFS/nonpaged-pool reports and PDFs into `materials/reports/`.
- Copied the user-provided screenshot into `materials/screenshots/`.
- Copied the local mitigation installer into `materials/installers/` for traceability only.
- Exported a redacted visible conversation transcript to `records/conversation-visible.md`.
- Generated local host/process/storage evidence in `records/local-audit-snapshot.md`.
- Generated public reference snapshots in `records/external-references.md` and `records/external-reference-snapshot.json`.
- Wrote the main Chinese review document at `docs/审查文档.md`.

Not done:

- No installer execution.
- No active process termination.
- No `.codex` cleanup.
- No official package modification.
- No wrapper/watchdog implementation yet.

## Key Finding

Current evidence points to Codex Desktop / app-server failing to reliably reap descendant tool processes. The repeated process stacks include MCP, `uv`, Python, Node, PowerShell, Open Design/Hermes, and `node_repl` children. This is separate from, but related to, the known image/base64 history growth issue.

The existing local mitigation is pinned to an older official package copy, while the registered official package has moved forward. Future work needs a launcher that dynamically discovers the latest official package and applies mitigation to an isolated side-by-side runtime copy.

## Next Step

Start with an implementation plan for a no-side-effect dry-run wrapper.

Recommended first milestone:

- Build a read-only `ChatGPT-Fix doctor` command that reports:
  - latest official `OpenAI.Codex` package path/version/hash;
  - current launcher target and local mitigation version;
  - version drift between official package and isolated copy;
  - candidate orphaned descendants that would be reclaimed by watchdog rules;
  - `.codex` session/log size risk.

Do not implement destructive cleanup until the dry-run output is reviewed.

## Suggested Skills

- `diagnose`: for repro loops and process/storage verification.
- `document-generate`: for updating docs after implementation.
- `careful`: before any destructive cleanup or process termination.
- `handoff`: when compacting the next session state.

## Known Workspace Caveat

`repo-control doctor` currently fails because existing project `decretum-matrix` has manifest version `beta1.0.0` while its child `VERSION` reports `beta0.5.10`. This predates `ChatGPT-Fix`; do not mix that repair into this work unless the user explicitly expands scope.

