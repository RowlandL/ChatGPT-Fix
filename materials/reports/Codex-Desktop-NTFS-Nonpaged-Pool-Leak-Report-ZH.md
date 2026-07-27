# Codex for Windows Desktop `NtFC` 非分页池泄漏：触发边界、内核栈相关性、隔离规避与安装器验收报告

报告日期：2026-07-14（Asia/Shanghai）  
主线状态：`ACTIVATION-CONTEXT BOUNDARY VERIFIED / SETUP 0.2.0.0 FROZEN / FINAL REPORT REVIEW IN PROGRESS`  
主线索引：`C:\Users\32893\codex-ntfs-fix\MAIN-GOAL-INDEX.md`

## 1. 执行摘要

本机已在两个 Codex Desktop 版本阶段观测到同一类故障：桌面主进程 `ChatGPT.exe` 产生异常高的 Other I/O，并带动 NTFS `NtFC` 非分页池持续增加，严重时可使 32 GB 主机的不可分页池超过 26 GB。

2026-07-14 对当前官方包 `OpenAI.Codex_26.707.8479.0_x64__2p2nqsd0c76g0` 完成了受保护复现、WPR 原生跟踪、TraceProcessor 池分配分析、文件 I/O 聚合、三组隔离 A/B 和 side-by-side 安装副本动态验收。当前最强结论是：

1. 官方 `IApplicationActivationManager.ActivateApplication` / AUMID `OpenAI.Codex_2p2nqsd0c76g0!App` 激活路径可快速触发 `NtFC` 增长。
2. WPR 的新增未释放 `NtFC` 分配栈明确包含 `Ntfs.sys`、`FLTMGR.SYS` 和 `bindflt.sys`。
3. 相同 WindowsApps `ChatGPT.exe` 经 `System.Diagnostics.Process.Start` 直接启动时，运行时仍可呈现相同 `PackageFullName` 与 AUMID，且同为 `AppContainer=false`、Medium Integrity；该路径在本次受保护短窗中保持稳定。
4. 因此，“运行时是否存在 package identity 或 AUMID”不是充分触发条件。当前经机器可读差分验证的边界是 activation method/context；activation、package projection、`bindflt` 与应用 I/O 模式之间的完整中间机制，以及最终缺陷所有者，均尚未确定。
5. 当前有效用户态规避是把本机官方程序复制到普通独立目录，以直接可执行文件和固定外部 profile 启动，从而避开已复现异常的官方 AUMID activation 路径。它不修改 WindowsApps、Microsoft 签名文件或内核驱动。
6. `Codex NTFS Fix Setup 0.2.0.0` 已冻结：27 项全仓回归全部通过，另有 1 项冻结产物完整生命周期通过；冻结产物还通过了注册 MSIX 真实 `install`、`verify`、固定 profile `--dry-run` 与 `uninstall`，测试前后保留的 baseline 均为 10 个进程且未被触碰。
7. 当前采集证据未显示隐藏远程控制路径、认证绕过、隐蔽持久化或数据外传。该表述是“未发现此类证据”，不等于对所有未观测路径作不存在证明。

当前产物的正式定位是 `UNSIGNED_INDEPENDENT_MITIGATION_CANDIDATE`：一项未签名、独立的用户态规避方案，不是供应商级 root-cause fix、内核修复或 OpenAI/Microsoft 官方发行物。

## 2. 结论等级

### 2.1 已证实

- 故障的主要系统表征是 `NtFC` 非分页池增长，不是普通进程工作集、页面文件或待机缓存。
- `ChatGPT.exe` 是增长活动的直接用户态触发者：历史版本暂停该进程时增长归零，恢复后立即继续。
- 当前 `26.707.8479.0` 官方 AUMID 激活在两个受保护试验中均触发 `NtFC` 增长并被精确熔断。
- 当前 WPR ETL 中有 `44,659` 个新增未释放 `NtFC` 分配，共 `10,003,616` 字节。
- PID `30252` 的 `ChatGPT.exe` 占其中 `43,697` 个分配、`9,788,128` 字节，即新增未释放字节的 `97.8459%`。
- 最大新增未释放栈明确经过 `Ntfs.sys`、`FLTMGR.SYS` 和 `bindflt.sys`。
- 同一官方二进制在三组直接启动 A/B 中不复现同类增长。
- 官方 AUMID activation 与 WindowsApps `Process.Start` 直接启动均可呈现 `PackageFullName/AUMID=present`、`AppContainer=false`、Medium Integrity；前者触发、后者稳定。
- side-by-side 安装副本在 38.2 秒受保护动态验收中 `NtFC` 净增长为 `0`，未释放分配净增长为 `0`，无熔断触发。
- Setup `0.2.0.0` 的 27 项全仓回归、1 项冻结产物生命周期和注册 MSIX 真实隔离生命周期均为 `PASS`；运行中的 10 进程 baseline 未变化。

