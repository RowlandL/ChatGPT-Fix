# ChatGPT-Fix v1.0.4 版本整改计划

- 分支：release/1.0.4
- 基准：main @ 898c68e（v1.0.3 升级 bug 报告后）
- 状态：**整改中**——整改点 1 已提交（524f967），整改点 2 已实现并本机验证（26.818 + PATCH2 后切换会话无卡顿）；整改点 3 为文档/发布收尾

## 整改点 1：profile 权威路径统一为基线内 profile（baseline-profile-user-data）

- **背景**：v1.0.3 将 launcher 的 --user-data-dir 从基线内 profile（<baseline>/profile/user-data）改为 program-root 稳定 profile（<program-root>/profile/user-data），造成：
  - 各基线（26.730 / 26.803 / 26.814）的 profile 位置分裂，登录态与桌面设置的数据来源不一致；
  - 26.814 基线内不再存在 profile 目录，profile 归属无法按基线隔离；
  - 旧版本（v1.0.1/v1.0.2）使用的基线内 profile 与新稳定 profile 并存，行为差异难以追踪。
- **决定**：以 baseline-profile-user-data 作为 profile 的**权威路径**（回归 v1.0.1/v1.0.2 行为，基线内隔离）。
- **涉及**：
  - launcher profile 目录解析（generation.rs 的 resolve_user_data_dir / --user-data-dir 逻辑）；
  - 既有稳定 profile（<program-root>/profile/user-data）的迁移/回退策略；
  - README / HANDOFF 等文档同步。
- **验收**：各基线使用各自基线内 profile；登录态/桌面设置随基线隔离；跨基线不共享 profile。
- **实现**：generation.rs profile_dir 改为 baseline.join("profile").join("user-data")（已提交 524f967）。

## 整改点 2：NTC HUD 可见性状态缓存（修复切换会话卡顿）

- **背景**：26.818 基线 + 修复后 NTC（rAF 批处理 + 500ms 节流，已提交 86feff2）下，**切换会话时 UI 仍会卡顿**。诊断确认：
  - NTC 的 syncHubVisibility 在每次执行时都会重写 HUD 的 dataset.cltcHubVisible / aria-hidden / style.display（none 与空切换）；
  - 会话切换时 DOM 大面积重建 → NTC 全局 MutationObserver 触发 → syncHubVisibility 反复改 display → **改 display 改变布局/尺寸 → 触发 26.818 前端自带的 ResizeObserver（官方 bug，日志 620 行，切换时 82/分）→ RO 回调又改尺寸 → 自激循环** → 渲染器主线程饱和 → 卡顿。
  - 26.730 / 26.803 前端无该 ResizeObserver 监听，故不触发（NTC 本身不用 ResizeObserver，NTC 非直接元凶；是 NTC 的 DOM 写与官方 RO 的交互放大）。
- **改动**：给 syncHubVisibility 加**可见性状态缓存**——if (state.hubVisibilityLastVisible === visible) return visible;，可见性未变化时**跳过全部 DOM 写**，切断"改 display 与 ResizeObserver"自激循环。**行为保持**（该显该隐照旧），只去掉无谓的 DOM 写入。
- **Patch 位置**：inject-native-token-cost.js 的 PATCHES 新增 [2]（syncHubVisibility state cache）。
- **打包与原生模块**：注入脚本第 4 步改用 createPackageWithOptions(work, outAsar, {})（**无 unpack**）。已实测验证：unpack glob 会同时 unpack 掉 tslib 等纯 JS 依赖导致加载失败；**无 unpack 打包**让 native（better-sqlite3.node 等）留在 asar body、tslib 等 JS 依赖可解析、app 正常启动（better-sqlite3 加载成功、NTC loader 生效）。
- **验收**：26.818 基线 + PATCH2 注入后，多次切换会话无卡顿；native 正常加载；NTC HUD 显示正确。

## 整改点 3：发布产物与安装方式完善（v1.0.4 发布契约）

- README 版本号 v1.0.3 → v1.0.4，并补充 PATCH2 行为说明。
- 记录 NTC injector 作为发布物的标准注入流程（含 native 保留的打包方式说明）。
- 保留 v1.0.3 的发布契约（快捷方式、install 支持 scripts/inject-native-token-cost.js、hash/SBOM/receipt 仅就实际构建产物生成）。
