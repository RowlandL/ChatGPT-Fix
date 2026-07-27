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
- Added the read-only dry-run doctor at `scripts/chatgpt-fix-doctor.ps1`.

Not done:

- No installer execution.
- No active process termination.
- No `.codex` cleanup.
- No official package modification.
- No refreshed launcher/baseline implementation yet.

## Key Finding

Current evidence points to Codex Desktop / app-server failing to reliably reap descendant tool processes. The repeated process stacks include MCP, `uv`, Python, Node, PowerShell, Open Design/Hermes, and `node_repl` children. This is separate from, but related to, the known image/base64 history growth issue.

The existing local mitigation is pinned to an older official package copy, while the registered official package has moved forward. Future work needs a launcher that dynamically discovers the latest official package and applies mitigation to an isolated side-by-side runtime copy.

## Doctor Status

The first read-only `ChatGPT-Fix doctor` command now exists:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\chatgpt-fix-doctor.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\chatgpt-fix-doctor.ps1 -Json
```

It reports:

- latest official `OpenAI.Codex` package path/version/hash;
- current launcher target and local mitigation version/state;
- version drift between official package and isolated copy;
- candidate orphaned descendants that the local launcher/baseline lifecycle model must account for;
- `.codex` session/log size risk.

Validation on 2026-07-28:

- text mode ran successfully;
- JSON mode parsed successfully with schema `chatgpt_fix.doctor.v1`;
- `read_only = true`;
- current status was `high-risk` because `.codex/logs_2.sqlite` exceeded the high threshold;
- current evidence also showed official package `26.721.4979.0`, local mitigation source `26.707.8479.0`, and lifecycle candidates present. The candidate count is a live machine snapshot and will vary between doctor runs.
- The doctor now prints lifecycle candidate summaries by process name and command kind so `python.exe` residual chains are visible instead of being hidden behind `bocha-search-mcp.exe` examples.

## Next Step

Review the doctor output, then design the first non-dry-run milestone using the same family of approach as the existing `ChatGPT-NTFS-Fix`: immutable official package, refreshed isolated baseline copy, launcher/manager-owned startup path, and explicit approval gates. A standalone watchdog should not be the primary implementation path because it is less stable.

Do not implement destructive cleanup until the dry-run output is reviewed.

## Suggested Skills

- `diagnose`: for repro loops and process/storage verification.
- `document-generate`: for updating docs after implementation.
- `careful`: before any destructive cleanup or process termination.
- `handoff`: when compacting the next session state.

## Known Workspace Caveat

`repo-control doctor` currently fails because existing project `decretum-matrix` has manifest version `beta1.0.0` while its child `VERSION` reports `beta0.5.10`. This predates `ChatGPT-Fix`; do not mix that repair into this work unless the user explicitly expands scope.