### 2.2 当前证据支持的高置信边界

在已测试主机、Windows 构建和 Codex `26.707.8479.0` 的短窗试验中，异常在官方 AUMID activation 路径复现；同一 WindowsApps 二进制经 `Process.Start` 直接启动时仍保留 package identity、AUMID、非 AppContainer 与 Medium IL，却保持稳定。由此可把当前边界收敛到 activation method/context；package identity 只能作为运行时属性记录，不能单独解释触发。ETL 同时显示新增未释放 `NtFC` 栈包含 `Ntfs.sys`、`FLTMGR.SYS` 与 `bindflt.sys`，但 activation 到该内核栈之间的完整中间机制仍未证明。

### 2.3 尚未证明

- 不能仅凭现有证据断言缺陷只存在于 `bindflt.sys`、只存在于 Codex，或由其中一方单独负责。
- 尚未获得 Microsoft 私有符号，因此内核模块内部仍为偏移地址，未解析到函数名。
- 尚未定位到 Codex/Electron 的精确源码调用点。
- 尚未在多种 Windows 正式发行构建和多台硬件上交叉复现。
- 动态内核验收为数十秒级；冻结 `0.2.0.0` 的注册 MSIX 生命周期使用 launch dry-run，并不替代更长时间真实任务验收。
- `launch-probe.json` 已固化 activation API、实际命令行、映像、package identity、AUMID、AppContainer 和完整性级别，但它不能直接观测 Windows activation、package projection、Bind Filter 与应用文件 I/O 之间的内部转换链。
- “未发现后门、认证绕过、隐蔽持久化或外传证据”是当前证据集的负面发现，不是对任意未知代码路径的形式化不存在证明。

因此，正式措辞应使用“已验证的触发边界/交互路径”，不应写成“已经证明是某一个驱动的单点代码缺陷”。

## 3. 测试环境

| 项目 | 值 |
| --- | --- |
| 当前官方 Codex 包 | `OpenAI.Codex_26.707.8479.0_x64__2p2nqsd0c76g0` |
| 官方 AUMID | `OpenAI.Codex_2p2nqsd0c76g0!App` |
| 官方主程序 | `C:\Program Files\WindowsApps\OpenAI.Codex_26.707.8479.0_x64__2p2nqsd0c76g0\app\ChatGPT.exe` |
| 历史严重复现包 | `OpenAI.Codex_26.707.8168.0_x64` |
| 物理内存 | 31.9 GB |
| `bindflt.sys` 文件版本 | `10.0.26100.8655` |
| `bindflt.sys` SHA-256 | `5D9AF...BF83B`（完整值保留在本机证据记录） |
| Bind Filter altitude | `409800` |
| 工程候选默认根目录 | `%LOCALAPPDATA%\Programs\Codex-NTFS-Fix\baseline` |

## 4. 历史严重故障：`26.707.8168.0`

2026-07-13 的首次系统级故障提供了跨重启前的严重性基线：

| 指标 | 观测值 |
| --- | ---: |
| 不可分页池最高值 | 26.89 GB |
| 可用物理内存最低值 | 约 0.35 GB |
| `NtFC` 增长速度 | 约 9 至 12 MB/秒 |
| `ChatGPT.exe` Other I/O | 约 104,000 至 119,000 次/秒 |
| 15 秒 `NtFC` 增长 | 173.4 MB |
| 15 秒新增未释放分配 | 811,801 |
| 平均每项大小 | 约 224 字节 |

历史暂停/恢复隔离试验：

| 阶段 | 时长 | `NtFC` 增长 | 新增分配 | 释放 |
| --- | ---: | ---: | ---: | ---: |
| PID 16676 正常运行 | 3 秒 | 26.09 MB | 122,150 | 20 |
| PID 16676 暂停 | 3 秒 | 0.00 MB | 0 | 0 |
| PID 16676 恢复后 | 约 0.75 秒 | 5.86 MB | 27,448 | 13 |

该试验证明桌面主进程活动与 `NtFC` 增长存在直接因果关系，但当时尚未取得原生池分配栈。2026-07-14 的 8479 试验补齐了这部分证据。

