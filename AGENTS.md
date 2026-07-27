# ChatGPT-Fix Agent Notes

## Scope

This repository is an evidence and design handoff repository for the local Codex Desktop / ChatGPT Desktop mitigation work. It is not the official OpenAI package, and it must not modify files under `C:\Program Files\WindowsApps`.

## Safety Rules

- Treat all materials as local diagnostic evidence.
- Do not commit raw `.codex` session JSONL unless the user explicitly requests it and a redaction pass has been completed.
- Do not record or echo API keys, tokens, cookies, private IDs, or auth files.
- Do not run `materials/installers/Codex-NTFS-Fix-Setup.exe` unless the user explicitly authorizes execution.
- Keep official Codex package contents immutable. Any future mitigation should use a wrapper, isolated side-by-side copy, or launcher layer.
- Keep this child repository independent from `D:\project` root git. Do not add it as a submodule.

## Current Focus

The next implementation pass should design and verify a wrapper that survives official Codex updates while preserving official package integrity and applying local lifecycle/log mitigations.
