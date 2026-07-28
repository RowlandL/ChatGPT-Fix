# ChatGPT-Fix

Local evidence and handoff repository for the Codex Desktop / ChatGPT Desktop mitigation review.

This repository captures the July 2026 local audit of three related failure surfaces:

- Codex Desktop / ChatGPT Desktop child processes, especially MCP/`uv`/Node/Python process stacks, remaining after task activity.
- Codex session and log growth caused by repeated history compaction and inline image/base64 payloads.
- A wrapper-based mitigation strategy that preserves the official OpenAI package while keeping a local patch effective after official updates.

## Repository Map

| Path | Purpose |
|---|---|
| `docs/审查文档.md` | Main review document with findings, evidence, and recommended architecture. |
| `docs/HANDOFF.md` | Current handoff: where the work stopped and what the next session should do. |
| `docs/修复升级执行方向报告.md` | Execution-direction report for upgrading the current ChatGPT-NTFS-Fix-style mitigation. |
| `records/conversation-visible.md` | Redacted visible user/assistant conversation export from the local Codex session. |
| `records/local-audit-snapshot.md` | Local host/process/version/storage snapshot gathered during review. |
| `records/external-references.md` | Public issue and official documentation reference snapshot. |
| `records/external-reference-snapshot.json` | Machine-readable GitHub issue metadata snapshot. |
| `scripts/chatgpt-fix-doctor.ps1` | Read-only dry-run doctor for refreshing local package, launcher, process, and `.codex` risk evidence. |
| `materials/reports/` | Previous bilingual NTFS/nonpaged-pool reports and PDFs. |
| `materials/screenshots/` | Screenshot supplied by the user for the image/base64 storage issue. |
| `materials/installers/` | Local mitigation installer artifact, stored for traceability only. |
| `checksums/SHA256SUMS.txt` | SHA-256 hashes for committed materials. |

## Current Status

The review phase is complete, and the first no-side-effect doctor prototype is available. No installer is executed, no active process is terminated, no logs are cleaned, and no official package files are modified by the doctor.

Run the doctor from this repository with:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\chatgpt-fix-doctor.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\chatgpt-fix-doctor.ps1 -Json
```

The doctor reports:

- latest registered official `OpenAI.Codex` package path, version, and `ChatGPT.exe` SHA-256;
- current ChatGPT shortcut target and local mitigation state;
- version drift between the official package and local isolated baseline;
- candidate MCP/`uv`/Node/Python/PowerShell descendants that the local lifecycle model must account for;
- `.codex` session and `logs_2.sqlite` size risk.

The next useful step is a reviewed `ChatGPT-NTFS-Fix`-style launcher/baseline implementation that:

- discovers the latest registered official `OpenAI.Codex` package on each launch;
- prepares an isolated side-by-side runtime copy when the official version changes;
- applies local mitigations only to the isolated runtime or launcher environment;
- accounts for MCP/`uv`/Node/Python/PowerShell lifecycle from the launcher-owned isolated runtime rather than relying on a separate watchdog as the primary mechanism;
- caps or externalizes image-heavy history/log payloads without corrupting active sessions.