## 5. 当前官方版受保护复现

### 5.1 内核保护器设计

保护器不使用总内存或 87% 内存占用作为主触发条件。每秒只检查：

- `NtFC` 单样本字节增长是否达到 `1 MiB`；
- `NtFC` 单样本未释放分配增长是否达到 `10,000`；
- 精确目标主进程 Other I/O 是否达到 `50,000 次/秒`。

发生任一内核泄漏或异常 I/O 条件后，保护器继续采集 5 秒证据，再通过精确可执行文件路径终止目标进程树。它不会终止仍在运行的 side-by-side baseline，也不依赖模糊进程名匹配。

实现文件：

- `C:\Users\32893\codex-ntfs-fix\scripts\test-kernel-leak-guard.ps1`
- `C:\Users\32893\codex-ntfs-fix\scripts\start-protected-kernel-test.ps1`
- `C:\Users\32893\codex-ntfs-fix\tests\kernel-leak-guard.tests.ps1`

### 5.2 第一轮 AUMID 复现

证据目录：

`C:\Users\32893\codex-ntfs-fix\evidence\official-ab\20260714-032439-6d429ead`

| 指标 | 结果 |
| --- | ---: |
| 样本数 | 19 |
| 全运行窗口 | 22.830 秒 |
| `NtFC` 净增长 | 27,033,440 字节（约 25.78 MiB） |
| 未释放分配净增长 | 120,685 |
| 主进程 Other I/O 峰值 | 188,028.2103 次/秒 |
| 主进程 Other I/O 平均值 | 70,792.4097 次/秒 |
| 触发原因 | `NTFC_BYTES_GROWTH`、`NTFC_OUTSTANDING_GROWTH` |
| 结果 | 采集后精确终止官方目标树 |

保护器的约 5.095 秒触发证据窗口内，`NtFC` 增长约 21.546 MiB，约为 4.229 MiB/秒。

### 5.3 第二轮 AUMID + WPR 复现

证据目录：

`C:\Users\32893\codex-ntfs-fix\evidence\official-native-trace\20260714-032925-26ccd2f7`

| 指标 | 结果 |
| --- | ---: |
| 样本数 | 8 |
| 运行窗口 | 9.859 秒 |
| `NtFC` 净增长 | 9,916,704 字节 |
| 未释放分配净增长 | 44,271 |
| 主进程 Other I/O 峰值 | 50,906.9596 次/秒 |
| 触发 | 是 |
| 结果 | 采集后终止官方目标树并保存 ETL |

WPR ETL：

- 路径：`C:\Users\32893\codex-ntfs-fix\evidence\official-native-trace\20260714-032925-26ccd2f7\official-native.etl`
- 大小：6,269,435,904 字节
- SHA-256：`115652B8437C0A0CC97D604D0126EA7BA6EF94028E4A815D183971EB9A8DAAF3`

这取代了旧报告中“当前权限无法录制 WPR”的过时状态。

## 6. TraceProcessor 池分配归因

使用 Microsoft TraceProcessor 依赖构建了本地只读分析器：

- `C:\Users\32893\codex-ntfs-fix\tools\PoolTraceAnalyzer\PoolTraceAnalyzer.csproj`
- `C:\Users\32893\codex-ntfs-fix\tools\PoolTraceAnalyzer\Program.cs`

主分析输出：

`C:\Users\32893\codex-ntfs-fix\evidence\official-native-trace\20260714-032925-26ccd2f7\pool-analysis-csharp-v2.json`

| 指标 | 值 |
| --- | ---: |
| 匹配 `NtFC` 分配 | 170,797 |
| 匹配总字节 | 38,187,184 |
| 新增未释放分配 | 44,659 |
| 新增未释放字节 | 10,003,616 |
| 已释放分配 | 1,846 |
| 已释放字节 | 390,848 |
| 分析耗时 | 60.068 秒 |

最大新增未释放栈：

```text
ChatGPT.exe PID 30252
  -> ntoskrnl.exe
  -> Ntfs.sys
  -> FLTMGR.SYS
  -> bindflt.sys+0x2477
  -> bindflt.sys+0x1A6A3
  -> FLTMGR.SYS
  -> ntoskrnl.exe
```

该栈共有 `43,697` 次分配、`9,788,128` 字节，占新增未释放字节的 `97.8459%`。

符号分支运行成功，但没有下载到可用的 Microsoft 符号，模块内部仍保留为偏移。模块链本身来自 ETL 栈，不依赖函数名解析。

