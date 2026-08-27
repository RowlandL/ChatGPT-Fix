// ChatGPT-Fix Setup — .NET WinForms installer (based on the initial
// Codex-NTFS-Fix installer: VERIFY -> INSTALL -> CONFIGURE -> LAUNCH).
// Compile in a normal terminal (not inside the AI tool sandbox):
//   cd <dir>
//   "C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe" /nologo /target:winexe /out:ChatGPT-Fix-Setup.exe /win32manifest:app.manifest SetupForm.cs
// Requires .NET Framework 4.x (preinstalled on Windows; no SDK needed).
using System;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using System.Windows.Forms;
using Microsoft.Win32;

// Product identity for the published artifact: the PE version resource
// (file properties) comes from these assembly attributes, so the Setup
// exe reports 1.0.5 just like VERSION and the registry DisplayVersion.
[assembly: System.Reflection.AssemblyVersion("1.0.5.0")]
[assembly: System.Reflection.AssemblyFileVersion("1.0.5.0")]
[assembly: System.Reflection.AssemblyInformationalVersion("1.0.5")]
[assembly: System.Reflection.AssemblyProduct("ChatGPT-Fix")]
[assembly: System.Reflection.AssemblyTitle("ChatGPT-Fix Setup")]

namespace ChatGPTFixSetup
{
    public class SetupForm : Form
    {
        private readonly Label[] _steps = new Label[4];
        private readonly Label _status;
        private readonly ProgressBar _progress;
        private readonly Button _install;
        private readonly Button _uninstall;
        private readonly Button _cancel;
        private volatile bool _running;
        private bool _ntcWarning;
        private bool _selfCleanupScheduled;

        private const string LocalAppData = @"%LOCALAPPDATA%\Programs\ChatGPT-Fix";

        // True when running as the CLI automation mode (--test-install):
        // there is no window handle, so ALL UI updates must be skipped
        // (BeginInvoke on a handle-less control can throw and abort the copy).
        private static bool _cliMode;
        internal static void SetCliMode(bool cli) { _cliMode = cli; }

        // --- Test hooks (opt-in, default OFF) -------------------------------
        // Two channels (BOTH default OFF, production behaviour unchanged):
        //   1. A sidecar file "test-config.json" sitting NEXT TO the running
        //      Setup.exe (e.g. inside a throwaway _iso_test folder). This is
        //      the RELIABLE channel: no env-var inheritance needed, so a CLI
        //      subprocess launched by an external tool cannot accidentally
        //      target the real installation. Format:
        //        { "install_root": "...", "shell_root": "...", "uninstall_subkey": "...",
        //          "package_root": "..." }
        //   2. Env vars CODEX_NTFS_FIX_SETUP_TESTING=1 + TEST_INSTALL_ROOT /
        //      TEST_SHELL_ROOT / TEST_UNINSTALL_SUBKEY (fallback).
        //   Optional sidecar field "skip_launch": "1" makes --test-install
        //   skip the final Launcher start, so an automated re-verification
        //   can never hijack (or kill) a live ChatGPT instance. Production
        //   has no test-config.json and always launches.
        private sealed class TestConfig
        {
            public string install_root;
            public string shell_root;
            public string uninstall_subkey;
            public string package_root;
            public string skip_launch;
            public bool enabled;
        }

        private sealed class PayloadEntry
        {
            public string Source;
            public string RelativeDestination;
        }

        private sealed class PayloadDeploymentReceipt
        {
            public string InstallRoot;
            public string BackupRoot;
            public PayloadEntry[] Entries;
            public bool[] Existed;
        }

        private sealed class BoundedProcessResult
        {
            public int ExitCode;
            public string Stdout;
            public string Stderr;
        }

        private sealed class ShortcutOwnership
        {
            public string OfficialName;
            public string WrapperName;
        }

        private sealed class ShortcutInstallReceipt
        {
            public string OfficialPath;
            public string WrapperPath;
            public bool OfficialCreated;
            public bool WrapperCreated;
            public byte[] PreviousOfficialBytes;
            public byte[] PreviousWrapperBytes;
            public byte[] CommittedOfficialBytes;
            public byte[] CommittedWrapperBytes;
            public bool MarkerExisted;
            public string PreviousMarker;
            public string SupersededOfficialPath;
            public byte[] PreviousSupersededOfficialBytes;
            public bool SupersededOfficialDeleted;
            public string SupersededWrapperPath;
            public byte[] PreviousSupersededWrapperBytes;
            public bool SupersededWrapperDeleted;
        }

        private sealed class SetupSelfUpdatePendingException : IOException
        {
            public SetupSelfUpdatePendingException(string message) : base(message) { }
        }

        private static TestConfig _testConfig;
        private static TestConfig GetTestConfig()
        {
            if (_testConfig != null) return _testConfig;
            string exeDir = Path.GetDirectoryName(
                System.Reflection.Assembly.GetExecutingAssembly().Location) ?? ".";
            string cfg = Path.Combine(exeDir, "test-config.json");
            bool explicitTesting = Environment.GetEnvironmentVariable("CODEX_NTFS_FIX_SETUP_TESTING") == "1";
            if ((_cliMode || explicitTesting) && File.Exists(cfg))
            {
                _testConfig = NewtonsoftSafe(File.ReadAllText(cfg));
                _testConfig.enabled = true;
                ValidateTestConfig(_testConfig, "test-config.json");
                return _testConfig;
            }
            if (_cliMode)
                throw new InvalidOperationException("--test-install requires an isolated test-config.json sidecar.");
            if (explicitTesting)
            {
                _testConfig = new TestConfig
                {
                    install_root = Environment.GetEnvironmentVariable("TEST_INSTALL_ROOT"),
                    shell_root = Environment.GetEnvironmentVariable("TEST_SHELL_ROOT"),
                    uninstall_subkey = Environment.GetEnvironmentVariable("TEST_UNINSTALL_SUBKEY"),
                    package_root = Environment.GetEnvironmentVariable("TEST_PACKAGE_ROOT"),
                    enabled = true
                };
                ValidateTestConfig(_testConfig, "test environment");
                return _testConfig;
            }
            _testConfig = new TestConfig(); // all null => disabled
            return _testConfig;
        }

        private static void ValidateTestConfig(TestConfig cfg, string source)
        {
            if (cfg == null || string.IsNullOrEmpty(cfg.install_root)
                || string.IsNullOrEmpty(cfg.shell_root)
                || string.IsNullOrEmpty(cfg.uninstall_subkey))
                throw new InvalidOperationException(source + " must provide install_root, shell_root, and uninstall_subkey.");
            if (!Path.IsPathRooted(cfg.install_root) || !Path.IsPathRooted(cfg.shell_root))
                throw new InvalidOperationException(source + " install_root and shell_root must be absolute paths.");
            string install = CanonicalFullPath(cfg.install_root);
            string shell = CanonicalFullPath(cfg.shell_root);
            EnsureNoReparseComponents(install);
            EnsureNoReparseComponents(shell);
            if (string.Equals(install, shell, StringComparison.OrdinalIgnoreCase)
                || IsSubPath(install, shell) || IsSubPath(shell, install))
                throw new InvalidOperationException(source + " install_root and shell_root must be isolated paths.");

            string productionInstall = CanonicalFullPath(Environment.ExpandEnvironmentVariables(LocalAppData));
            string productionPrograms = CanonicalFullPath(
                Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.StartMenu), "Programs"));
            if (IsSameOrSubPath(productionInstall, install) || IsSameOrSubPath(productionPrograms, install)
                || IsSameOrSubPath(productionInstall, shell) || IsSameOrSubPath(productionPrograms, shell))
                throw new InvalidOperationException(source +
                    " may not target the production install root or the real Start Menu Programs tree.");
            if (!IsAllowedTestRoot(install) || !IsAllowedTestRoot(shell))
                throw new InvalidOperationException(source +
                    " roots must be under TEMP or inside a directory whose name starts with ChatGPT-Fix-Test.");

            if (!string.IsNullOrEmpty(cfg.package_root))
            {
                if (!Path.IsPathRooted(cfg.package_root))
                    throw new InvalidOperationException(source + " package_root must be an absolute path.");
                string package = CanonicalFullPath(cfg.package_root);
                EnsureNoReparseComponents(package);
                if (!IsAllowedTestRoot(package))
                    throw new InvalidOperationException(source +
                        " package_root must be under TEMP or inside a directory whose name starts with ChatGPT-Fix-Test.");
                if (IsSameOrSubPath(install, package) || IsSameOrSubPath(package, install)
                    || IsSameOrSubPath(shell, package) || IsSameOrSubPath(package, shell))
                    throw new InvalidOperationException(source +
                        " package_root must be isolated from install_root and shell_root.");
                if (!File.Exists(Path.Combine(package, "app", "ChatGPT.exe"))
                    || !File.Exists(Path.Combine(package, "app", "resources", "app.asar")))
                    throw new InvalidOperationException(source +
                        " package_root must contain app/ChatGPT.exe and app/resources/app.asar.");
                cfg.package_root = package;
            }
            else if (_cliMode)
            {
                throw new InvalidOperationException("--test-install requires package_root in test-config.json.");
            }

            cfg.install_root = install;
            cfg.shell_root = shell;
            const string productionSubkey = @"Software\Microsoft\Windows\CurrentVersion\Uninstall\ChatGPT-Fix";
            if (string.Equals(cfg.uninstall_subkey, productionSubkey, StringComparison.OrdinalIgnoreCase)
                || !cfg.uninstall_subkey.StartsWith(@"Software\Microsoft\Windows\CurrentVersion\Uninstall\", StringComparison.OrdinalIgnoreCase))
                throw new InvalidOperationException(source + " uninstall_subkey must be a non-production HKCU uninstall subkey.");
        }

        private static bool IsSubPath(string parent, string child)
        {
            string prefix = parent.TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar) + Path.DirectorySeparatorChar;
            return child.StartsWith(prefix, StringComparison.OrdinalIgnoreCase);
        }

        private static bool IsSameOrSubPath(string parent, string child)
        {
            return string.Equals(parent, child, StringComparison.OrdinalIgnoreCase) || IsSubPath(parent, child);
        }

