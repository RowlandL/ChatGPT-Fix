# ChatGPT-Fix v1.0.3

此文件记录 v1.0.3 的发布范围与交付契约。它不是构建 receipt，不包含尚未实际生成的工件 hash、SBOM、测试数量或签名证明。

## 修复内容

- **Issue #4（跨机器启动降级与连续对话卡死）**：
  - 启动清洗不再"重新指向"保留市场 `openai-bundled`，而是整体移除该外部声明（保留 `[plugins."*@openai-bundled"]` 启用段）。桌面 app-server 对保留名的任何外部注册都会拒绝并在每次窗口聚焦时重试，形成 marketplace/add 失败风暴；移除声明后风暴消失（本机验证：修复后 0 次失败）。
  - 旧版 `windows.sandbox = "low"` 归一为 `elevated`（保持原有归一逻辑）。
  - 用户数据目录从基线内迁出到 `<program-root>/profile/user-data`：基线更换不再重置外观/桌面设置与登录状态；首次启动一次性迁移旧 profile（同卷 rename），迁移标记防重复。
  - 新增 `[desktop]` 段快照/恢复（`preserve_desktop_section`）：配置管理器（如 cc-switch）重写 config.toml 会丢弃非托管段，导致外观/语言重启丢失；Launcher 每次启动刷新快照，发现段丢失时自动写回，不触碰配置管理器的数据。
- **快捷方式"点了没反应"**：应用关窗后进程树可能以无窗口后台态残留，单实例守卫静默复用导致无可见反馈。Launcher 现在在找不到可见窗口时优雅关闭残留实例并重新拉起；`tasklist`/`taskkill` 调用加 `CREATE_NO_WINDOW`，不再闪现控制台窗口。
- **中文界面离线持久化（新工具）**：桌面应用只有从 chatgpt.com 拉取到 Statsig layer（`enable_i18n`）时才翻译界面，断网/网络受限时缓存过期即回退英文。新增专用修复程序 `ChatGPT-Fix-Locale.exe`：`fix` 将渲染器入口 bundle 内 `enable_i18n` 默认值改为开启（等长字节替换），`status` 查询注入状态。幂等、失败关闭、注入前备份 `app.asar.pre-locale` 并写入 `locale-commit.json`；配合 `scripts/inject-locale-i18n.js`（依赖安装目录本地运行时 `ntc\node_modules\@electron\asar`）。

## 发布 payload

v1.0.3 的发布包应包含 6 项自有 payload：

1. `ChatGPT-Fix-Launcher.exe`
2. `ChatGPT-Fix-Manager.exe`
3. `ChatGPT-Fix-Packer.exe`
4. `ChatGPT-Fix-Locale.exe`（新增）
5. `ChatGPT-Fix-Setup.exe`
6. 本项目自有 injector（`scripts/inject-native-token-cost.js`，另含 `scripts/inject-locale-i18n.js`）

第 6 项是本项目自有 injector，不是第三方 Token 用量 userscript。第三方 userscript 继续不随本发布包分发，运行时按其独立上游来源与许可获取。官方 OpenAI 包同样不包含在发布包内，也不得改写 `WindowsApps`。

## 发布状态与不可变历史

- v1.0.3 为 **unsigned** 发布；本说明不代表任何工件已经构建、签名或验证。
- 只有实际构建完成后，才能生成并独立复核 v1.0.3 的工件 hash、SBOM、测试结果与 build receipt。
- 不得改写、替换或重新发布 v1.0.1 / v1.0.2 的 assets、checksums、receipt、SBOM 或任何历史记录。