## 7. 文件 I/O 与静态代码审查

文件 I/O 输出：

`C:\Users\32893\codex-ntfs-fix\evidence\official-native-trace\20260714-032925-26ccd2f7\pool-file-analysis-pid30252.json`

PID `30252` 共匹配 `143,432` 个文件操作。主要路径包括：

| 路径/类别 | 操作数 | 字节/错误 |
| --- | ---: | ---: |
| 官方 `resources\app.asar` | 4,389 | 29,494,551 字节 |
| package `LocalCache` 根目录 | 3,128 | 24 个错误 |
| `LocalCache\Roaming\codex\web` | 3,080 | 24 个错误 |
| `LocalCache\Roaming\codex` | 3,080 | 24 个错误 |
| `.codex\keybindings.json` | 808 | 486 字节、162 个错误 |
| 多个 Chromium LevelDB/Extension/Shader cache 路径 | 数百级 | 多为查询/创建/关闭 |

静态审查报告：

`C:\Users\32893\codex-ntfs-fix\evidence\static-8479\STATIC-MAIN-IO-ANALYSIS.md`

结论：

- 未发现能独立解释泄漏的 JavaScript 无退避高速文件循环。
- `keybindings.json` 是确定的启动/焦点放大项，但 `808 / 143,432 = 0.5633%`，不应作为主修复。
- 每次菜单重建约读取 keybindings 41 次；162 个错误大致对应 4 次菜单重建。
- 可选优化插入点是 `main-gIZ5Kjoy.js` 原始 byte offset `1361814`：复用已读 keymap 可把每次约 41 次访问降到 1 次。
- 该 ASAR 优化未实施，因为它不是主根因，且修改官方/重打包内容会扩大兼容和更新风险。

Chromium/浏览器插件、Codex 插件、skills、Git watcher 和 ASAR 访问可能增加 I/O，但三组直接启动 A/B 证明它们不是触发该泄漏的充分条件。

文件 I/O 与池分配来自同一 ETL 和同一 PID，但当前分析没有把某一条 FileIO 事件与某一条 `NtFC` allocation 一一关联；表中路径只能称为“同时观测到的访问面”，不能直接写成这些路径导致了对应池分配。

## 8. 隔离 A/B 与运行时身份差分

三组试验均使用同一官方 8479 `ChatGPT.exe`，只改变启动边界和 profile 位置；全部在同一内核保护器下运行。

| 试验 | profile 条件 | 运行窗口 | `NtFC` 净增长 | 未释放净增长 | Other I/O 峰值 | 触发 |
| --- | --- | ---: | ---: | ---: | ---: | --- |
| 直接启动 A | 新普通外部 profile | 22.990 秒 | 3,584 字节 | 16 | 9,203.4652/s | 否 |
| 直接启动 B | 显式官方 package profile 路径 | 21.885 秒 | 3,584 字节 | 16 | 12,557.5297/s | 否 |
| 直接启动 C | 完整官方 profile 克隆到普通目录 | 23.354 秒 | 3,584 字节 | 16 | 15,415.8481/s | 否 |

完整 profile 克隆包含 1,153 个文件、156,572,912 字节，源/目标哈希一致。

这组结果说明：

- 在这三个约 22 秒直接启动窗口内，profile 内容不是该异常的充分条件；尚未排除 profile 是放大因子、延迟触发因子或特定长期状态因素。
- 在这些短窗条件下，同一官方二进制本身也不是充分触发条件。
- 进一步的运行时身份差分证明，直接启动路径仍可具有 package identity 与 AUMID；当前边界是 activation method/context，尚未证明其中 AUMID API、package projection、Bind Filter 或应用调用模式的最终责任分配。

### 8.1 官方 activation 与 WindowsApps 直接启动对照

两份机器可读 `launch-probe.json` 对同一官方 WindowsApps 映像给出如下差分：

| 字段 | 官方 AUMID activation | WindowsApps `Process.Start` |
| --- | --- | --- |
| 启动 API | `IApplicationActivationManager.ActivateApplication` | `System.Diagnostics.Process.Start` |
| `ChatGPT.exe` SHA-256 | `28C3E8B6C55FFF39ECB12A5EB27F493ABF997804247517AA7A46C277CA5D9E93` | 相同 |
| `PackageFullName` | `present` | `present` |
| AUMID | `present` | `present` |
| AppContainer | `false` | `false` |
| Integrity | Medium，RID `8192` | Medium，RID `8192` |
| 受保护短窗结果 | `TRIGGERED` | `PASS_NO_TRIGGER` |