        private static string CanonicalFullPath(string path)
        {
            return Path.GetFullPath(path).TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar);
        }

        // Reparse points can redirect a lexical TEMP/test path into the
        // installed product or WindowsApps. Check every existing component;
        // missing leaf components are safe to create after their parents pass.
        private static void EnsureNoReparseComponents(string path)
        {
            if (string.IsNullOrEmpty(path)) throw new ArgumentException("Path is required.");
            string full = Path.GetFullPath(path);
            string root = Path.GetPathRoot(full);
            if (string.IsNullOrEmpty(root)) throw new ArgumentException("Path must be rooted: " + path);
            string current = root;
            string remainder = full.Substring(root.Length);
            string[] components = remainder.Split(new[] { Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar },
                StringSplitOptions.RemoveEmptyEntries);
            foreach (string component in components)
            {
                current = string.IsNullOrEmpty(current) ? component : Path.Combine(current, component);
                try
                {
                    if ((File.GetAttributes(current) & FileAttributes.ReparsePoint) != 0)
                        throw new IOException("拒绝使用包含 reparse point 的路径组件：" + current);
                }
                catch (FileNotFoundException) { break; }
                catch (DirectoryNotFoundException) { break; }
                catch (UnauthorizedAccessException ex)
                {
                    throw new IOException("无法验证路径组件是否为 reparse point：" + current, ex);
                }
            }
        }

        private static bool IsAllowedTestRoot(string path)
        {
            string temp = CanonicalFullPath(Path.GetTempPath());
            if (IsSameOrSubPath(temp, path)) return true;
            string[] components = path.Split(new[] { Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar },
                StringSplitOptions.RemoveEmptyEntries);
            foreach (string component in components)
                if (component.StartsWith("ChatGPT-Fix-Test", StringComparison.OrdinalIgnoreCase)) return true;
            return false;
        }

        private static TestConfig NewtonsoftSafe(string json)
        {
            // Minimal JSON parse (no external dependency): extract the three
            // string values by key. Keys must be the exact names above.
            var cfg = new TestConfig();
            cfg.install_root = ExtractJsonString(json, "install_root");
            cfg.shell_root = ExtractJsonString(json, "shell_root");
            cfg.uninstall_subkey = ExtractJsonString(json, "uninstall_subkey");
            cfg.package_root = ExtractJsonString(json, "package_root");
            cfg.skip_launch = ExtractJsonString(json, "skip_launch");
            return cfg;
        }

        private static string ExtractJsonString(string json, string key)
        {
            if (json == null) return null;
            string marker = "\"" + key + "\"";
            int i = json.IndexOf(marker, StringComparison.Ordinal);
            if (i < 0) return null;
            int colon = json.IndexOf(':', i + marker.Length);
            if (colon < 0) return null;
            int position = colon + 1;
            while (position < json.Length && char.IsWhiteSpace(json[position])) position++;
            if (position >= json.Length || json[position] != '"') return null;
            position++;
            var value = new StringBuilder();
            while (position < json.Length)
            {
                char current = json[position++];
                if (current == '"') return value.ToString();
                if (current != '\\')
                {
                    value.Append(current);
                    continue;
                }
                if (position >= json.Length) return null;
                char escape = json[position++];
                switch (escape)
                {
                    case '"': value.Append('"'); break;
                    case '\\': value.Append('\\'); break;
                    case '/': value.Append('/'); break;
                    case 'b': value.Append('\b'); break;
                    case 'f': value.Append('\f'); break;
                    case 'n': value.Append('\n'); break;
                    case 'r': value.Append('\r'); break;
                    case 't': value.Append('\t'); break;
                    case 'u':
                        if (position + 4 > json.Length) return null;
                        int code = 0;
                        for (int digit = 0; digit < 4; digit++)
                        {
                            int hex = HexDigit(json[position++]);
                            if (hex < 0) return null;
                            code = (code << 4) | hex;
                        }
                        value.Append((char)code);
                        break;
                    default: return null;
                }
            }
            return null;
        }

        private static int HexDigit(char value)
        {
            if (value >= '0' && value <= '9') return value - '0';
            if (value >= 'a' && value <= 'f') return value - 'a' + 10;
            if (value >= 'A' && value <= 'F') return value - 'A' + 10;
            return -1;
        }

        private static string GetInstallRoot()
        {
            var cfg = GetTestConfig();
            if (!string.IsNullOrEmpty(cfg.install_root)) return cfg.install_root;
            return Environment.ExpandEnvironmentVariables(LocalAppData);
        }

        private static string GetShellProgramsDir()
        {
            var cfg = GetTestConfig();
            if (!string.IsNullOrEmpty(cfg.shell_root)) return cfg.shell_root;
            return Environment.GetFolderPath(Environment.SpecialFolder.StartMenu) + "\\Programs";
        }

        private static string GetUninstallSubkey()
        {
            var cfg = GetTestConfig();
            if (!string.IsNullOrEmpty(cfg.uninstall_subkey)) return cfg.uninstall_subkey;
            return @"Software\Microsoft\Windows\CurrentVersion\Uninstall\ChatGPT-Fix";
        }

        public SetupForm()
        {
            Text = "ChatGPT-Fix 安装程序 v1.0.5";
            ClientSize = new Size(500, 250);
            StartPosition = FormStartPosition.CenterScreen;
            FormBorderStyle = FormBorderStyle.FixedDialog;
            MaximizeBox = false;

            string[] stepTexts = {
                "1. 校验官方 ChatGPT 包",
                "2. 复制到用户目录（修复 NTFS 泄漏）",
                "3. 创建快捷方式与配置",
                "4. 启动 ChatGPT",
            };
            for (int i = 0; i < 4; i++)
            {
                _steps[i] = new Label { Text = "○ " + stepTexts[i], Left = 20, Top = 34 + i * 24, Width = 420 };
                Controls.Add(_steps[i]);
            }

            _status = new Label { Text = "准备就绪，点击“开始安装”。", Left = 20, Top = 136, Width = 420 };
            Controls.Add(_status);

            _progress = new ProgressBar { Left = 20, Top = 160, Width = 420, Height = 20 };
            Controls.Add(_progress);

            _install = new Button { Text = "开始安装", Left = 180, Top = 200, Width = 90, Height = 30 };
            _install.Click += (s, e) => StartInstall();
            Controls.Add(_install);

            _uninstall = new Button { Text = "卸载", Left = 280, Top = 200, Width = 90, Height = 30 };
            _uninstall.Click += (s, e) => StartUninstall();
            Controls.Add(_uninstall);

            _cancel = new Button { Text = "取消", Left = 380, Top = 200, Width = 90, Height = 30 };
            _cancel.Click += (s, e) => Close();
            Controls.Add(_cancel);
        }

        private void SetStep(int index)
        {
            if (index < _steps.Length && _steps[index] != null)
                _steps[index].Text = "● " + _steps[index].Text.Substring(2);
        }

        private void StartInstall()
        {
            if (_running) return;
            _running = true;
            _install.Enabled = false;
            _install.Text = "安装中…";
            SetStep(0);
            _status.Text = "正在校验官方 ChatGPT 包…";
            new System.Threading.Thread(() =>
            {
                var result = DoInstall();
                BeginInvoke((Action)(() => OnFinished(result)));
            }).Start();
        }

        internal bool DoInstall()
        {
            LastError = null;
            _ntcWarning = false;
            try
            {
                return DoInstallInner();
            }
            catch (Exception ex)
            {
                LastError = ex.ToString();
                SetStatus("安装失败：" + ex.Message);
                return false;
            }
        }

        // Last error detail (used by --test-install to write test-install.log).
        internal string LastError { get; private set; }
        internal bool HasWarnings { get { return _ntcWarning; } }

        private bool DoInstallInner()
        {
            string installRoot = null;
            ShortcutInstallReceipt shortcutReceipt = null;
            PayloadDeploymentReceipt payloadReceipt = null;
            bool statePublished = false;
            try
            {
                // 1. Discover the official package.
                var appRoot = DiscoverPackage();
                if (appRoot == null) { SetStatus("未检测到官方 OpenAI.Codex 包。"); return false; }
                string exe = Path.Combine(appRoot, "app", "ChatGPT.exe");
                if (!File.Exists(exe)) { SetStatus("官方包 app\\ChatGPT.exe 不存在。"); return false; }

                // 2. Copy to the user-owned baseline with progress. State is
                // not published until the installer payload is fully verified.
                installRoot = GetInstallRoot();
                EnsureNoReparseComponents(installRoot);
                string pkgName = new DirectoryInfo(appRoot).Name;
                string baseline = Path.Combine(installRoot, "baselines", pkgName);
                string appDst = Path.Combine(baseline, "app");
                EnsureNoReparseComponents(baseline);
                EnsureNoReparseComponents(appDst);
                SetStatus("正在校准官方包到用户目录（约 1-2 分钟）…");
                long total = TotalBytes(Path.Combine(appRoot, "app"));
                CopyTree(Path.Combine(appRoot, "app"), appDst, total);
                ReconcileNtcAfterBaselineCopy(
                    installRoot,
                    Path.Combine(baseline, "app", "resources"),
                    Path.Combine(appRoot, "app", "resources"));
                SetStep(1);
                _progress.Value = 92;

                // 3. Stage and verify every installer-owned payload before
                // replacing bin/scripts. A failed copy cannot publish state.
                SetStatus("正在暂存并校验程序文件…");
                string exeDir = Path.GetDirectoryName(
                    System.Reflection.Assembly.GetExecutingAssembly().Location) ?? ".";
                string binDir2 = Path.Combine(installRoot, "bin");
                payloadReceipt = DeployPayloadTransactional(installRoot, exeDir);
                string resources = Path.Combine(baseline, "app", "resources");
                EnsureNoReparseComponents(resources);
                Directory.CreateDirectory(resources);
                EnsureNoReparseComponents(resources);
                WriteTextAtomic(Path.Combine(resources, "chatgpt-fix.fixture"), "launcher-owned-v1");

                // 4. Shortcuts are deployable payload too: validate them before
                // current.json/state.json and the uninstall entry are written.
                SetStatus("正在创建并校验快捷方式…");
                shortcutReceipt = CreateShortcuts(installRoot);

                // 5. Only a fully validated payload is allowed to become the
                // current baseline and a registered installation.
                PublishInstallState(installRoot, baseline, pkgName, exe, appDst);
                statePublished = true;
                try { FinalizePayloadDeployment(payloadReceipt); }
                catch (Exception cleanupError)
                {
                    _ntcWarning = true;
                    WriteSetupLog(installRoot, "安装已提交，但旧 payload backup 清理失败：" + cleanupError.Message);
                }
                try
                {
                    RegisterUninstallEntry(installRoot);
                }
                catch (Exception registryError)
                {
                    // The durable install is committed. Keep it usable and
                    // make the registry failure explicit and retryable.
                    _ntcWarning = true;
                    WriteSetupLog(installRoot, "基础安装已提交，但卸载注册失败：" + registryError.Message);
                    SetStatus("基础安装已完成，但卸载注册失败；请运行 --repair 重试。" + registryError.Message);
                    if (_cliMode)
                        Console.WriteLine("INSTALL WARNING: uninstall registration failed; retry with --repair. " + registryError.Message);
                }
                SetStep(2);
                _progress.Value = 100;

                // 6. v1.0.5 keeps NTC injection disabled. The baseline copy
                // above reconciles stale NTC state, but a fresh injection can
                // reintroduce the known session-switch rendering stall.
                SetStatus("正在启动 ChatGPT…");
                string launcher = Path.Combine(binDir2, "ChatGPT-Fix-Launcher.exe");
                try
                {
                    if (!File.Exists(launcher)) throw new FileNotFoundException("部署后的 Launcher 不存在。", launcher);
                    if (!TestSkipLaunch())
                    {
                        Process launched = Process.Start(launcher);
                        if (launched == null) throw new InvalidOperationException("启动 ChatGPT 失败：Process.Start 返回空值。");
                    }
                }
                catch (Exception launchError)
                {
                    // current/state are already the durable commit. A launch
                    // failure is a retryable warning, not an install failure.
                    _ntcWarning = true;
                    string warning = "基础安装已提交，但 Launcher 启动失败：启动 ChatGPT 失败；请手动启动或重试。" + launchError.Message;
                    WriteSetupLog(installRoot, warning);
                    SetStatus(warning);
                    if (_cliMode) Console.WriteLine("INSTALL WARNING: ChatGPT launch failed; retry manually. " + launchError.Message);
                }
                SetStep(3);
                return true;
            }
            catch (Exception ex)
            {
                if (!statePublished)
                {
                    if (shortcutReceipt != null && !string.IsNullOrEmpty(installRoot))
                        RollbackShortcutInstall(installRoot, shortcutReceipt);
                    if (payloadReceipt != null)
                        RollbackPayloadDeployment(payloadReceipt);
                }
                // Re-throw so the OUTER DoInstall() records LastError and
                // returns false; keeping a local catch here would swallow
                // the detail (LastError stayed empty in --test-install).
                throw new IOException("安装失败：" + ex.Message, ex);
            }
        }

        private void StartUninstall()
        {
            if (_running) return;
            var confirm = MessageBox.Show(
                "将移除 ChatGPT-Fix 程序文件（bin、快捷方式、卸载项），\n但保留你的数据（登录信息、设置、备份、已修复的基线）。\n\n确定要卸载吗？",
                "ChatGPT-Fix 卸载（保留数据）",
                MessageBoxButtons.YesNo, MessageBoxIcon.Question);
            if (confirm != DialogResult.Yes) return;
            _running = true;
            _install.Enabled = false;
            _uninstall.Enabled = false;
            _uninstall.Text = "卸载中…";
            SetStatus("正在卸载（保留数据）…");
            new System.Threading.Thread(() =>
            {
                bool ok = DoUninstall();
                BeginInvoke((Action)(() => OnUninstallFinished(ok)));
            }).Start();
        }

        /// <summary>Repair mode: rebuild both start-menu shortcuts and the
        /// uninstall registry entry WITHOUT touching the baseline (no copy,
        /// no state.json rewrite). CLI: `ChatGPT-Fix-Setup.exe --repair`.</summary>
        public bool RepairShortcutAndRegistry()
        {
            string installRoot = null;
            ShortcutInstallReceipt shortcutReceipt = null;
            try
            {
                installRoot = GetInstallRoot();
                string launcher = Path.Combine(installRoot, "bin", "ChatGPT-Fix-Launcher.exe");
                string setup = Path.Combine(installRoot, "bin", "ChatGPT-Fix-Setup.exe");
                if (!File.Exists(launcher) || !File.Exists(setup))
                    throw new FileNotFoundException("修复需要完整的已安装 Launcher 和 Setup；请重新运行 v1.0.5 安装包。");
                shortcutReceipt = CreateShortcuts(installRoot);
                RegisterUninstallEntry(installRoot);
                return true;
            }
            catch (Exception ex)
            {
                if (shortcutReceipt != null && !string.IsNullOrEmpty(installRoot))
                    RollbackShortcutInstall(installRoot, shortcutReceipt);
                SetStatus("修复失败：" + ex.Message);
                return false;
            }
        }

        public bool DoUninstall()
        {
            try
            {
                string installRoot = GetInstallRoot();
                // 1. Validate shortcut ownership while the installed Launcher
                // still exists, then delete only marker-listed shortcuts whose
                // live COM properties still match this installation.
                if (!RemoveOwnedShortcuts(installRoot))
                    throw new IOException("一个已登记快捷方式发生所有权冲突或无法删除；卸载项已保留以便重试。");

                // 2. Remove the four EXEs from bin\ (Launcher/Manager/Packer/Setup).
                // A running installed Setup cannot delete itself. Every other
                // managed file must be gone before a post-exit helper is armed.
                string binDir = Path.Combine(installRoot, "bin");
                string selfToDelete = null;
                var cleanupErrors = new StringBuilder();
                if (Directory.Exists(binDir))
                {
                    string self = System.Reflection.Assembly.GetExecutingAssembly().Location;
                    foreach (string name in new[] { "ChatGPT-Fix-Launcher.exe", "ChatGPT-Fix-Manager.exe", "ChatGPT-Fix-Packer.exe", "ChatGPT-Fix-Setup.exe", "ChatGPT-Fix-Locale.exe" })
                    {
                        string p = Path.Combine(binDir, name);
                        try
                        {
                            if (!File.Exists(p)) continue;
                            // Compare normalized full paths: Assembly.Location
                            // may use a different casing/format than Path.Combine.
                            string pFull = Path.GetFullPath(p);
                            string selfFull = Path.GetFullPath(self);
                            if (string.Equals(pFull, selfFull, StringComparison.OrdinalIgnoreCase))
                            {
                                selfToDelete = pFull;
                                continue;
                            }
                            File.Delete(p);
                        }
                        catch (Exception ex)
                        {
                            cleanupErrors.Append(" [").Append(name).Append(": ")
                                .Append(ex.Message).Append("]");
                        }
                    }
                }
                string scriptsDir = Path.Combine(installRoot, "scripts");
                string managedScript = Path.Combine(scriptsDir, "inject-native-token-cost.js");
                try { if (File.Exists(managedScript)) File.Delete(managedScript); }
                catch (Exception ex)
                {
                    cleanupErrors.Append(" [scripts/inject-native-token-cost.js: ")
                        .Append(ex.Message).Append("]");
                }
                try
                {
                    if (Directory.Exists(scriptsDir)
                        && Directory.GetFiles(scriptsDir).Length == 0
                        && Directory.GetDirectories(scriptsDir).Length == 0)
                        Directory.Delete(scriptsDir);
                }
                catch { /* A non-empty user scripts directory is preserved. */ }

                if (cleanupErrors.Length > 0)
                    throw new IOException("受管文件未完全删除，卸载项已保留以便重试。" + cleanupErrors);

                // Only remove the registry entry once every directly deletable
                // managed file is gone. When Setup is the running installed
                // copy, the helper removes the registry entry only after its
                // bounded delete retry succeeds, so a failed cleanup remains
                // discoverable and retryable.
                if (selfToDelete != null)
                {
                    if (!ScheduleSelfCleanup(selfToDelete, binDir, GetUninstallSubkey()))
                        throw new IOException("无法启动 Setup 退出后清理器；卸载项已恢复以便重试。");
                    _selfCleanupScheduled = true;
                }
                else
                {
                    RemoveUninstallEntry();
                    try
                    {
                        if (Directory.Exists(binDir)
                            && Directory.GetFiles(binDir).Length == 0
                            && Directory.GetDirectories(binDir).Length == 0)
                            Directory.Delete(binDir);
                    }
                    catch { }
                }
                // 4. KEEP baselines\ (user data + profile), backups\, logs\.
                return true;
            }
            catch (Exception ex)
            {
                SetStatus("卸载失败：" + ex.Message);
                return false;
            }
        }

        private void OnUninstallFinished(bool ok)
        {
            _running = false;
            _uninstall.Enabled = true;
            if (ok)
            {
                if (_selfCleanupScheduled)
                {
                    _status.Text = "已卸载（数据已保留），正在完成 Setup 自清理。";
                    Close();
                    return;
                }
                _status.Text = "已卸载（数据已保留）。可随时重新安装。";
                _uninstall.Text = "关闭";
                _uninstall.Click -= StartUninstallClick;
                _uninstall.Click += (s, e) => Close();
            }
            else
            {
                _uninstall.Text = "重试";
            }
        }

        private void StartUninstallClick(object s, EventArgs e) { }

        private static void RegisterUninstallEntry(string installRoot)
        {
            string binSetup = Path.Combine(installRoot, "bin", "ChatGPT-Fix-Setup.exe");
            string uninstallCmd = "\"" + binSetup + "\" uninstall";
            using (RegistryKey key = Registry.CurrentUser.CreateSubKey(GetUninstallSubkey()))
            {
                if (key == null) throw new IOException("无法创建 HKCU 卸载注册表项。");
                key.SetValue("DisplayName", "ChatGPT-Fix");
                key.SetValue("DisplayVersion", "1.0.5");
                key.SetValue("InstallLocation", installRoot);
                key.SetValue("UninstallString", uninstallCmd);
                key.SetValue("QuietUninstallString", uninstallCmd);
                key.SetValue("NoModify", 1, RegistryValueKind.DWord);
                key.SetValue("NoRepair", 1, RegistryValueKind.DWord);
            }
        }

        private static void RemoveUninstallEntry()
        {
            Registry.CurrentUser.DeleteSubKeyTree(GetUninstallSubkey(), false);
        }

        private static bool ScheduleSelfCleanup(string setupPath, string binDir, string uninstallSubkey)
        {
            try
            {
                string marker = Path.Combine(binDir, ".uninstall-pending");
                EnsureNoReparseComponents(binDir);
                File.WriteAllText(marker, setupPath, new UTF8Encoding(false));
                int pid = Process.GetCurrentProcess().Id;
                string registryPath = @"Registry::HKEY_CURRENT_USER\" + uninstallSubkey;
                string script = "$ErrorActionPreference='SilentlyContinue';" +
                    "Wait-Process -Id " + pid + ";" +
                    "$s=" + PowerShellLiteral(setupPath) + ";" +
                    "for($i=0;$i -lt 100 -and (Test-Path -LiteralPath $s);$i++){" +
                    "Remove-Item -LiteralPath $s -Force;Start-Sleep -Milliseconds 100};" +
                    "if(Test-Path -LiteralPath $s){exit 1};" +
                    "Remove-Item -LiteralPath " + PowerShellLiteral(marker) + " -Force;" +
                    "$d=" + PowerShellLiteral(binDir) + ";" +
                    "if((Test-Path -LiteralPath $d) -and @((Get-ChildItem -LiteralPath $d -Force)).Count -eq 0){Remove-Item -LiteralPath $d -Force};" +
                    "Remove-Item -LiteralPath " + PowerShellLiteral(registryPath) + " -Recurse -Force";
                string encoded = Convert.ToBase64String(Encoding.Unicode.GetBytes(script));
                var info = new ProcessStartInfo(
                    FindWindowsPowerShell(),
                    "-NoProfile -NonInteractive -WindowStyle Hidden -EncodedCommand " + encoded)
                {
                    UseShellExecute = false,
                    CreateNoWindow = true
                };
                return Process.Start(info) != null;
            }
            catch { return false; }
        }

        private static string PowerShellLiteral(string value)
        {
            if (value == null || value.IndexOfAny(new[] { '\r', '\n', '\0' }) >= 0)
                throw new ArgumentException("Unsafe path for cleanup helper.");
            return "'" + value.Replace("'", "''") + "'";
        }

        private static PayloadDeploymentReceipt DeployPayloadTransactional(string installRoot, string sourceDirectory)
        {
            EnsureNoReparseComponents(installRoot);
            EnsureNoReparseComponents(sourceDirectory);
            Directory.CreateDirectory(installRoot);
            string scriptSource = Path.Combine(sourceDirectory, "scripts", "inject-native-token-cost.js");
            if (!File.Exists(scriptSource))
                scriptSource = Path.Combine(sourceDirectory, "inject-native-token-cost.js");
            EnsureNoReparseComponents(scriptSource);

            string[] executables = {
                "ChatGPT-Fix-Launcher.exe",
                "ChatGPT-Fix-Manager.exe",
                "ChatGPT-Fix-Packer.exe",
                "ChatGPT-Fix-Setup.exe",
                "ChatGPT-Fix-Locale.exe"
            };
            var entries = new PayloadEntry[6];
            for (int i = 0; i < executables.Length; i++)
            {
                entries[i] = new PayloadEntry
                {
                    Source = Path.Combine(sourceDirectory, executables[i]),
                    RelativeDestination = Path.Combine("bin", executables[i])
                };
            }
            entries[5] = new PayloadEntry
            {
                Source = scriptSource,
                RelativeDestination = Path.Combine("scripts", "inject-native-token-cost.js")
            };

            string token = Guid.NewGuid().ToString("N");
            string stageRoot = Path.Combine(installRoot, ".payload-stage-" + token);
            string backupRoot = Path.Combine(installRoot, ".payload-backup-" + token);
            var existed = new bool[entries.Length];
            int committed = 0;
            bool preserveBackup = false;
            try
            {
                for (int i = 0; i < entries.Length; i++)
                {
                    if (!File.Exists(entries[i].Source))
                        throw new FileNotFoundException("发布 payload 不完整：" + entries[i].RelativeDestination, entries[i].Source);
                    string staged = Path.Combine(stageRoot, entries[i].RelativeDestination);
                    EnsureNoReparseComponents(Path.GetDirectoryName(staged));
                    Directory.CreateDirectory(Path.GetDirectoryName(staged));
                    File.Copy(entries[i].Source, staged, true);
                    if (!File.Exists(staged) || !string.Equals(Sha256(entries[i].Source), Sha256(staged), StringComparison.OrdinalIgnoreCase))
                        throw new IOException("暂存 payload 校验失败：" + entries[i].RelativeDestination);
                }

                string installedSetup = Path.Combine(installRoot, "bin", "ChatGPT-Fix-Setup.exe");
                string runningSetup = System.Reflection.Assembly.GetExecutingAssembly().Location;
                if (PathsEqual(runningSetup, installedSetup))
                {
                    string stagedSetup = Path.Combine(stageRoot, "bin", "ChatGPT-Fix-Setup.exe");
                    string pendingSetup = installedSetup + ".pending-v1.0.2";
                    Directory.CreateDirectory(Path.GetDirectoryName(pendingSetup));
                    File.Copy(stagedSetup, pendingSetup, true);
                    if (!string.Equals(Sha256(stagedSetup), Sha256(pendingSetup), StringComparison.OrdinalIgnoreCase))
                        throw new IOException("Setup 自更新 pending 文件校验失败。");
                    string marker = Path.Combine(installRoot, "state", "setup-update-pending-v1.0.2.json");
                    WriteTextAtomic(marker,
                        "{\"schema\":\"chatgpt_fix.setup_update_pending.v1\",\"version\":\"1.0.2\"," +
                        "\"pending_path\":\"" + JsonEscape(pendingSetup.Replace('\\', '/')) + "\"," +
                        "\"sha256\":\"" + Sha256(pendingSetup) + "\",\"action\":\"rerun_external_installer\"," +
                        "\"created_at_utc\":\"" + DateTime.UtcNow.ToString("yyyy-MM-ddTHH:mm:ssZ") + "\"}\n");
                    WriteSetupLog(installRoot, "running installed Setup detected; staged " + pendingSetup +
                        "; payload left unchanged; rerun the external v1.0.2 installer");
                    throw new SetupSelfUpdatePendingException(
                        "检测到正在运行已安装的 Setup，未替换当前完整程序集合。新 Setup 已暂存为 " +
                        pendingSetup + "；请关闭本程序并从外部 v1.0.2 安装包重新运行，不能将本次视为安装成功。");
                }
                ClearExternalSetupUpdatePending(installRoot, installedSetup);

                for (int i = 0; i < entries.Length; i++)
                {
                    string destination = Path.Combine(installRoot, entries[i].RelativeDestination);
                    existed[i] = File.Exists(destination);
                    if (existed[i])
                    {
                        string backup = Path.Combine(backupRoot, entries[i].RelativeDestination);
                        EnsureNoReparseComponents(Path.GetDirectoryName(backup));
                        Directory.CreateDirectory(Path.GetDirectoryName(backup));
                        File.Copy(destination, backup, true);
                        if (!string.Equals(Sha256(destination), Sha256(backup), StringComparison.OrdinalIgnoreCase))
                            throw new IOException("备份既有 payload 失败：" + entries[i].RelativeDestination);
                    }
                }

                for (int i = 0; i < entries.Length; i++)
                {
                    string staged = Path.Combine(stageRoot, entries[i].RelativeDestination);
                    string destination = Path.Combine(installRoot, entries[i].RelativeDestination);
                    EnsureNoReparseComponents(Path.GetDirectoryName(destination));
                    Directory.CreateDirectory(Path.GetDirectoryName(destination));
                    string pending = destination + ".new-" + token;
                    File.Copy(staged, pending, true);
                    if (!string.Equals(Sha256(staged), Sha256(pending), StringComparison.OrdinalIgnoreCase))
                        throw new IOException("待提交 payload 校验失败：" + entries[i].RelativeDestination);
                    if (File.Exists(destination)) File.Replace(pending, destination, null, true);
                    else File.Move(pending, destination);
                    committed++;
                    if (!string.Equals(Sha256(staged), Sha256(destination), StringComparison.OrdinalIgnoreCase))
                        throw new IOException("已提交 payload 校验失败：" + entries[i].RelativeDestination);
                }
                preserveBackup = true;
                return new PayloadDeploymentReceipt
                {
                    InstallRoot = installRoot,
                    BackupRoot = backupRoot,
                    Entries = entries,
                    Existed = existed
                };
            }
            catch (SetupSelfUpdatePendingException)
            {
                throw;
            }
            catch (Exception error)
            {
                var rollbackErrors = new StringBuilder();
                for (int i = committed - 1; i >= 0; i--)
                {
                    string destination = Path.Combine(installRoot, entries[i].RelativeDestination);
                    try
                    {
                        if (existed[i])
                        {
                            File.Copy(Path.Combine(backupRoot, entries[i].RelativeDestination), destination, true);
                            if (!string.Equals(Sha256(Path.Combine(backupRoot, entries[i].RelativeDestination)),
                                Sha256(destination), StringComparison.OrdinalIgnoreCase))
                                throw new IOException("回滚后哈希不匹配。");
                        }
                        else if (File.Exists(destination)) File.Delete(destination);
                    }
                    catch (Exception rollbackError)
                    {
                        rollbackErrors.Append(" [").Append(entries[i].RelativeDestination).Append(": ")
                            .Append(rollbackError.Message).Append("]");
                    }
                }
                preserveBackup = rollbackErrors.Length > 0;
                string recovery = preserveBackup ? " 回滚不完整，备份保留于 " + backupRoot + "。" : " 旧完整集合已回滚。";
                WriteSetupLog(installRoot, "payload transaction failed: " + error.Message + recovery + rollbackErrors);
                throw new IOException("发布 payload 事务失败。请关闭 ChatGPT、Launcher、Manager、Packer 后重试。" +
                    recovery + rollbackErrors, error);
            }
            finally
            {
                for (int i = 0; i < entries.Length; i++)
                {
                    string pending = Path.Combine(installRoot, entries[i].RelativeDestination) + ".new-" + token;
                    try { if (File.Exists(pending)) File.Delete(pending); } catch { }
                }
                TryDeleteOwnedDirectory(installRoot, stageRoot, ".payload-stage-");
                if (!preserveBackup)
                    TryDeleteOwnedDirectory(installRoot, backupRoot, ".payload-backup-");
            }
        }

        private static void FinalizePayloadDeployment(PayloadDeploymentReceipt receipt)
        {
            if (receipt == null) return;
            TryDeleteOwnedDirectory(receipt.InstallRoot, receipt.BackupRoot, ".payload-backup-");
        }

        private static void RollbackPayloadDeployment(PayloadDeploymentReceipt receipt)
        {
            if (receipt == null) return;
            var errors = new StringBuilder();
            for (int i = receipt.Entries.Length - 1; i >= 0; i--)
            {
                string destination = Path.Combine(receipt.InstallRoot, receipt.Entries[i].RelativeDestination);
                string backup = Path.Combine(receipt.BackupRoot, receipt.Entries[i].RelativeDestination);
                try
                {
                    if (receipt.Existed[i])
                    {
                        if (!File.Exists(backup)) throw new IOException("payload backup is missing");
                        File.Copy(backup, destination, true);
                        if (!string.Equals(Sha256(backup), Sha256(destination), StringComparison.OrdinalIgnoreCase))
                            throw new IOException("restored payload hash mismatch");
                    }
                    else if (File.Exists(destination))
                    {
                        File.Delete(destination);
                    }
                }
                catch (Exception error)
                {
                    errors.Append(" [").Append(receipt.Entries[i].RelativeDestination)
                        .Append(": ").Append(error.Message).Append("]");
                }
            }
            if (errors.Length > 0)
            {
                WriteSetupLog(receipt.InstallRoot, "payload rollback failed:" + errors);
                throw new IOException("payload rollback failed:" + errors);
            }
            FinalizePayloadDeployment(receipt);
        }

        private static void ClearExternalSetupUpdatePending(string installRoot, string installedSetup)
        {
            string pending = installedSetup + ".pending-v1.0.2";
            string marker = Path.Combine(installRoot, "state", "setup-update-pending-v1.0.2.json");
            EnsureNoReparseComponents(Path.Combine(installRoot, "state"));
            EnsureNoReparseComponents(marker);
            EnsureNoReparseComponents(pending);
            if (File.Exists(marker))
            {
                string json = File.ReadAllText(marker);
                string recorded = ExtractJsonString(json, "pending_path");
                string expectedHash = ExtractJsonString(json, "sha256");
                if (!PathsEqual(recorded ?? "", pending) || !IsSha256(expectedHash))
                    throw new IOException("setup-update-pending marker 校验失败，拒绝清理。");
                if (!File.Exists(pending) || !string.Equals(Sha256(pending), expectedHash, StringComparison.OrdinalIgnoreCase))
                    throw new IOException("setup-update-pending 文件校验失败，拒绝清理。");
                File.Delete(pending);
                File.Delete(marker);
                WriteSetupLog(installRoot, "cleared verified setup-update-pending-v1.0.2 marker and pending file");
                return;
            }
            if (File.Exists(pending))
            {
                File.Delete(pending);
                WriteSetupLog(installRoot, "cleared orphan setup-update-pending-v1.0.2 file");
            }
        }

        private static void WriteSetupLog(string installRoot, string message)
        {
            try
            {
                string logs = Path.Combine(installRoot, "logs");
                Directory.CreateDirectory(logs);
                File.AppendAllText(Path.Combine(logs, "setup.log"),
                    DateTime.UtcNow.ToString("o") + " " + message + "\r\n", new UTF8Encoding(false));
            }
            catch { }
        }

        private static void TryDeleteOwnedDirectory(string installRoot, string path, string requiredPrefix)
        {
            try
            {
                string root = Path.GetFullPath(installRoot);
                string full = Path.GetFullPath(path);
                if (IsSubPath(root, full) && Path.GetFileName(full).StartsWith(requiredPrefix, StringComparison.Ordinal)
                    && Directory.Exists(full))
                    Directory.Delete(full, true);
            }
            catch { }
        }

        private static void PublishInstallState(string installRoot, string baseline, string packageName, string sourceExe, string appDirectory)
        {
            EnsureNoReparseComponents(installRoot);
            EnsureNoReparseComponents(baseline);
            EnsureNoReparseComponents(Path.Combine(baseline, "app"));
            EnsureNoReparseComponents(Path.Combine(baseline, "app", "resources"));
            EnsureNoReparseComponents(Path.Combine(installRoot, "state"));
            Directory.CreateDirectory(installRoot);
            Directory.CreateDirectory(baseline);
            var copied = DirStats(appDirectory);
            string normalizedBaseline = baseline.Replace('\\', '/');
            string stateJson =
                "{\"schema\":\"chatgpt_fix.staging.v1\",\"source_package_full_name\":\"" + JsonEscape(packageName) +
                "\",\"source_version\":\"\",\"source_hash_manifest\":[{\"relative_path\":\"ChatGPT.exe\",\"bytes\":" +
                new FileInfo(sourceExe).Length + ",\"sha256\":\"" + Sha256(sourceExe) +
                "\"}],\"staging_root\":\"" + JsonEscape(normalizedBaseline) +
                "\",\"baseline_root\":\"" + JsonEscape(normalizedBaseline) +
                "\",\"files_staged\":" + copied.Item1 + ",\"total_bytes\":" + copied.Item2 +
                ",\"state\":\"verified\",\"created_at_utc\":\"" + DateTime.UtcNow.ToString("yyyy-MM-ddTHH:mm:ssZ") + "\"}\n";
            string pointerJson = "{\"schema\":\"chatgpt_fix.pointer.v1\",\"baseline_root\":\"" +
                JsonEscape(normalizedBaseline) + "\"}\n";
            // state.json is prepared first. current.json is deliberately the
            // last write and the sole commit point observed by Launcher.
            WriteTextAtomic(Path.Combine(baseline, "state.json"), stateJson);
            WriteTextAtomic(Path.Combine(installRoot, "current.json"), pointerJson);
        }

        private static void WriteTextAtomic(string path, string content)
        {
            string parent = Path.GetDirectoryName(path);
            EnsureNoReparseComponents(parent);
            Directory.CreateDirectory(parent);
            EnsureNoReparseComponents(path);
            string temporary = path + ".tmp-" + Guid.NewGuid().ToString("N");
            try
            {
                File.WriteAllText(temporary, content, new UTF8Encoding(false));
                if (File.Exists(path)) File.Replace(temporary, path, null, true);
                else File.Move(temporary, path);
            }
            finally
            {
                try { if (File.Exists(temporary)) File.Delete(temporary); } catch { }
            }
        }

        private static string JsonEscape(string value)
        {
            return (value ?? "").Replace("\\", "\\\\").Replace("\"", "\\\"")
                .Replace("\r", "\\r").Replace("\n", "\\n");
        }

        private bool EnsureNtc(string installRoot, string baseline)
        {
            try { return EnsureNtcInner(installRoot, baseline); }
            catch (Exception ex)
            {
                return NtcFailure(installRoot, "Token 统计后处理异常：" + ex.Message, "", ex.ToString());
            }
        }

        // Runs Manager's bounded ntc-ensure operation and accepts success only
        // when its committed receipt matches both the backup and current asar.
        private bool EnsureNtcInner(string installRoot, string baseline)
        {
            string manager = Path.Combine(installRoot, "bin", "ChatGPT-Fix-Manager.exe");
            string resources = Path.Combine(baseline, "app", "resources");
            EnsureNoReparseComponents(installRoot);
            EnsureNoReparseComponents(baseline);
            EnsureNoReparseComponents(resources);
            if (!File.Exists(manager))
            {
                return NtcFailure(installRoot, "缺少 ChatGPT-Fix-Manager.exe。", "", "");
            }
            if (!Directory.Exists(resources))
            {
                return NtcFailure(installRoot, "baseline resources 目录不存在。", "", "");
            }
            var psi = new ProcessStartInfo(manager,
                "ntc-ensure --fixture-root \"" + resources + "\"")
            { CreateNoWindow = true, UseShellExecute = false, RedirectStandardOutput = true, RedirectStandardError = true };
            // Manager owns the Node process tree and enforces this timeout.
            psi.EnvironmentVariables["CHATGPT_FIX_NTC_TIMEOUT_SECS"] = "60";
            // Forward the module location explicitly (NODE_PATH).
            string nodePath = FindAsarNodePath();
            if (nodePath != null) psi.EnvironmentVariables["NODE_PATH"] = nodePath;
            using (var p = Process.Start(psi))
            {
                if (p == null)
                    return NtcFailure(installRoot, "无法启动 Manager。", "", "");

                string stdout = "";
                string stderr = "";
                Exception stdoutError = null;
                Exception stderrError = null;
                var stdoutReader = new System.Threading.Thread(() =>
                {
                    try { stdout = p.StandardOutput.ReadToEnd(); }
                    catch (Exception ex) { stdoutError = ex; }
                });
                var stderrReader = new System.Threading.Thread(() =>
                {
                    try { stderr = p.StandardError.ReadToEnd(); }
                    catch (Exception ex) { stderrError = ex; }
                });
                stdoutReader.IsBackground = true;
                stderrReader.IsBackground = true;
                stdoutReader.Start();
                stderrReader.Start();

                // Manager normally owns its Node tree, but Setup still enforces
                // an outer bound. A timed-out Manager is never left behind.
                if (!p.WaitForExit(90000))
                {
                    string killDetail = KillProcessTree(p.Id);
                    bool exited = false;
                    try { exited = p.WaitForExit(10000); } catch { }
                    bool readersJoined = stdoutReader.Join(5000) && stderrReader.Join(5000);
                    WriteNtcPendingMarker(installRoot, "manager_timeout", killDetail);
                    if (!exited || !readersJoined)
                        return NtcFailure(installRoot,
                            "Manager 超过 90 秒且进程树清理未能完整确认；已记录 pending，不能宣称 Token 统计成功。 " + killDetail,
                            stdout, stderr);
                    return NtcFailure(installRoot,
                        "Manager 超过 90 秒；已终止进程树并记录 pending，Token 统计未启用。 " + killDetail,
                        stdout, stderr);
                }
                if (!stdoutReader.Join(5000) || !stderrReader.Join(5000))
                    return NtcFailure(installRoot, "无法完整收集 Manager 输出。", stdout, stderr);
                if (stdoutError != null || stderrError != null)
                    return NtcFailure(installRoot, "读取 Manager 输出失败。", stdout, stderr);

                stdout = stdout.Trim();
                stderr = stderr.Trim();
                WriteNtcLog(installRoot, p.ExitCode, stdout, stderr);
                if (p.ExitCode != 0)
                    return NtcFailure(installRoot, "Manager 退出码 " + p.ExitCode + "。", stdout, stderr);
                if (!NtcCommitIsValid(resources))
                    return NtcFailure(installRoot, "ntc-commit.json 缺失、无效或哈希不匹配。", stdout, stderr);
            }
            try { File.Delete(Path.Combine(installRoot, "state", "ntc-pending.json")); } catch { }
            return true;
        }

        private static string KillProcessTree(int pid)
        {
            try
            {
                var psi = new ProcessStartInfo("taskkill.exe", "/PID " + pid + " /T /F")
                {
                    CreateNoWindow = true,
                    UseShellExecute = false,
                    RedirectStandardOutput = true,
                    RedirectStandardError = true
                };
                using (var killer = Process.Start(psi))
                {
                    if (killer == null) return "taskkill 未能启动。";
                    if (!killer.WaitForExit(15000))
                    {
                        try { killer.Kill(); } catch { }
                        return "taskkill 超过 15 秒未退出。";
                    }
                    string stdout = killer.StandardOutput.ReadToEnd().Trim();
                    string stderr = killer.StandardError.ReadToEnd().Trim();
                    return "taskkill exit=" + killer.ExitCode +
                        (string.IsNullOrEmpty(stderr) ? "" : " stderr=" + stderr) +
                        (string.IsNullOrEmpty(stdout) ? "" : " stdout=" + stdout);
                }
            }
            catch (Exception ex) { return "taskkill 异常：" + ex.Message; }
        }

        private static void WriteNtcPendingMarker(string installRoot, string reason, string detail)
        {
            try
            {
                WriteTextAtomic(Path.Combine(installRoot, "state", "ntc-pending.json"),
                    "{\"schema\":\"chatgpt_fix.ntc_pending.v1\",\"reason\":\"" + JsonEscape(reason) +
                    "\",\"detail\":\"" + JsonEscape(detail) + "\",\"created_at_utc\":\"" +
                    DateTime.UtcNow.ToString("yyyy-MM-ddTHH:mm:ssZ") + "\"}\n");
            }
            catch { }
        }

        private bool NtcFailure(string installRoot, string message, string stdout, string stderr)
        {
            string detail = message + NtcDiagnostic(stdout, stderr);
            LastError = detail;
            _ntcWarning = true;
            WriteNtcLog(installRoot, -1, stdout, stderr + (string.IsNullOrEmpty(stderr) ? "" : "\r\n") + message);
            SetStatus("Token 统计未启用：" + detail + " 请从外部 v1.0.5 安装包重新安装后重试。");
            if (_cliMode) Console.WriteLine("NTC WARNING: Token statistics are not enabled. Retry with the external v1.0.5 installer. " + detail);
            return false;
        }

        private static void WriteNtcLog(string installRoot, int exitCode, string stdout, string stderr)
        {
            try
            {
                string logs = Path.Combine(installRoot, "logs");
                Directory.CreateDirectory(logs);
                File.AppendAllText(Path.Combine(logs, "setup-ntc.log"),
                    DateTime.UtcNow.ToString("o") + " exit=" + exitCode + "\r\nstdout:\r\n" +
                    (stdout ?? "") + "\r\nstderr:\r\n" + (stderr ?? "") + "\r\n---\r\n",
                    new UTF8Encoding(false));
            }
            catch { }
        }

        private static bool NtcCommitIsValid(string resources)
        {
            try
            {
                string receipt = File.ReadAllText(Path.Combine(resources, "ntc-commit.json"));
                string schema = ExtractJsonString(receipt, "schema");
                string artifact = ExtractJsonString(receipt, "artifact");
                string before = ExtractJsonString(receipt, "before_sha256");
                string after = ExtractJsonString(receipt, "after_sha256");
                if (!string.Equals(schema, "chatgpt_fix.ntc_commit.v1", StringComparison.Ordinal)
                    || !string.Equals(artifact, "app.asar", StringComparison.Ordinal)
                    || !IsSha256(before) || !IsSha256(after)) return false;
                return string.Equals(Sha256(Path.Combine(resources, "app.asar.pre-ntc")), before, StringComparison.OrdinalIgnoreCase)
                    && string.Equals(Sha256(Path.Combine(resources, "app.asar")), after, StringComparison.OrdinalIgnoreCase);
            }
            catch { return false; }
        }

        // A full baseline reconciliation can legitimately replace an injected
        // app.asar with the current official source. Retain the old NTC
        // artifacts for recovery, but remove their receipt only after the
        // copied app.asar is byte-identical to the source; the next EnsureNtc
        // then creates a fresh backup/receipt for this source generation.
        private static void ReconcileNtcAfterBaselineCopy(string installRoot, string resources, string sourceResources)
        {
            string marker = Path.Combine(resources, "ntc-commit.json");
            string backup = Path.Combine(resources, "app.asar.pre-ntc");
            if (!File.Exists(marker) && !File.Exists(backup)) return;
            if (!File.Exists(marker) || !File.Exists(backup))
                throw new IOException("NTC 事务状态不完整，拒绝覆盖旧恢复文件。");

            string receipt = File.ReadAllText(marker);
            string before = ExtractJsonString(receipt, "before_sha256");
            string after = ExtractJsonString(receipt, "after_sha256");
            if (!IsSha256(before) || !IsSha256(after)
                || !string.Equals(Sha256(backup), before, StringComparison.OrdinalIgnoreCase))
                throw new IOException("NTC 旧事务 receipt 或 backup 校验失败。");

            string sourceAsar = Path.Combine(sourceResources, "app.asar");
            string currentAsar = Path.Combine(resources, "app.asar");
            if (!File.Exists(sourceAsar) || !File.Exists(currentAsar)
                || !string.Equals(Sha256(currentAsar), Sha256(sourceAsar), StringComparison.OrdinalIgnoreCase))
                throw new IOException("baseline copy 后 app.asar 与官方 source 不一致，拒绝清理旧 NTC 事务。");

            string stale = backup + ".stale-" + DateTime.UtcNow.ToString("yyyyMMddHHmmssfff");
            EnsureNoReparseComponents(stale);
            File.Move(backup, stale);
            try { File.Delete(marker); }
            catch
            {
                if (File.Exists(stale) && !File.Exists(backup)) File.Move(stale, backup);
                throw;
            }
            WriteSetupLog(installRoot,
                "reconciled stale NTC transaction after official baseline copy; preserved " + stale);
        }

        private static bool IsSha256(string value)
        {
            if (value == null || value.Length != 64) return false;
            foreach (char c in value)
                if (!((c >= '0' && c <= '9') || (c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F'))) return false;
            return true;
        }

        private static string NtcDiagnostic(string stdout, string stderr)
        {
            string detail = !string.IsNullOrEmpty(stderr) ? stderr : stdout;
            if (string.IsNullOrEmpty(detail)) return " 请检查 Manager、Node 和 @electron/asar 的安装日志。";
            detail = detail.Replace("\r", "  ").Replace("\n", "  ");
            if (detail.Length > 360) detail = detail.Substring(0, 360) + "…";
            return " 诊断：" + detail;
        }

        // Test hook: when test-config.json sets "skip_launch": "1", the
        // final Launcher start is skipped (see TestConfig). Production has
        // no test-config.json, so this is always false there.
        private static bool TestSkipLaunch()
        {
            var cfg = GetTestConfig();
            if (cfg == null) return false;
            return string.Equals(cfg.skip_launch, "1", StringComparison.OrdinalIgnoreCase)
                || string.Equals(cfg.skip_launch, "true", StringComparison.OrdinalIgnoreCase);
        }

        // Locates a node_modules dir containing @electron\asar (needed by
        // the token-cost injector). Candidates, in order: an inherited
        // NODE_PATH, the Setup's own ntc\node_modules (shipped next to the
        // exe), the per-user npx cache, the per-user global npm root.
        // Returns null when not found — injection then degrades to a
        // no-op and the plugin is simply not installed (never a crash).
        private static string FindAsarNodePath()
        {
            string existing = Environment.GetEnvironmentVariable("NODE_PATH");
            if (!string.IsNullOrEmpty(existing)
                && Directory.Exists(Path.Combine(existing, "@electron", "asar")))
                return existing;
            string exeDir = Path.GetDirectoryName(
                System.Reflection.Assembly.GetExecutingAssembly().Location) ?? ".";
            string local = Path.Combine(exeDir, "ntc", "node_modules");
            if (Directory.Exists(Path.Combine(local, "@electron", "asar"))) return local;
            string localApp = Environment.GetEnvironmentVariable("LOCALAPPDATA");
            if (localApp != null)
            {
                string npxRoot = Path.Combine(localApp, "npm-cache", "_npx");
                if (Directory.Exists(npxRoot))
                {
                    try
                    {
                        foreach (string dir in Directory.GetDirectories(npxRoot))
                        {
                            string cand = Path.Combine(dir, "node_modules");
                            if (Directory.Exists(Path.Combine(cand, "@electron", "asar")))
                                return cand;
                        }
                    }
                    catch { }
                }
                string npmGlobal = Path.Combine(localApp, "npm", "node_modules");
                if (Directory.Exists(Path.Combine(npmGlobal, "@electron", "asar")))
                    return npmGlobal;
            }
            return null;
        }

        private void OnFinished(bool ok)
        {
            _running = false;
            _install.Enabled = true;
            if (ok)
            {
                _status.Text = _ntcWarning
                    ? "基础安装完成；Token 统计尚未启用，详见日志后重试。"
                    : "安装完成！ChatGPT 已启动。";
                _install.Text = "关闭";
                _install.Click -= StartInstallClick;
                _install.Click += (s, e) => Close();
            }
            else
            {
                _install.Text = "重试";
            }
        }

        private void StartInstallClick(object s, EventArgs e) { }

        private void SetStatus(string text)
        {
            if (_cliMode) return; // CLI mode: no window, status is a no-op.
            try
            {
                if (InvokeRequired) BeginInvoke((Action)(() => _status.Text = text));
                else _status.Text = text;
            }
            catch (InvalidOperationException)
            {
                // CLI mode (no window handle): status updates are a no-op.
            }
        }

        private static string FindWindowsPowerShell()
        {
            string systemRoot = Environment.GetEnvironmentVariable("SystemRoot") ?? @"C:\Windows";
            string path = Path.Combine(systemRoot, "System32", "WindowsPowerShell", "v1.0", "powershell.exe");
            if (!File.Exists(path))
                throw new FileNotFoundException("Windows PowerShell helper 不存在，拒绝使用 PATH fallback。", path);
            return path;
        }

        private static string DiscoverPackage()
        {
            var cfg = GetTestConfig();
            if (!string.IsNullOrEmpty(cfg.package_root)) return cfg.package_root;
            var psi = new ProcessStartInfo(FindWindowsPowerShell(),
                "-NoProfile -NonInteractive -Command \"(Get-AppxPackage -Name 'OpenAI.Codex' -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty InstallLocation)\"")
            { RedirectStandardOutput = true, UseShellExecute = false, CreateNoWindow = true };
            BoundedProcessResult result = RunProcessBounded(psi, 30000, "DiscoverPackage PowerShell");
            if (result.ExitCode != 0)
                throw new IOException("DiscoverPackage PowerShell 退出码 " + result.ExitCode + "。" +
                    (result.Stderr ?? ""));
            string line = (result.Stdout ?? "").Trim();
            return string.IsNullOrEmpty(line) ? null : line;
        }

        // ProcessStartInfo has no timeout primitive on .NET Framework. Keep
        // both output readers and process cleanup bounded so a child retaining
        // an inherited pipe cannot stall install indefinitely.
        private static BoundedProcessResult RunProcessBounded(
            ProcessStartInfo info, int timeoutMilliseconds, string description)
        {
            if (info == null) throw new ArgumentNullException("info");
            if (timeoutMilliseconds <= 0) throw new ArgumentOutOfRangeException("timeoutMilliseconds");
            info.RedirectStandardOutput = true;
            info.RedirectStandardError = true;
            info.UseShellExecute = false;
            info.CreateNoWindow = true;
            using (var process = Process.Start(info))
            {
                if (process == null) throw new IOException("无法启动 " + description + "。");
                string stdout = "";
                string stderr = "";
                Exception stdoutError = null;
                Exception stderrError = null;
                var stdoutReader = new System.Threading.Thread(() =>
                {
                    try { stdout = process.StandardOutput.ReadToEnd(); }
                    catch (Exception ex) { stdoutError = ex; }
                });
                var stderrReader = new System.Threading.Thread(() =>
                {
                    try { stderr = process.StandardError.ReadToEnd(); }
                    catch (Exception ex) { stderrError = ex; }
                });
                stdoutReader.IsBackground = true;
                stderrReader.IsBackground = true;
                stdoutReader.Start();
                stderrReader.Start();
                bool exited = process.WaitForExit(timeoutMilliseconds);
                if (!exited)
                {
                    try { process.Kill(); } catch { }
                    bool killed = false;
                    try { killed = process.WaitForExit(5000); } catch { }
                    bool readersJoined = stdoutReader.Join(1000) && stderrReader.Join(1000);
                    if (!killed || !readersJoined)
                        throw new IOException(description + " 超时且进程/输出清理未完成。");
                    throw new IOException(description + " 超时。");
                }
                if (!stdoutReader.Join(2000) || !stderrReader.Join(2000))
                    throw new IOException(description + " 输出收集超时。");
                if (stdoutError != null || stderrError != null)
                    throw new IOException(description + " 输出读取失败。");
                return new BoundedProcessResult
                {
                    ExitCode = process.ExitCode,
                    Stdout = stdout,
                    Stderr = stderr
                };
            }
        }

        // Returns (fileCount, totalBytes) for a directory tree. Enumeration
        // errors are fatal: publishing verified state with incomplete stats
        // would make a partial baseline appear healthy.
        private static Tuple<int, long> DirStats(string dir)
        {
            int count = 0;
            long bytes = 0;
            foreach (var f in Directory.GetFiles(LongPath(dir), "*", SearchOption.AllDirectories))
            {
                count++;
                bytes += FileLength(f);
            }
            return Tuple.Create(count, bytes);
        }

        private static long TotalBytes(string dir)
        {
            long sum = 0;
            foreach (var f in Directory.GetFiles(LongPath(dir), "*", SearchOption.AllDirectories))
                sum += FileLength(f);
            return sum;
        }

        private static long FileLength(string path)
        {
            try { return new FileInfo(path).Length; }
            catch (ArgumentException)
            {
                using (var stream = File.OpenRead(LongPath(path))) return stream.Length;
            }
            catch (NotSupportedException)
            {
                using (var stream = File.OpenRead(LongPath(path))) return stream.Length;
            }
        }

        // Prepends the Windows long-path prefix (\\?\) ONLY when the path is
        // near MAX_PATH (260), so the native CopyFile can handle deeply
        // nested directories (resources\cua_node\... in this package have
        // paths > 260 chars). Short paths are left untouched.
        private static string LongPath(string path)
        {
            if (path.Length < 250) return path;
            if (path.StartsWith(@"\\?\")) return path;
            if (path.Length >= 3 && path[1] == ':' && path[2] == '\\')
                return @"\\?\" + path;
            if (path.Length >= 2 && path.StartsWith(@"\\"))
                return @"\\?\UNC\" + path.Substring(2);
            return path;
        }

        // Native CopyFileW via P/Invoke — used only for single files (e.g.
        // bin\ deployment). Tree copies go through RobocopyTree (robocopy),
        // which is native, handles long paths, and copied the full 5421-file
        // package in 8 seconds with the current user's token.
        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern bool CopyFileW(string lpExistingFileName, string lpNewFileName, bool bFailIfExists);

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern bool CreateDirectoryW(string lpPathName, IntPtr lpSecurityAttributes);

        private static void NativeCopy(string src, string dst)
        {
            if (!CopyFileW(LongPath(src), LongPath(dst), false))
            {
                int err = Marshal.GetLastWin32Error();
                throw new IOException("CopyFileW 失败（错误 " + err + "）：" + src);
            }
        }

        // Tree copy via robocopy (Windows built-in). Rationale: robocopy is
        // native, handles long paths (>260) and ACL-restricted sources
        // (C:\Program Files\WindowsApps\) correctly, and copied this exact
        // 5421-file / 1.76 GB package in ~8 seconds with the current user's
        // token. Hand-rolled .NET recursion is either slow (per-file
        // P/Invoke) or breaks (FileIOPermission emulation, MAX_PATH,
        // stack overflow on deep node_modules trees). robocopy exit codes:
        // 0-7 are success (1=files copied, 2=extra files, 4=mismatch);
        // 8+ are failures. /R:0 /W:0 = no retries, /E = include subdirs,
        // /NFL /NDL /NJH /NJS /NP = quiet output.
        //
        // Arguments are passed directly to robocopy. QuoteWindowsArgument
        // implements the CommandLineToArgvW-compatible escaping needed by
        // .NET Framework's ProcessStartInfo.Arguments string.
        private void CopyTree(string src, string dst, long total)
        {
            string args = QuoteWindowsArgument(src) + " " + QuoteWindowsArgument(dst) +
                " /E /R:0 /W:0 /NFL /NDL /NJH /NJS /NP";
            var psi = new ProcessStartInfo("robocopy.exe", args)
            {
                CreateNoWindow = true,
                UseShellExecute = false,
                RedirectStandardOutput = true,
                RedirectStandardError = true,
                WorkingDirectory = Path.GetDirectoryName(dst)
            };
            BoundedProcessResult result = RunProcessBounded(psi, 600000, "robocopy");
            if (result.ExitCode >= 8)
                throw new IOException("robocopy 复制官方包失败（退出码 " + result.ExitCode + "）。" +
                    (result.Stdout ?? "") + (result.Stderr ?? ""));
        }

        private static string QuoteWindowsArgument(string value)
        {
            if (value == null || value.IndexOfAny(new[] { '\0', '\r', '\n' }) >= 0)
                throw new ArgumentException("Unsafe process argument.");
            var builder = new StringBuilder();
            builder.Append('"');
            int slashes = 0;
            foreach (char c in value)
            {
                if (c == '\\') { slashes++; continue; }
                if (c == '"') { builder.Append(new string('\\', slashes * 2 + 1)).Append('"'); slashes = 0; continue; }
                if (slashes > 0) { builder.Append(new string('\\', slashes)); slashes = 0; }
                builder.Append(c);
            }
            if (slashes > 0) builder.Append(new string('\\', slashes * 2));
            builder.Append('"');
            return builder.ToString();
        }

        private static string Sha256(string path)
        {
            using (var sha = SHA256.Create())
            using (var fs = File.OpenRead(LongPath(path)))
                return BitConverter.ToString(sha.ComputeHash(fs)).Replace("-", "").ToLowerInvariant();
        }

        private static ShortcutInstallReceipt CreateShortcuts(string installRoot)
        {
            string lnkDir = GetShellProgramsDir();
            EnsureNoReparseComponents(lnkDir);
            EnsureNoReparseComponents(installRoot);
            // WScript.Shell fails when the target directory is missing
            // (observed in the isolated fresh-env test), so create it first.
            Directory.CreateDirectory(lnkDir);
            // COM WScript.Shell via dynamic binding (no Interop reference needed).
            // Never overwrite an unrelated shortcut. When a generic name is
            // occupied, use a product-specific fallback and persist exactly
            // which two files this installer owns.
            const string aumid = @"shell:AppsFolder\OpenAI.Codex_2p2nqsd0c76g0!App";
            string windows = Environment.GetEnvironmentVariable("SystemRoot") ?? @"C:\Windows";
            string explorer = Path.Combine(windows, "explorer.exe");
            string launcher = Path.Combine(installRoot, "bin", "ChatGPT-Fix-Launcher.exe");
            if (!File.Exists(launcher))
                throw new FileNotFoundException("无法创建 wrapper 快捷方式：Launcher 不存在。", launcher);
            Exception lastError = null;
            for (int attempt = 0; attempt < 2; attempt++)
            {
                ShortcutInstallReceipt receipt = null;
                string officialTemp = null;
                string wrapperTemp = null;
                try
                {
                    ShortcutOwnership ownership = ReadShortcutOwnership(installRoot);
                    Func<string, bool> isOfficialOwnedOrLegacy = delegate(string path)
                    {
                        return IsOwnedOfficialShortcut(path)
                            || IsLegacyV101OfficialShortcut(path, installRoot)
                            || IsLegacyV102OfficialFallback(path);
                    };
                    string standardOfficialPath = Path.Combine(lnkDir, "ChatGPT.lnk");
                    string officialPath = File.Exists(standardOfficialPath)
                        && isOfficialOwnedOrLegacy(standardOfficialPath)
                        ? standardOfficialPath
                        : ReuseOwnedShortcutPath(
                            lnkDir,
                            ownership == null ? null : ownership.OfficialName,
                            isOfficialOwnedOrLegacy);
                    if (officialPath == null)
                    {
                        officialPath = SelectShortcutPath(
                            lnkDir,
                            "ChatGPT.lnk",
                            "ChatGPT (official).lnk",
                            isOfficialOwnedOrLegacy);
                    }
                    string supersededOfficialPath = null;
                    if (ownership != null
                        && string.Equals(ownership.OfficialName, "ChatGPT (official).lnk", StringComparison.OrdinalIgnoreCase)
                        && PathsEqual(officialPath, standardOfficialPath))
                    {
                        string recordedFallbackPath = Path.Combine(lnkDir, ownership.OfficialName);
                        if (File.Exists(recordedFallbackPath) && isOfficialOwnedOrLegacy(recordedFallbackPath))
                            supersededOfficialPath = recordedFallbackPath;
                    }

                    Func<string, bool> isWrapperOwnedOrLegacy = delegate(string path)
                    {
                        return IsOwnedWrapperShortcut(path, launcher)
                            || IsLegacyV101WrapperShortcut(path, launcher)
                            || IsLegacyV102WrapperFallback(path, launcher);
                    };
                    string standardWrapperPath = Path.Combine(lnkDir, "ChatGPT-Fix-Launcher.lnk");
                    string wrapperPath = File.Exists(standardWrapperPath)
                        && isWrapperOwnedOrLegacy(standardWrapperPath)
                        ? standardWrapperPath
                        : ReuseOwnedShortcutPath(
                            lnkDir,
                            ownership == null ? null : ownership.WrapperName,
                            isWrapperOwnedOrLegacy);
                    if (wrapperPath == null)
                    {
                        wrapperPath = SelectShortcutPath(
                            lnkDir,
                            "ChatGPT-Fix-Launcher.lnk",
                            "ChatGPT-Fix-Launcher (ChatGPT-Fix).lnk",
                            isWrapperOwnedOrLegacy);
                    }
                    string supersededWrapperPath = null;
                    if (ownership != null
                        && string.Equals(ownership.WrapperName, "ChatGPT-Fix-Launcher (ChatGPT-Fix).lnk", StringComparison.OrdinalIgnoreCase)
                        && PathsEqual(wrapperPath, standardWrapperPath))
                    {
                        string recordedFallbackPath = Path.Combine(lnkDir, ownership.WrapperName);
                        if (File.Exists(recordedFallbackPath) && isWrapperOwnedOrLegacy(recordedFallbackPath))
                            supersededWrapperPath = recordedFallbackPath;
                    }

                    string markerPath = ShortcutOwnershipPath(installRoot);
                    bool officialExists = File.Exists(officialPath);
                    bool wrapperExists = File.Exists(wrapperPath);
                    receipt = new ShortcutInstallReceipt
                    {
                        OfficialPath = officialPath,
                        WrapperPath = wrapperPath,
                        OfficialCreated = !officialExists,
                        WrapperCreated = !wrapperExists,
                        PreviousOfficialBytes = officialExists ? File.ReadAllBytes(officialPath) : null,
                        PreviousWrapperBytes = wrapperExists ? File.ReadAllBytes(wrapperPath) : null,
                        MarkerExisted = File.Exists(markerPath),
                        PreviousMarker = File.Exists(markerPath) ? File.ReadAllText(markerPath) : null
                    };

                    if (officialExists && !isOfficialOwnedOrLegacy(officialPath))
                        throw new IOException("官方快捷方式在写入前发生所有权冲突：" + officialPath);
                    if (wrapperExists && !isWrapperOwnedOrLegacy(wrapperPath))
                        throw new IOException("wrapper 快捷方式在写入前发生所有权冲突：" + wrapperPath);

                    Type wsType = Type.GetTypeFromProgID("WScript.Shell");
                    if (wsType == null) throw new InvalidOperationException("WScript.Shell COM 组件不可用。");
                    dynamic ws = Activator.CreateInstance(wsType);
                    officialTemp = Path.Combine(lnkDir, ".chatgpt-fix-official-" + Guid.NewGuid().ToString("N") + ".lnk");
                    wrapperTemp = Path.Combine(lnkDir, ".chatgpt-fix-wrapper-" + Guid.NewGuid().ToString("N") + ".lnk");
                    dynamic official = ws.CreateShortcut(officialTemp);
                    official.TargetPath = explorer;
                    official.Arguments = aumid;
                    official.WorkingDirectory = windows;
                    official.Description = "ChatGPT official entry (managed by ChatGPT-Fix v1.0.5)";
                    official.Save();
                    dynamic wrapper = ws.CreateShortcut(wrapperTemp);
                    wrapper.TargetPath = launcher;
                    wrapper.Arguments = "";
                    wrapper.WorkingDirectory = Path.GetDirectoryName(launcher);
                    wrapper.Description = "ChatGPT-Fix Launcher wrapper (managed by ChatGPT-Fix v1.0.5)";
                    wrapper.Save();
                    if (!IsOwnedOfficialShortcut(officialTemp)
                        || !IsOwnedWrapperShortcut(wrapperTemp, launcher))
                        throw new IOException("临时快捷方式属性验证失败。");

                    receipt.CommittedOfficialBytes = File.ReadAllBytes(officialTemp);
                    CommitShortcutFile(officialTemp, officialPath, receipt.PreviousOfficialBytes);
                    officialTemp = null;
                    receipt.CommittedWrapperBytes = File.ReadAllBytes(wrapperTemp);
                    CommitShortcutFile(wrapperTemp, wrapperPath, receipt.PreviousWrapperBytes);
                    wrapperTemp = null;
                    if (!IsOwnedOfficialShortcut(officialPath)
                        || !IsOwnedWrapperShortcut(wrapperPath, launcher))
                        throw new IOException("已提交的快捷方式属性验证失败。");
                    WriteShortcutOwnership(installRoot, officialPath, wrapperPath);
                    if (!string.IsNullOrEmpty(supersededOfficialPath))
                    {
                        receipt.SupersededOfficialPath = supersededOfficialPath;
                        receipt.PreviousSupersededOfficialBytes = File.ReadAllBytes(supersededOfficialPath);
                        if (!isOfficialOwnedOrLegacy(supersededOfficialPath)
                            || !FileBytesEqual(supersededOfficialPath, receipt.PreviousSupersededOfficialBytes))
                            throw new IOException("旧官方 fallback 快捷方式在删除前发生所有权冲突：" + supersededOfficialPath);
                        File.Delete(supersededOfficialPath);
                        receipt.SupersededOfficialDeleted = true;
                    }
                    if (!string.IsNullOrEmpty(supersededWrapperPath))
                    {
                        receipt.SupersededWrapperPath = supersededWrapperPath;
                        receipt.PreviousSupersededWrapperBytes = File.ReadAllBytes(supersededWrapperPath);
                        if (!isWrapperOwnedOrLegacy(supersededWrapperPath)
                            || !FileBytesEqual(supersededWrapperPath, receipt.PreviousSupersededWrapperBytes))
                            throw new IOException("旧 wrapper fallback 快捷方式在删除前发生所有权冲突：" + supersededWrapperPath);
                        File.Delete(supersededWrapperPath);
                        receipt.SupersededWrapperDeleted = true;
                    }
                    return receipt;
                }
                catch (Exception ex)
                {
                    lastError = ex;
                    if (receipt != null) RollbackShortcutInstall(installRoot, receipt);
                    if (attempt == 0) System.Threading.Thread.Sleep(500);
                }
                finally
                {
                    try { if (!string.IsNullOrEmpty(officialTemp) && File.Exists(officialTemp)) File.Delete(officialTemp); } catch { }
                    try { if (!string.IsNullOrEmpty(wrapperTemp) && File.Exists(wrapperTemp)) File.Delete(wrapperTemp); } catch { }
                }
            }
            throw new IOException("创建并验证开始菜单快捷方式失败（已重试一次）。", lastError);
        }

        private static void RollbackShortcutInstall(string installRoot, ShortcutInstallReceipt receipt)
        {
            if (receipt == null) return;
            try
            {
                bool unchanged = FileBytesEqual(receipt.OfficialPath, receipt.CommittedOfficialBytes);
                if (receipt.OfficialCreated && unchanged)
                    File.Delete(receipt.OfficialPath);
                else if (!receipt.OfficialCreated && receipt.PreviousOfficialBytes != null && unchanged)
                    WriteBytesAtomic(receipt.OfficialPath, receipt.PreviousOfficialBytes);
            }
            catch { }
            try
            {
                bool unchanged = FileBytesEqual(receipt.WrapperPath, receipt.CommittedWrapperBytes);
                if (receipt.WrapperCreated && unchanged)
                    File.Delete(receipt.WrapperPath);
                else if (!receipt.WrapperCreated && receipt.PreviousWrapperBytes != null && unchanged)
                    WriteBytesAtomic(receipt.WrapperPath, receipt.PreviousWrapperBytes);
            }
            catch { }
            try
            {
                if (receipt.SupersededWrapperDeleted
                    && receipt.PreviousSupersededWrapperBytes != null
                    && !File.Exists(receipt.SupersededWrapperPath))
                    WriteBytesAtomic(receipt.SupersededWrapperPath, receipt.PreviousSupersededWrapperBytes);
            }
            catch { }
            try
            {
                if (receipt.SupersededOfficialDeleted
                    && receipt.PreviousSupersededOfficialBytes != null
                    && !File.Exists(receipt.SupersededOfficialPath))
                    WriteBytesAtomic(receipt.SupersededOfficialPath, receipt.PreviousSupersededOfficialBytes);
            }
            catch { }
            try
            {
                string markerPath = ShortcutOwnershipPath(installRoot);
                if (receipt.MarkerExisted)
                    WriteTextAtomic(markerPath, receipt.PreviousMarker ?? "");
                else if (File.Exists(markerPath))
                    File.Delete(markerPath);
            }
            catch { }
        }

        private static void CommitShortcutFile(string temp, string destination, byte[] expectedExisting)
        {
            if (expectedExisting == null)
            {
                // File.Move is intentionally non-overwriting: if another
                // process claims the selected name, preserve it and fail.
                File.Move(temp, destination);
                return;
            }
            if (!FileBytesEqual(destination, expectedExisting))
                throw new IOException("快捷方式在提交前发生并发修改：" + destination);
            File.Replace(temp, destination, null, true);
        }

        private static void WriteBytesAtomic(string path, byte[] bytes)
        {
            string temp = path + ".tmp-" + Guid.NewGuid().ToString("N");
            File.WriteAllBytes(temp, bytes);
            try
            {
                if (File.Exists(path)) File.Replace(temp, path, null, true);
                else File.Move(temp, path);
            }
            finally
            {
                try { if (File.Exists(temp)) File.Delete(temp); } catch { }
            }
        }

        private static bool FileBytesEqual(string path, byte[] expected)
        {
            if (expected == null || !File.Exists(path)) return false;
            byte[] actual = File.ReadAllBytes(path);
            if (actual.Length != expected.Length) return false;
            for (int i = 0; i < actual.Length; i++)
                if (actual[i] != expected[i]) return false;
            return true;
        }

        private static string ReuseOwnedShortcutPath(
            string directory,
            string recordedName,
            Func<string, bool> isOwned)
        {
            if (string.IsNullOrEmpty(recordedName)) return null;
            string path = Path.Combine(directory, recordedName);
            return File.Exists(path) && isOwned(path) ? path : null;
        }

        private static string SelectShortcutPath(
            string directory,
            string preferredName,
            string fallbackName,
            Func<string, bool> isOwned)
        {
            string preferred = Path.Combine(directory, preferredName);
            if (!File.Exists(preferred) || isOwned(preferred)) return preferred;
            string fallback = Path.Combine(directory, fallbackName);
            if (!File.Exists(fallback) || isOwned(fallback)) return fallback;
            throw new IOException("开始菜单快捷方式名称均被其他程序占用：" + preferredName + " / " + fallbackName);
        }

        private static string ShortcutOwnershipPath(string installRoot)
        {
            return Path.Combine(installRoot, "state", "shortcut-ownership.json");
        }

        private static void WriteShortcutOwnership(string installRoot, string officialPath, string wrapperPath)
        {
            EnsureNoReparseComponents(installRoot);
            EnsureNoReparseComponents(Path.Combine(installRoot, "state"));
            string json = "{\"schema\":\"chatgpt_fix.shortcut_ownership.v1\",\"official_name\":\"" +
                JsonEscape(Path.GetFileName(officialPath)) + "\",\"wrapper_name\":\"" +
                JsonEscape(Path.GetFileName(wrapperPath)) + "\"}\n";
            WriteTextAtomic(ShortcutOwnershipPath(installRoot), json);
        }

        private static ShortcutOwnership ReadShortcutOwnership(string installRoot)
        {
            try
            {
                string json = File.ReadAllText(ShortcutOwnershipPath(installRoot));
                if (!string.Equals(ExtractJsonString(json, "schema"),
                    "chatgpt_fix.shortcut_ownership.v1", StringComparison.Ordinal)) return null;
                string official = ExtractJsonString(json, "official_name");
                string wrapper = ExtractJsonString(json, "wrapper_name");
                if (!IsAllowedShortcutName(official, true) || !IsAllowedShortcutName(wrapper, false)) return null;
                return new ShortcutOwnership { OfficialName = official, WrapperName = wrapper };
            }
            catch { return null; }
        }

        private static bool IsAllowedShortcutName(string name, bool official)
        {
            if (official)
                return string.Equals(name, "ChatGPT.lnk", StringComparison.OrdinalIgnoreCase)
                    || string.Equals(name, "ChatGPT (official).lnk", StringComparison.OrdinalIgnoreCase);
            return string.Equals(name, "ChatGPT-Fix-Launcher.lnk", StringComparison.OrdinalIgnoreCase)
                || string.Equals(name, "ChatGPT-Fix-Launcher (ChatGPT-Fix).lnk", StringComparison.OrdinalIgnoreCase);
        }

        private static bool RemoveOwnedShortcuts(string installRoot)
        {
            string directory = GetShellProgramsDir();
            string launcher = Path.Combine(installRoot, "bin", "ChatGPT-Fix-Launcher.exe");
            ShortcutOwnership ownership = ReadShortcutOwnership(installRoot);
            if (ownership != null)
            {
                bool officialRemoved = DeleteShortcutIfOwned(
                    Path.Combine(directory, ownership.OfficialName), true, launcher, false, true);
                bool wrapperRemoved = DeleteShortcutIfOwned(
                    Path.Combine(directory, ownership.WrapperName), false, launcher, false, true);
                if (officialRemoved && wrapperRemoved)
                {
                    try { File.Delete(ShortcutOwnershipPath(installRoot)); } catch { }
                }
                return officialRemoved && wrapperRemoved;
            }

            // Backward-compatible cleanup is still fail-closed: only the exact
            // product fields are accepted, never the filename alone.
            foreach (string name in new[] { "ChatGPT.lnk", "ChatGPT (official).lnk" })
                DeleteShortcutIfOwned(Path.Combine(directory, name), true, launcher, false, false);
            foreach (string name in new[] { "ChatGPT-Fix-Launcher.lnk", "ChatGPT-Fix-Launcher (ChatGPT-Fix).lnk" })
                DeleteShortcutIfOwned(
                    Path.Combine(directory, name),
                    false,
                    launcher,
                    string.Equals(name, "ChatGPT-Fix-Launcher.lnk", StringComparison.OrdinalIgnoreCase),
                    false);
            return true;
        }

        private static bool DeleteShortcutIfOwned(
            string path,
            bool official,
            string launcher,
            bool allowLegacyV101,
            bool requireOwnedRemoval)
        {
            if (!File.Exists(path)) return true;
            bool owned = official ? IsOwnedOfficialShortcut(path) : IsOwnedWrapperShortcut(path, launcher);
            if (!owned && !official && allowLegacyV101)
                owned = IsLegacyV101WrapperShortcut(path, launcher);
            if (owned) File.Delete(path);
            return owned ? !File.Exists(path) : !requireOwnedRemoval;
        }

        private static bool IsOwnedOfficialShortcut(string path)
        {
            try
            {
                Type wsType = Type.GetTypeFromProgID("WScript.Shell");
                if (wsType == null || !File.Exists(path)) return false;
                dynamic ws = Activator.CreateInstance(wsType);
                dynamic shortcut = ws.CreateShortcut(path);
                string windows = Environment.GetEnvironmentVariable("SystemRoot") ?? @"C:\Windows";
                return PathsEqual((string)shortcut.TargetPath, Path.Combine(windows, "explorer.exe"))
                    && string.Equals((string)shortcut.Arguments, @"shell:AppsFolder\OpenAI.Codex_2p2nqsd0c76g0!App", StringComparison.Ordinal)
                    && PathsEqual((string)shortcut.WorkingDirectory, windows)
                    && string.Equals((string)shortcut.Description,
                        "ChatGPT official entry (managed by ChatGPT-Fix v1.0.5)", StringComparison.Ordinal);
            }
            catch { return false; }
        }

        private static bool IsLegacyV101OfficialShortcut(string path, string installRoot)
        {
            try
            {
                if (!string.Equals(Path.GetFileName(path), "ChatGPT.lnk", StringComparison.OrdinalIgnoreCase))
                    return false;
                if (string.IsNullOrEmpty(installRoot) || !File.Exists(path)) return false;
                EnsureNoReparseComponents(path);
                EnsureNoReparseComponents(installRoot);
                Type wsType = Type.GetTypeFromProgID("WScript.Shell");
                if (wsType == null) return false;
                dynamic ws = Activator.CreateInstance(wsType);
                dynamic shortcut = ws.CreateShortcut(path);
                string target = Path.GetFullPath((string)shortcut.TargetPath);
                EnsureNoReparseComponents(target);
                string targetParent = Path.GetDirectoryName(target);
                string packageDirectory = targetParent == null ? null : Path.GetDirectoryName(targetParent);
                string baselinesDirectory = packageDirectory == null ? null : Path.GetDirectoryName(packageDirectory);
                return string.Equals(Path.GetFileName(target), "ChatGPT.exe", StringComparison.OrdinalIgnoreCase)
                    && string.Equals(Path.GetFileName(targetParent), "app", StringComparison.OrdinalIgnoreCase)
                    && !string.IsNullOrEmpty(Path.GetFileName(packageDirectory))
                    && PathsEqual(baselinesDirectory, Path.Combine(Path.GetFullPath(installRoot), "baselines"))
                    && string.Equals((string)shortcut.Arguments, "", StringComparison.Ordinal)
                    && PathsEqual((string)shortcut.WorkingDirectory, targetParent)
                    && string.Equals((string)shortcut.Description, "ChatGPT", StringComparison.Ordinal);
            }
            catch { return false; }
        }

        private static bool IsOwnedWrapperShortcut(string path, string launcher)
        {
            try
            {
                Type wsType = Type.GetTypeFromProgID("WScript.Shell");
                if (wsType == null || !File.Exists(path)) return false;
                dynamic ws = Activator.CreateInstance(wsType);
                dynamic shortcut = ws.CreateShortcut(path);
                return PathsEqual((string)shortcut.TargetPath, launcher)
                    && string.Equals((string)shortcut.Arguments, "", StringComparison.Ordinal)
                    && PathsEqual((string)shortcut.WorkingDirectory, Path.GetDirectoryName(launcher))
                    && string.Equals((string)shortcut.Description,
                        "ChatGPT-Fix Launcher wrapper (managed by ChatGPT-Fix v1.0.5)", StringComparison.Ordinal);
            }
            catch { return false; }
        }

        private static bool IsLegacyV101WrapperShortcut(string path, string launcher)
        {
            try
            {
                if (!string.Equals(Path.GetFileName(path), "ChatGPT-Fix-Launcher.lnk",
                    StringComparison.OrdinalIgnoreCase)) return false;
                Type wsType = Type.GetTypeFromProgID("WScript.Shell");
                if (wsType == null || !File.Exists(path)) return false;
                dynamic ws = Activator.CreateInstance(wsType);
                dynamic shortcut = ws.CreateShortcut(path);
                return PathsEqual((string)shortcut.TargetPath, launcher)
                    && string.Equals((string)shortcut.Arguments, "", StringComparison.Ordinal)
                    && PathsEqual((string)shortcut.WorkingDirectory, Path.GetDirectoryName(launcher))
                    && string.IsNullOrEmpty((string)shortcut.Description);
            }
            catch { return false; }
        }

        private static bool IsLegacyV102OfficialFallback(string path)
        {
            try
            {
                if (!string.Equals(Path.GetFileName(path), "ChatGPT (official).lnk",
                    StringComparison.OrdinalIgnoreCase)) return false;
                if (!File.Exists(path)) return false;
                Type wsType = Type.GetTypeFromProgID("WScript.Shell");
                if (wsType == null) return false;
                dynamic ws = Activator.CreateInstance(wsType);
                dynamic shortcut = ws.CreateShortcut(path);
                string windows = Environment.GetEnvironmentVariable("SystemRoot") ?? @"C:\Windows";
                return PathsEqual((string)shortcut.TargetPath, Path.Combine(windows, "explorer.exe"))
                    && string.Equals((string)shortcut.Arguments,
                        @"shell:AppsFolder\OpenAI.Codex_2p2nqsd0c76g0!App", StringComparison.Ordinal)
                    && PathsEqual((string)shortcut.WorkingDirectory, windows)
                    && string.Equals((string)shortcut.Description,
                        "ChatGPT official entry (managed by ChatGPT-Fix v1.0.2)", StringComparison.Ordinal);
            }
            catch { return false; }
        }

        private static bool IsLegacyV102WrapperFallback(string path, string launcher)
        {
            try
            {
                if (!string.Equals(Path.GetFileName(path), "ChatGPT-Fix-Launcher (ChatGPT-Fix).lnk",
                    StringComparison.OrdinalIgnoreCase)) return false;
                if (!File.Exists(path)) return false;
                Type wsType = Type.GetTypeFromProgID("WScript.Shell");
                if (wsType == null) return false;
                dynamic ws = Activator.CreateInstance(wsType);
                dynamic shortcut = ws.CreateShortcut(path);
                return PathsEqual((string)shortcut.TargetPath, launcher)
                    && string.Equals((string)shortcut.Arguments, "", StringComparison.Ordinal)
                    && PathsEqual((string)shortcut.WorkingDirectory, Path.GetDirectoryName(launcher))
                    && string.Equals((string)shortcut.Description,
                        "ChatGPT-Fix Launcher wrapper (managed by ChatGPT-Fix v1.0.2)", StringComparison.Ordinal);
            }
            catch { return false; }
        }

        private static bool PathsEqual(string left, string right)
        {
            try { return string.Equals(Path.GetFullPath(left), Path.GetFullPath(right), StringComparison.OrdinalIgnoreCase); }
            catch { return string.Equals(left, right, StringComparison.OrdinalIgnoreCase); }
        }
    }

    public static class Program
    {
        [STAThread]
        public static void Main(string[] args)
        {
            if (args != null && args.Length == 1 && string.Equals(args[0], "install", StringComparison.OrdinalIgnoreCase))
            {
                // Explicit CLI install/upgrade for automation and recovery.
                // The Setup exe still discovers its colocated payload and the
                // official package; no test redirect is enabled here.
                var form = new SetupForm();
                bool ok = form.DoInstall();
                try
                {
                    string setupDir = Path.GetDirectoryName(
                        System.Reflection.Assembly.GetExecutingAssembly().Location) ?? ".";
                    File.WriteAllText(Path.Combine(setupDir, "install-result.log"),
                        DateTime.UtcNow.ToString("o") + " " + (ok ? "OK" : "FAILED") + "\n" +
                        (form.LastError ?? "") + "\n", new UTF8Encoding(false));
                }
                catch { }
                Console.WriteLine(ok
                    ? (form.HasWarnings
                        ? "ChatGPT-Fix install/upgrade complete with warnings; retry is safe."
                        : "ChatGPT-Fix install/upgrade complete.")
                    : "ChatGPT-Fix install/upgrade failed.");
                Environment.Exit(ok ? 0 : 1);
                return;
            }
            if (args != null && args.Length > 0 && string.Equals(args[0], "--test-install", StringComparison.OrdinalIgnoreCase))
            {
                // CLI self-test install (automation): runs the FULL DoInstall
                // flow with no window. Use together with a sidecar
                // test-config.json that redirects install_root/shell_root/
                // uninstall_subkey to a throwaway directory, so the real
                // installation is never touched. Exit code 0 = success.
                SetupForm.SetCliMode(true);
                var form = new SetupForm();
                bool ok = false;
                try { ok = form.DoInstall(); }
                catch (Exception ex) { Console.WriteLine("TEST-INSTALL EXCEPTION: " + ex); }
                try
                {
                    File.WriteAllText(Path.Combine(
                        Path.GetDirectoryName(System.Reflection.Assembly.GetExecutingAssembly().Location) ?? ".",
                        "test-install.log"),
                        DateTime.Now.ToString("yyyy-MM-dd HH:mm:ss") + " " + (ok ? "OK" : "FAILED") + "\n" +
                        (form.LastError ?? ""));
                }
                catch { }
                Console.WriteLine(ok
                    ? "TEST-INSTALL OK"
                    : "TEST-INSTALL FAILED");
                Environment.Exit(ok ? 0 : 1);
                return;
            }
            if (args != null && args.Length > 0 && string.Equals(args[0], "uninstall", StringComparison.OrdinalIgnoreCase))
            {
                // CLI uninstall (invoked by the registry UninstallString).
                // Data-preserving: removes bin + shortcuts + registry entry,
                // keeps baselines\backups\logs.
                var form = new SetupForm();
                bool ok = form.DoUninstall();
                Console.WriteLine(ok
                    ? "ChatGPT-Fix uninstalled (data preserved)."
                    : "ChatGPT-Fix uninstall failed.");
                Environment.Exit(ok ? 0 : 1);
                return;
            }
            if (args != null && args.Length > 0 && string.Equals(args[0], "--repair", StringComparison.OrdinalIgnoreCase))
            {
                // CLI repair: rebuild both start-menu shortcuts + uninstall registry
                // entry WITHOUT touching the baseline (no copy, no state.json
                // rewrite). Use it after manual cleanup or shortcut loss.
                var form = new SetupForm();
                bool ok = form.RepairShortcutAndRegistry();
                Console.WriteLine(ok
                    ? "ChatGPT-Fix repair complete (baseline untouched)."
                    : "ChatGPT-Fix repair failed.");
                Environment.Exit(ok ? 0 : 1);
                return;
            }
            Application.EnableVisualStyles();
            Application.SetCompatibleTextRenderingDefault(false);
            Application.Run(new SetupForm());
        }
    }
}
