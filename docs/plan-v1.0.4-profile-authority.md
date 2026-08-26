# ChatGPT-Fix v1.0.4 版本整改计划（最终定版）

- 分支：release/1.0.4
- 基准：main @ 898c68e（v1.0.3 升级 bug 报告后）
- 状态：**已定版**——v1.0.4 = 26.818 基线 + 基线内 profile（整改点 1）+ locale 中文，**不集成 NTC**（NTC 存在切换会话/流式输出卡顿 bug，见整改点 2 结论）。整改点 1/2 实现已提交；本定版明确 NTC 暂时不可用。

## 定版结论（重要）

v1.0.4 最终定版**不含可用的 NTC（Token 用量插件）**。A/B 实测确认：26.818 上**关闭 NTC 后切换会话不再卡顿**，而**开启 NTC（无论带不带 PATCH2 状态缓存）切换会话仍卡**。根因是 NTC 的 syncHubVisibility 反复改写 HUD DOM（style.display/dataset），会话切换时触发 26.814+/26.818 桌面端自带的 ResizeObserver 布局循环。**该插件自身修复尚未彻底完成，故 v1.0.4 默认不集成、不启用**；注入脚本与修复补丁保留在仓库（后续重新启用用）。

## 整改点 1：profile 权威路径统一为基线内 profile（baseline-profile-user-data）✅

- **背景**：v1.0.3 将 launcher 的 --user-data-dir 从基线内 profile（<baseline>/profile/user-data）改为 program-root 稳定 profile（<program-root>/profile/user-data），造成各基线 profile 位置分裂、登录态不一致、新旧并存难追踪。
- **决定**：以 baseline-profile-user-data 作为 profile 的权威路径（回归 v1.0.1/v1.0.2 行为，基线内隔离）。
- **实现**：generation.rs profile_dir 改为 baseline.join("profile").join("user-data")（commit 524f967）。
- **验收**：各基线使用各自基线内 profile；跨基线不共享。

## 整改点 2：NTC 切换会话卡顿（诊断 + 修复补丁，但定版不启用）⚠️

- **诊断**：26.818 + NTC 下切换会话 UI 卡顿。定位为 NTC 的 syncHubVisibility 每次执行都重写 HUD 的 style.display/dataset/aria-hidden；会话切换 DOM 重建触发 NTC 全局 MutationObserver → 反复改 display → 触发 26.818 桌面端自带的 ResizeObserver 布局循环（日志 620 行、切换时 82/分）→ 自激循环 → 渲染器主线程饱和。26.730/26.803 前端无该 RO 监听故不触发。
- **A/B 验证**：关闭 NTC 后切换会话**不卡**；开启 NTC（含修复补丁）仍卡 → **确证 NTC 是卡顿源，且当前修复不彻底**。
- **修复补丁（保留备用）**：inject 脚本 PATCHES[2] 加 hubVisibilityLastVisible 状态缓存（可见性不变则跳过全部 DOM 写）+ 打包改 createPackageWithOptions 无 unpack（native 留 body）+ unpackedDir 固定名 app.asar.unpacked + 幂等重注入（commit 4d064bb/96f8f5b1）。
- **定版决定**：因修复不彻底，**v1.0.4 不集成 NTC**，作为「已知限制」记录；待后续修复完成再重新启用。

## 整改点 3：发布产物与安装方式完善 ✅

- README 版本 v1.0.3 → v1.0.4，功能说明更新，已知限制新增 NTC bug 条目（v1.0.4 不集成）。
- 记录 NTC injector 标准注入流程 + native 保留打包方式。
- 保留 v1.0.3 发布契约（快捷方式、install 支持 inject-native-token-cost.js、hash/SBOM/receipt 仅就实际构建产物生成）。