这项对照给出的正向事实是：官方 AUMID activation 触发，而相同 WindowsApps EXE 经 `Process.Start` 可在保留 package identity、AUMID、非 AppContainer 与 Medium IL 的同时不触发。当前边界位于 activation method/context；这些运行时身份属性不是充分条件，完整中间机制和最终缺陷所有者未定。

## 9. 修复原理

side-by-side 修复不对官方程序做二进制补丁。它执行以下动作：

1. 在本机发现最新官方 OpenAI Codex x64 包。
2. 把官方 `app` 目录复制到普通、独立、可写的用户级安装路径。
3. 校验 `ChatGPT.exe` 和 `resources\app.asar` 的 SHA-256。
4. 从普通路径直接启动 `ChatGPT.exe`，不使用官方 AUMID。
5. 固定独立 Electron profile：`profile\user-data`。
6. 继续使用当前用户的 `.codex` 业务数据、skills 和 CLI 会话面，不删除或迁移这些内容。

默认路径：

```text
%LOCALAPPDATA%\Programs\Codex-NTFS-Fix\baseline\
├─ app\
├─ profile\user-data\
├─ state.json
└─ setup.lock
```

本修复避开的是已复现异常的官方 AUMID activation 路径，而不是在泄漏发生后靠总内存阈值关闭程序。因此日常运行不依赖熔断器。由于规避路径同时改变 activation、程序目录与 profile 位置，它是独立用户态 mitigation；它没有把 package identity 的有无认定为根因，也不构成供应商级最终修复。

## 10. 冻结 Setup `0.2.0.0`

### 10.1 当前产物

- 工作区：`C:\Users\32893\codex-ntfs-fix\dist\release-0.2.0\Codex-NTFS-Fix-Setup.exe`
- D 盘交付：`D:\新建文件夹\Codex-NTFS-Fix-Setup.exe`
- 版本：`0.2.0.0`；ProductVersion：`0.2.0-local-mitigation`
- 大小：69,632 字节
- SHA-256：`E8A9C1C238513F6240C4F90622F4CCECAE67A14002A3BD93DA0A04C9407FB3CD`
- Authenticode：`NotSigned`
- 定位：`UNSIGNED_INDEPENDENT_MITIGATION_CANDIDATE`，不是 OpenAI/Microsoft 官方包，也不是供应商级 root-cause fix。
- 被替换旧版备份：`D:\新建文件夹\Codex-NTFS-Fix-Setup.previous-20260714-090747.exe`
- 旧版版本/大小/SHA-256：`0.1.0.0` / 37,376 字节 / `67BF7CE72FF4558B2D090106E005C9C4AA3872FFCC9205B7F1E533587A7550AF`

### 10.2 双击行为

无参数/双击时：

1. 自动发现最新官方 Codex x64 包；
2. 校验包注册、identity、架构、publisher identity、部署路径和 `AppxSignature.p7x`；
3. 安装或修复独立副本，并校验 `ChatGPT.exe` 与 `resources\app.asar` 哈希；
4. 固定隔离 profile 后启动；
5. 已安装且关键哈希一致时执行 `VERIFIED_NO_OP`，不重复复制、创建备份或重写状态，直接启动。

默认无参数入口为 WinExe GUI，并提供源版本、独立路径、profile、进度、结果与修复提示。Setup 还建立开始菜单入口、HKCU 卸载登记与受管 Setup 自副本。

显式命令：

```text
--inspect-source
--install [--source-app <app-dir>] [--install-root <dir>]
--verify  [--install-root <dir>]
--launch  [--install-root <dir>] [--dry-run]
--list-backups [--install-root <dir>]
--rollback-latest [--install-root <dir>]
--uninstall [--install-root <dir>] [--purge-profile]
```

### 10.3 可恢复性

- 更新前复制到 staging，完成完整性检查后再替换。
- 旧安装进入带 manifest 的可恢复备份；新安装失败时恢复旧版本，并可列出备份或一键回滚最新完整恢复点。
- 目标源不完整或关键文件被篡改时拒绝安装。
- 卸载默认保留隔离 profile；只有显式 `--purge-profile` 才清理。
- `--user-data-dir` 不允许被透传参数覆盖；managed root/app/profile 遇 reparse point 时 fail closed。
- 安装、更新、回滚和卸载使用独占锁；同一精确路径受管实例运行时返回 `MANAGED_INSTANCE_RUNNING`，不会自动终止。
- 诊断使用脱敏轮转 JSONL，不记录凭据、会话正文、主机名或网络标识。
- 安装器不嵌入、下载或重新分发官方 Codex 二进制。
- 其他机器必须已安装官方 Codex，或由用户显式提供可信 `--source-app`。

