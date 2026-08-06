# ChatGPT-Fix Doctor v2 Probe Boundaries Proposal

> P2 阶段产出（`docs/chatgpt-fix-doctor-v2-proposal.md`）。本文只描述未来 doctor v2 的
> probe 接口与授权门，不实现、不运行 doctor，不修改旧 `scripts/chatgpt-fix-doctor.ps1`。

## 1. 目的

P1/P2 阶段确立了 `chatgpt_fix.live_inspection.v1`（package/manifest/EXE/shortcut 只读快照）
与 `chatgpt_fix.doctor.v2` schema（`src/chatgpt-fix-core/src/doctor.rs`，仅 schema 与
`DoctorV2` 类型，无读取实现）。本提案定义未来 doctor v2 在 P7 前如何扩展为真正可运行的
健康检查，并把读取面拆分为独立 probe，每个 probe 绑定独立授权门。

## 2. 原则

- **不运行**：P2 不执行 `scripts/chatgpt-fix-doctor.ps1`，也不启动任何 doctor v2 逻辑。
- **不修改旧 doctor**：`scripts/chatgpt-fix-doctor.ps1` 不在 P2 产品写集内，保持原样。
- **读取边界**：任何 probe 只读；不修改 WindowsApps、shortcut、配置、`.codex`、进程状态。
- **授权门**：每个 probe 类别必须显式取得对应 A 编号授权后才能接线（见下表）。

## 3. 未来 probe 拆分（P7 之前的提案接口）

| Probe | 读取类别 | 需要授权 | 输出契约 |
|---|---|---|---|
| `probe-package` | `Get-AppxPackage -Name OpenAI.Codex`、InstallLocation、AppxManifest、primary EXE metadata/hash | A2-P2（已定义） | `chatgpt_fix.live_inspection.v1` |
| `probe-shortcut` | `ChatGPT.lnk` TargetPath/Arguments/WorkingDirectory/IconLocation | A2-P2（已定义） | 并入 live_inspection 的 shortcut 字段 |
| `probe-process` | 进程树/父链/命令行（仅所有权归属分析） | A5（P6 受控验证） | `chatgpt_fix.process_tree.v1`（提案） |
| `probe-effective-config` | MCP allowlist、CODEX_HOME、history 配置（只读提案 diff） | A6（P7） | `chatgpt_fix.effective_config.v1`（提案） |
| `probe-history` | `.codex/sessions` 体积、最大 JSONL、`logs_2.sqlite`、图片痕迹（dry-run） | A6（P7）+ 独立维护授权 | doctor 报告条目（只读） |

## 4. Doctor v2 组合器

未来 `DoctorV2` 组合器只做：

1. 按授权门收集已接线 probe 的规范化 JSON；
2. 汇总为 `chatgpt_fix.doctor.v2`（`overall_status` 取 ok/warning/high-risk）；
3. 输出 `would_*` 建议（如 `would_cover_by_lifecycle_policy`），**绝不输出 `would_kill`**；
4. 不自动清理、不 kill、不写配置；清理动作必须独立授权。

## 5. P2 边界声明

- P2 只交付本 proposal 与 `DoctorV2` schema/类型 + Manager `doctor` 子命令占位
  （未授权时 exit 2）。
- P2 不读取 process、`.codex`、`CODEX_HOME`、package `LocalState`、auth/token/SQLite/MCP 配置。
- `processes_inspected=false`、`codex_home_inspected=false`、`local_state_inspected=false`
  在 `live_inspection.v1` 中固定为 `false`。

## 6. 验收

- [ ] 本 proposal 未引入任何 live 读取实现。
- [ ] `scripts/chatgpt-fix-doctor.ps1` 未修改。
- [ ] 每个未来 probe 都有独立授权门与输出契约。
- [ ] `DoctorV2` schema 保持 P2 冻结内容，不随本提案变更。
