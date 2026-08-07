# ChatGPT-Fix 项目交接书（HANDOFF）

> 用途：把 ChatGPT-Fix 子项目 P1 会话（thread 019fb87a-35ac-7102-9d02-44a797b10d8d，标题「ChatGPT修复｜P1离线核心与EXE」）交给另一个智能体继续。本文件是项目正规交接文档，保存于 `docs/HANDOFF.md`（旧研究阶段交接内容已被本版取代，历史版本保留在 Git 记录中）。
> 交接日期：2026-08-06。生成方：Codex（`D:\project` 根控制面）。

## 1. 当前任务一句话

P1 阶段（离线核心：Rust workspace、schemas、fixtures、三个 EXE 的离线 dry-run）产品与构建证据已完成，接受事务停在 `ROOT_GOVERNANCE_COMMITTED`；下一步是完成 receipt commit、release/P2 refs、sync、canonical 对齐，并冻结 P1→P2 handoff。本恢复轮另按 2026-08-06 旨意把 A0q 旁路扩展（repowiki 知识库 + QoderCLI + Hermes + squad）写入了两书（未提交）。

## 2. 必须遵守的边界（最高优先级）

- P1 离线边界：不读取 live package/process/`.codex`；不运行任何 doctor（产品 doctor 与 `repo-control doctor` 都因 P1 禁令跳过，并在 receipt 显式记录）；不运行安装器；不修改 shortcut/WindowsApps/配置/用户 Rust cache/toolchain/live native overlay/helper task。
- 写集：只允许 P1 worktree 内两书、Rust 源码/测试/构建回执（本阶段已冻结）、`%TEMP%` 下 journal/handoff、task-local ignored `out/`、本地 commits/refs、`.repo-control` task/event 元数据。不得把 canonical child（`D:\project\ChatGPT-Fix`）当工作目录，只在接受后做 clean release 对齐（本交接文档落盘 canonical 是用户 2026-08-06 显式指令，属唯一例外）。
- 根与子 Git index 始终保持无暂存；根既有 dirty/untracked（`.gitignore`、`workspace.yaml` 无关改动、`docs/` 等）必须原样保留，不得清理、不得混入提交。
- 不 push/tag/PR/发布；无 remote。
- A0q 只读旁路：附件仅 `计划书.md`、`执行书.md`；调用前 secret/private-ID scan；QoderCLI 必须 `--print --no-session-persistence --tools ""`；Hermes 非交互 `hermes -z "<prompt>" -m <model> --provider deepseek --cli`；Qoder/Hermes 尽量经 squad（`C:\Tools\bin\squad.exe`）登记/编排；squad 状态只落被忽略的 `.squad/` 与 `locks/*.receive.lock`，`squad init` 自动注入 AGENTS/CLAUDE/GEMINI 说明必须在验收前撤回。
- 辅助模型：QoderCLI `Qwen3.8-Max`、`DeepSeek-V4-Flash`、`DeepSeek-V4-Pro`；Hermes `provider=deepseek`（本机默认 `deepseek-v4-flash`）。辅助知识库 `D:\project\.qoder\repowiki` 只读参考，不构成执行授权。
- 宿主投递故障（重要）：本会话验证 `spawn_agent` 初始消息、`followup_task`、`send_message` 的任务正文全部不可达；fork 上下文虽可达但会被子代理当成主线程并擅自改动文件（已发生两次并全部隔离回退）。若环境未修复，禁止派生子代理，用 `serial_inline` 并在 receipt 记录 `host_dispatch_delivery_failed`。

## 3. 当前冻结状态（2026-08-06 核验）

| 项 | 值 |
|---|---|
| P1 acceptance journal | `H:\TEMP_~1\ChatGPT-Fix-P1-acceptance-journal.json`，state=`ROOT_GOVERNANCE_COMMITTED`，SHA-256=`5669DC8ED8F313556C30AF15A26B3EC243A8F92FB7142AA2C5F8E3A47F894402` |
| child candidate | `f75f0f458b0d8cc81874a22b1ba44924a2a1e16b`（commit 只改 `VERSION` 0.2.0→0.3.0） |
| root governance | `c509c4e03e320fd691a6de12b69428d43d01680a`（只改 workspace.yaml 中 ChatGPT-Fix `current` 单行；patch SHA-256=`45b73743c38838a4dca7da1a632524fe8cba0420769849307674357898ae3c25`） |
| current-work root ref | `codex/ChatGPT-Fix/work/p1-offline-core` = `62660c9b9327c4fba47a02502778421e95f08b55`（不可推进） |
| P1 工作树 | `D:\project\worktrees\ChatGPT-Fix\chatgpt-fix-p1-20260731-7d7cf4c0`，branch `work/p1-offline-core`，HEAD=`f75f0f4`，index 空；仅 `计划书.md`、`执行书.md` 有未暂存改动（A0q 扩展） |
| canonical child | `D:\project\ChatGPT-Fix`，branch `release/0.2.0`；本交接文档为 canonical `docs/HANDOFF.md` 未暂存改动（用户显式指令） |
| 根仓库 | `D:\project`（project/develop），既有 dirty/untracked 保留；本恢复轮未新增其他 root 改动 |