官方配置/profile 的可恢复备份：

- 路径：`C:\Users\32893\codex-ntfs-fix\backups\20260714-031733-official-ab-prelaunch-625c93ee`
- 文件数：1,160
- manifest SHA-256：`63ADBF11F503CB923BE4FB23F5775DAD534B5DF86BD5409D9B6E453659F71172`

### 10.4 测试矩阵

当前生产化验证结果：

- 27 项全仓 Windows PowerShell 回归全部 `PASS`，覆盖安装、更新、失败回滚、no-op、GUI/双击语义、shell integration、来源/签名门禁、固定 profile、reparse 拒绝、运行中冲突、长路径、保护器 plan-only 等面。
- 另有 1 项冻结产物生命周期 `PASS`：`install -> verify -> list verified backups -> rollback -> profile-preserving uninstall -> reinstall -> purge uninstall`。
- 冻结 Setup 对真实注册的官方 MSIX 在独立临时根完成 `install -> verify -> fixed-profile launch --dry-run -> uninstall`，全部 `PASS`；未启动桌面应用。
- 真实注册 MSIX 生命周期测试前后，用户要求保留的 baseline 均为 10 个进程，创建时间与路径绑定项保持不变，未被终止、覆盖或迁移。
- 真实官方长路径验收仍为 `PASS`：源最长路径 277 字符、目标最长路径 296 字符，复制及关键文件哈希一致。

## 11. 用户态规避路径的动态内核与 profile 隔离验收

证据：

`C:\Users\32893\codex-ntfs-fix\evidence\setup-dynamic-acceptance\20260714-042129-4f554f6c\acceptance-result.json`

| 指标 | 结果 |
| --- | ---: |
| Setup 启动退出码 | 0 |
| 样本数 | 29 |
| 总采样窗口 | 38.203 秒 |
| `NtFC` 起始字节 | 37,524,480 |
| `NtFC` 结束字节 | 37,524,480 |
| `NtFC` 净增长 | 0 |
| 未释放分配净增长 | 0 |
| 最大单样本 `NtFC` 增长 | 0 |
| 主进程 Other I/O 峰值 | 23,652.5946 次/秒 |
| 主进程 Other I/O 平均值 | 3,554.6758 次/秒 |
| 熔断触发 | 0 |
| 目标树异常终止 | 0 |
| 结果 | `PASS` |

该次早期同机制受管副本验收结束后只终止临时验收副本，用户要求保持运行的既有 baseline 未被触碰。此处 `PASS` 验证用户态规避路径在该 38.2 秒窗口内的内核指标，不应倒推成冻结 `0.2.0.0` 已完成长时运行验证。

早期启动日志曾出现：

```text
Ignoring late userData path change after native startup.
Requested 'C:\Users\32893\AppData\Roaming\Codex\web\Codex',
selected '<managed-root>\profile\user-data'.
```

后续按绑定 PID 执行的 runtime inspector 已关闭该证据门禁：requested 与 effective `userData` 完全一致；隔离目录新增 213 项，官方 projected profile 与官方 roaming profile 前后均为 0 项变化。Setup `0.2.0.0` 也已拒绝透传第二个 `--user-data-dir`。这证明本次探针中的实际落盘隔离有效，但不扩大为所有未来 Electron 版本的永久保证。

## 12. 为什么不是 Chromium/插件或 skills 的单独缺陷

正式结论不是“这些组件绝对无关”，而是“它们不是充分根因”：

- WPR 文件聚合确实显示 Chromium LevelDB、Extension State、Shader cache、ASAR 和 `.codex` 文件访问。
- 静态代码也显示菜单刷新和插件状态扫描存在放大空间。
- 但使用完整克隆 profile 的同一官方二进制在约 23 秒直接启动窗口内仍然稳定。
- 如果某项 profile 内容或插件本身是短窗充分条件，完整克隆 profile 的直接启动应有相近增长；实际仅 `+3,584` 字节、`+16` 未释放分配且无触发。

因此，插件/skills/Chromium 访问可作为次级性能优化对象，不能取代对官方 AUMID activation 路径的主规避；尚未排除它们在更长窗口中成为放大因素。

## 13. Setup `0.2.0.0` 冻结状态与剩余发布门禁

