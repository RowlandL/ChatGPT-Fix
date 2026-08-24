# ChatGPT-Fix v1.0.2

此文件记录 v1.0.2 的发布范围与交付契约。它不是构建 receipt，不包含尚未实际生成的工件 hash、SBOM、测试数量或签名证明。

## 修复内容

- **Issue #2**：全新安装时缺少本项目自有 injector，导致 Token 用量 HUD 不能可靠完成注入。v1.0.2 将自有 injector 作为发布 payload 的第 5 项，并由安装布局提供给 Manager。
- **Issue #3**：安装器仅创建 `ChatGPT-Fix-Launcher` 入口，缺少预期的 `ChatGPT` 开始菜单入口。v1.0.2 要求安装与 repair 路径均维护两个快捷方式：`ChatGPT-Fix-Launcher` 与 `ChatGPT`。

## 发布 payload

v1.0.2 的发布包应包含 5 项自有 payload：

1. `ChatGPT-Fix-Launcher.exe`
2. `ChatGPT-Fix-Manager.exe`
3. `ChatGPT-Fix-Packer.exe`
4. `ChatGPT-Fix-Setup.exe`
5. 本项目自有 injector

第 5 项是本项目自有 injector，不是第三方 Token 用量 userscript。第三方 userscript 继续不随本发布包分发，运行时按其独立上游来源与许可获取。官方 OpenAI 包同样不包含在发布包内，也不得改写 `WindowsApps`。

## 发布状态与不可变历史

- v1.0.2 为 **unsigned** 发布；本说明不代表任何工件已经构建、签名或验证。
- 只有实际构建完成后，才能生成并独立复核 v1.0.2 的工件 hash、SBOM、测试结果与 build receipt。
- 不得改写、替换或重新发布 v1.0.1 的 assets、checksums、receipt、SBOM 或任何历史记录。