## 4. 测试（P1，task-local/offline）

`cargo metadata/fmt/clippy -D warnings/test/build --release --target x86_64-pc-windows-msvc` 全部 PASS；workspace 65/65 tests PASS；Packer/Manager canonical plan 字节一致；Launcher `would_start=false` 且 live launch exit 4；三个 release EXE size/SHA、typed receipts、Cargo.lock、license/SPDX 校验 PASS（详见执行书 §13 与工作树 records）。

## 5. 授权消费

- 已消费：A0、A0q（2026-08-06 扩展）、A1、A10-NTC delegated evidence 登记范围。
- 未授予：A1n、A2（含 A2-P2）、A3、A4a、A4b、A5-A7、A8a-A8c、A9。P2 开始前必须取得 A2 明确授权并写入执行书。

## 6. 仓库外写入

- `H:\TEMP_~1\ChatGPT-Fix-P1-acceptance-journal.json`：state=`VERIFIED`，doctor=`SKIPPED_P1_PROHIBITION`，SHA-256=`B64F0FAFBDB327A63794C8D404AAAC3D869AFA0E57123947CCF6B512013ED268`（含 accepted/refs/sync 全部 OID 与 11 个 branch-sync 事件路径）。
- `D:\project\.repo-control\events\ChatGPT-Fix\branch-sync\sync-branch-ref-*.json`：7 个新事件（全部 `preserved_existing`）。
- 根 `D:\project` 既有 dirty/untracked（.gitignore、workspace.yaml 无关项目字段、docs/ 等）原样保留，未进入任何提交。
- Qoder advisory 输出：`H:\TEMP_~1\ChatGPT-Fix-P1-advisory-qoder-20260806.txt`（有条件 PASS，三项建议已处理）；Hermes advisory 输出为空文件（超时未完成，未重跑）。

## 7. rollback

P1 只有 Git commits/refs 与 ignored task-local build outputs，无 live 系统事务。回退路径：删除/重置 child release/0.3.0 与 work/p2-live-inspection refs 及对应 root catalog refs（不动 current-work `62660c9b`），root manifest 用 bounded commit 恢复 `0.2.0`。

## 8. 下一任务命令（P2，等 START_GOAL 后执行）

```powershell
$TaskRoot = (git rev-parse --show-toplevel).Trim()
$ChildRoot = Join-Path $TaskRoot 'attached\ChatGPT-Fix'
python (Join-Path $TaskRoot 'repo-control\repo_control.py') materialize-current `
  --root-branch codex/ChatGPT-Fix/work/p2-live-inspection `
  --task chatgpt-fix-p2-live-inspection-20260806-<HHMMSS> `
  --attach-current
```

验证 task-shell workspace.yaml `version.current=0.3.0`、attached child OID=`3e77503...`、root ref/OID、根/子 index 干净后回报 `HANDOFF_ACCEPTED`，等待 `START_GOAL phase=P2 start_key=<hash>`，只创建一次 Goal。

## 9. 阻塞项

1. 本恢复会话无 `create_thread`/`list_projects`/`list_threads` 工具：P2 task 需用户/宿主按执行书 §9 创建。
2. Hermes advisory 超时未重跑（可选，P2 可按 A0q 重试并记录短标签）。
3. canonical child `docs/HANDOFF.md` 保持未提交（用户指令例外；含真实 UUID，禁止提交）。
4. 宿主子代理消息投递故障（`host_dispatch_delivery_failed`）若未修复，P2 继续 `serial_inline` 并记录。

---

> **恢复记录（2026-08-06 16:5x）：** 本文件前 30 行（§1-§3 及冻结状态表）为本会话 jsonl 中 `head -30` 的原始字节恢复；§4-§9 由已冻结的 `%TEMP%\ChatGPT-Fix-P1-to-P2-handoff-20260806.md` 提炼重建（该 handoff SHA-256 绑定同一内容）。真实 source thread ID `019fb87a-35ac-7102-9d02-44a797b10d8d` 保留在本文件，禁止提交入 Git。