单文件 `0.2.0.0` 已完成 WinExe GUI、源包预检、磁盘空间检查、开始菜单入口、HKCU 卸载登记、受管 Setup 自副本、脱敏日志、staging/原子替换、验证备份、一键回滚、运行中精确路径拒绝、固定 profile、reparse fail-closed、Windows PowerShell 5.1 相对路径兼容和 UTF-8 BOM manifest 兼容。27+1 回归与注册 MSIX 真实隔离生命周期已通过，D 盘候选也已完成哈希一致的原子替换。

尚未关闭的发布级门禁为：

- 正式代码签名及签名后的再验收；当前 Authenticode 为 `NotSigned`，另一台机器可能显示 SmartScreen/未知发布者警告。
- 干净 VM 与第二台独立 Windows 主机的安装、修复、卸载和功能回归。
- 显式备份保留/清理命令；当前已能列出与恢复验证备份，但尚无最终保留策略自动化。
- 冻结 `0.2.0.0` 的长时真实任务、多对话、skills、本地工具和官方更新后回归。
- 最终中英公开报告、厂商 Bug 提交 Markdown/PDF 与独立门下复核。

D 盘当前候选为 `D:\新建文件夹\Codex-NTFS-Fix-Setup.exe`，SHA-256 `E8A9C1C238513F6240C4F90622F4CCECAE67A14002A3BD93DA0A04C9407FB3CD`；被替换的 `0.1.0.0` 已保留为 `D:\新建文件夹\Codex-NTFS-Fix-Setup.previous-20260714-090747.exe`。

## 14. 残余风险

- 当前可移植性只在本机和模拟/真实长路径源上验证，尚未在第二台独立 Windows 主机完成安装验收。
- 当前冻结候选未签名，可能触发 SmartScreen；没有代码签名证书时只能披露，不能伪造可信发布者。
- 官方包目录结构或 Electron 启动语义未来变化时，发现/复制/启动逻辑可能需要更新。
- 直接启动路径绕开 MSIX 激活，也可能绕开未来依赖 package identity 的官方功能；需要在真实功能回归中持续核查。
- 完整 activation 中间机制和最终缺陷所有者仍未确定；不能把当前规避候选称为供应商级根因修复。
- 目前没有 Microsoft 私有符号，不能把 `bindflt` 偏移映射到函数名。
- 当前动态内核验收为数十秒级；冻结 Setup 的注册 MSIX 测试为 launch dry-run，仍应补充长时真实任务、多对话和官方更新后的回归。
- 27+1 与本机注册 MSIX 生命周期不能替代干净 VM、第二台机器、正式签名和签名后组合验证。
- 原始 ETL 与文件分析包含本机用户名和路径；对外提交前需另做路径隐私审查，不能把“无会话正文”等同于可公开原始 ETL。
- “未发现后门、认证绕过、隐蔽持久化或数据外传证据”不等于形式化证明不存在；后续若扩大代码或来源范围，应重新审查。
- `NotSigned` 与跨机器验证未完成前，不应把该独立候选标为正式 1.0 或官方发行版。

## 15. 建议补充的发布级回归

在现有 27+1 与注册 MSIX 生命周期通过的基础上，发布级验证还应补充：

- 双击首次安装、已安装 no-op、修复、官方版本更新、失败注入回滚和卸载；
- 长路径、空间不足、源缺失、源不完整、目标被篡改和运行中冲突；
- 多轮真实对话、打开 skills、执行本地工具、切换窗口和多会话；
- `NtFC` 在预热后不出现单调线性增长；
- 未释放分配不持续累积；
- Other I/O 不在持续窗口内回到官方 AUMID 复现中的 50k 至 188k/s 异常区间；瞬时峰值只能作辅助，应同时报告平均值或 P95，并以 `NtFC` 与未释放分配斜率为主判据；
- 功能不因降低 I/O 而缺失；
- 不修改 WindowsApps、系统驱动、CC Switch 数据库或用户 `.codex` 业务数据；
- 所有安装/更新状态均可恢复并有脱敏日志。

## 16. 证据索引与哈希

