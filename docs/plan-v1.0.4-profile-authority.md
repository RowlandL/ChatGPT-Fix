# ChatGPT-Fix v1.0.4 版本整改计划

- 分支：release/1.0.4
- 基准：main @ 898c68e（v1.0.3 升级 bug 报告后）
- 状态：**计划阶段**——本分支仅记录整改点，暂不包含任何代码改动

## 整改点 1：profile 权威路径统一为基线内 profile（`baseline\profile\user-data`）

- **背景**：v1.0.3 将 launcher 的 `--user-data-dir` 从基线内 profile（`<baseline>\profile\user-data`）改为 program-root 稳定 profile（`<program-root>\profile\user-data`），造成：
  - 各基线（26.730 / 26.803 / 26.814）的 profile 位置分裂，登录态与桌面设置的数据来源不一致；
  - 26.814 基线内不再存在 profile 目录，profile 归属无法按基线隔离；
  - 旧版本（v1.0.1/v1.0.2）使用的基线内 profile 与新稳定 profile 并存，行为差异难以追踪。
- **决定**：以 `baseline\profile\user-data` 作为 profile 的**权威路径**（回归 v1.0.1/v1.0.2 行为，基线内隔离）。
- **涉及**：
  - launcher profile 目录解析（`generation.rs` 的 `resolve_user_data_dir` / `--user-data-dir` 逻辑）；
  - 既有稳定 profile（`<program-root>\profile\user-data`）的迁移/回退策略；
  - README / HANDOFF 等文档同步。
- **验收**：各基线使用各自基线内 profile；登录态/桌面设置随基线隔离；跨基线不共享 profile。

## 整改点 2：（待补充——后续排查中提出的整改点填入此处）

## 整改点 3：（待补充）