| 证据 | 路径 | SHA-256 |
| --- | --- | --- |
| 主线目标索引 | `C:\Users\32893\codex-ntfs-fix\MAIN-GOAL-INDEX.md` | 写入后动态更新，以当前文件哈希为准 |
| WPR ETL | `...\official-native-trace\20260714-032925-26ccd2f7\official-native.etl` | `115652B8437C0A0CC97D604D0126EA7BA6EF94028E4A815D183971EB9A8DAAF3` |
| 池分析 | `...\pool-analysis-csharp-v2.json` | `34DFF4C19A3CD0CD9A41F79D37F7AB2EA83CEF91EAFA45B50F79C970594334A0` |
| 文件 I/O 分析 | `...\pool-file-analysis-pid30252.json` | `83AFE104E7536ACCE02228D63F0230D13084D1578E78010F40DDE9158EDBD3BC` |
| 静态审查 | `...\static-8479\STATIC-MAIN-IO-ANALYSIS.md` | `AE9B4E74811BC138D95C6C395D02DABF27F47461A4F8B9091000EC84E7521C98` |
| Setup 动态验收 | `...\setup-dynamic-acceptance\20260714-042129-4f554f6c\acceptance-result.json` | `298A6C696663C586E38644B6173FD55E06AF7AECF80BAA602B057F6726AC7150` |
| 官方 AUMID activation probe | `C:\Users\32893\codex-ntfs-fix\evidence\protected-launch-probe\20260714-053815-0f720fac\launch-probe.json` | `137B22FE93751B2586E175E30F7F68AB31C019A818095A3D5E4CD745EE0D92A8` |
| WindowsApps `Process.Start` probe | `C:\Users\32893\codex-ntfs-fix\evidence\profile-isolation-probe\runs\20260714-054942-1833b7a4\launch-probe.json` | `555FAB502220514D63BE1291581E9A03F63F6F5E08FDA1BCDBD810015AEE34CB` |
| 冻结产物生命周期测试 | `C:\Users\32893\codex-ntfs-fix\tests\setup-built-artifact-lifecycle.tests.ps1` | `3E45F39C07236B44BFA6F8D78C4D6174BFA81F6D68188EDF50392BEF1ECF22BE` |
| 冻结 Setup `0.2.0.0`（工作区） | `C:\Users\32893\codex-ntfs-fix\dist\release-0.2.0\Codex-NTFS-Fix-Setup.exe` | `E8A9C1C238513F6240C4F90622F4CCECAE67A14002A3BD93DA0A04C9407FB3CD` |
| 冻结 Setup `0.2.0.0`（D 盘交付） | `D:\新建文件夹\Codex-NTFS-Fix-Setup.exe` | `E8A9C1C238513F6240C4F90622F4CCECAE67A14002A3BD93DA0A04C9407FB3CD` |
| 被替换 `0.1.0.0` 备份 | `D:\新建文件夹\Codex-NTFS-Fix-Setup.previous-20260714-090747.exe` | `67BF7CE72FF4558B2D090106E005C9C4AA3872FFCC9205B7F1E533587A7550AF` |

旧报告原文已备份到：

`C:\Users\32893\codex-ntfs-fix\backups\20260714-043619-report-pre-20260714-rewrite\Codex-Desktop-NTFS-Nonpaged-Pool-Leak-Report-ZH.md`

备份 SHA-256：`A2C9F98480DF564E1CEB42B80EBB4AC9FAAECBC2B2124A06F0E82CE92B639791`

## 17. 最终判断

本轮已经从“内存逐步增长的表征”推进到可重复的内核级触发、ETL 调用链、同二进制 A/B 和有效规避路径：

```text
官方 IApplicationActivationManager/AUMID activation
  -> ChatGPT.exe 异常高文件系统活动
  -> 同一 ETL 中观测到 Ntfs/FLTMGR/bindflt 栈上的 NtFC 新增未释放分配
  -> 完整中间机制与最终缺陷所有者尚未确定

相同 WindowsApps EXE 经 Process.Start
  -> PackageFullName/AUMID 仍 present，AppContainer=false，Medium IL
  -> 本次受保护短窗不触发

普通目录直接启动本机官方程序副本 + 固定外部 profile
  -> 绕开该触发边界
  -> 当前动态验收 NtFC 净增长为 0
```

关键区分已从“package identity 是否存在”收敛到 activation method/context。`0.2.0.0` 单文件候选已冻结并完成 27+1、本机注册 MSIX 生命周期和 D 盘交付哈希验证；其性质仍是未签名独立用户态 mitigation，不是供应商级 root-cause fix。主线剩余工作是公开/厂商报告、PDF、独立复核以及签名与跨机器发布门禁。

当前证据没有显示隐藏远程控制、认证绕过、隐蔽持久化或数据外传，但该负面发现不应被扩写成“已形式化证明绝无后门”。本报告不包含提示词、会话正文、凭据、token、cookie、私密二维码、微信 ID 或私有文件内容。
