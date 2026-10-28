# English Companion Lite · 英语伴学 Lite

[English](README_EN.md) · [更新日志](CHANGELOG.md) · [构建说明](docs/BUILD.md) · [验证范围](TESTING.md)

在明确授权的 Windows 编辑控件中输入中文，句子稳定后显示英文学习提示，**不替换、不插入、不修改原文**。继续使用微软拼音，无需更换输入法。

源码版本 **0.1.15（早期预览）**。这是一个 Rust + Win32 桌面工具，适用于 **Windows 10/11 x64**。对目标应用的支持取决于其 Windows UI Automation 接口实现。

## 功能与特点

- **三种翻译服务**：DeepSeek 非推理翻译、MyMemory（无需 API Key）和 Azure Translator，用户明确选择服务，不自动切换接收方。新配置默认选择 DeepSeek，旧配置保留原服务。
- **本地 Mock 测试**：显示明确测试标记和字数，不提供真实译文，不需要联网。
- **授权后检测**：可单独授权微软记事本，或按完整 EXE 路径授权其他应用。
- **原生桌面体验**：托盘、四页设置、键盘操作、DPI 缩放、保存反馈及诊断导出。
- **可复制提示**：可拖选译文，使用 Ctrl+C、右键或“复制全部”；悬停或操作时暂停消失计时。
- **可调触发规则**：句末停顿、无标点停顿、最小汉字数、显示时长；进程内有界缓存。
- **单个 EXE 运行**：无需 Python、Rust 运行环境或嵌入式浏览器。

## 下载与安装

项目仓库：[zsp1029/english-companion-lite](https://github.com/zsp1029/english-companion-lite)。首个公开预览版：[v0.1.14](https://github.com/zsp1029/english-companion-lite/releases/tag/v0.1.14)。

1. [下载 Windows x64 压缩包](https://github.com/zsp1029/english-companion-lite/releases/download/v0.1.15/english-companion-lite-v0.1.15-windows-x64.zip)，解压到可写目录；[SHA-256 校验文件](https://github.com/zsp1029/english-companion-lite/releases/download/v0.1.15/SHA256SUMS.txt) 随版本提供。
2. 双击 `english-companion-lite.exe`。第一次启动会创建配置并打开设置。
3. Windows 可能对未签名 EXE 显示 SmartScreen 提示。请确认来源，并用 `Get-FileHash` 与附件 `SHA256SUMS.txt` 核对完整性。
4. 关闭设置只会隐藏窗口；要退出，请使用右下角托盘菜单。卸载时退出程序后删除自己解压的目录；配置仍保留在下述用户目录，是否删除由你决定。

本项目未提供安装器、自动更新或代码签名。旧版本升级前请先从托盘退出，并备份个人配置。

## 第一次使用：先做离线测试

1. 在“常规设置”选择 Mock 测试，保持“允许联网”关闭。
2. 在“应用授权”授权微软记事本，再启用自动翻译，点击“保存并应用”。
3. 打开新版微软记事本的新文档，用微软拼音输入：`今天工作比较忙，晚上想早点休息。`
4. 输入结束后应出现包含 `[MOCK / 测试模式]` 的提示，默认约 4 秒后消失；继续输入会关闭旧提示。
5. 若无提示，查看“运行诊断”。控件不提供所需 TextPattern/TextEditPattern 时会跳过。不要通过提升管理员权限绕过应用限制。

默认自动翻译、记事本授权和联网均关闭，额外白名单为空。程序不是输入法，没有注册键盘钩子，也不轮询剪贴板。

### 使用真实翻译

**DeepSeek（推荐）**：在设置中选择 DeepSeek，直接在下方填写 API Key，点击「保存并应用」后立即生效，无需环境变量或重启。密钥输入默认遮蔽，保存到当前 Windows 用户的凭据管理器；留空保留已有密钥，填写新值可替换。取消 Mock、授权应用并允许联网即可翻译。使用固定官方 HTTPS 接口及 `deepseek-flash`，显式关闭思考；只发送当前句子，返回一版自然英文。API 按量计费，密钥不写入 TOML、URL 或诊断；仍兼容 `DEEPSEEK_API_KEY` 作为未保存密钥时的备用来源。见 [DeepSeek 配置](docs/DEEPSEEK.md)。

**MyMemory**：选择 MyMemory，取消 Mock，允许联网并保存。联系邮箱可以留空；若填写，该邮箱会发送给服务方。免费额度与网络可用性由服务方决定，详见 [官方 API](https://mymemory.translated.net/doc/spec.php) 和 [使用限额](https://mymemory.translated.net/doc/usagelimits.php)。单句最多 500 个 UTF-8 字节，超长跳过，不拆分发送；配额耗尽后冷却 1 小时，限流冷却 60 秒，网络/服务错误冷却 10 秒。

**Azure Translator**：在 Azure 中自行创建资源，选择 Azure 并配置 `AZURE_TRANSLATOR_KEY` 和需要时的 `AZURE_TRANSLATOR_REGION` 环境变量。默认端点为 `https://api.cognitive.microsofttranslator.com`，自定义支持域名的端点通过 TOML 设置。密钥不写入配置、不放进截图或反馈。具体启动示例见 [Azure 配置](docs/AZURE.md)。本项目尚未用有效密钥验收 Azure 真实翻译。

### 授权其他应用与隐私

额外应用按 EXE 的**完整绝对路径**授权，例如配置示例中的虚构路径。授权涵盖该进程里符合规则的可编辑控件，无法识别所有未标记密码的敏感业务字段。

启用联网后，触发的中文会发送给所选第三方服务。DeepSeek 使用 HTTPS POST 发送当前句子，不发送聊天历史、窗口标题或其他应用文本；MyMemory 将原文放在 HTTPS URL 的查询参数中。本工具不调用贡献翻译的 `/set` 接口，但服务方仍可能记录或使用请求，受其条款和隐私政策约束。不要用于敏感聊天或保密资料。密码保护依赖目标应用正确报告属性。

缓存仅在进程内，最多 128 条，退出释放。诊断不记录输入正文，但可能包含进程路径、窗口类和应用元数据；分享前仍应检查。暂停会关闭采集和显示入口，已经发送的请求无法撤回，正在执行的调用需要返回后清理。

## 配置

实际配置：`%LOCALAPPDATA%\EnglishCompanionLite\config.toml`。优先使用设置页；通过托盘打开 TOML 编辑后，使用“重新加载配置”。不要把实际配置提交到仓库。

| 设置 | 默认值 | 范围 |
| --- | --- | --- |
| 句末停顿 | 300 ms | 200–5000 ms |
| 无标点停顿 | 1500 ms | 500–15000 ms |
| 显示时长 | 4000 ms | 500–30000 ms |
| 最小汉字数 | 6 | 1–300 |

全局单句上限为 300 个 Unicode 字符，服务可能有更小限制。进入控件时先建立基线，不翻译原有文字；仅移动光标不会翻译已有内容。

## 源码构建

需要 Windows x64、Rust Stable（项目声明最低 Rust 1.85；最低版本尚未单独验证）以及 C++ 链接工具。推荐安装 Rust 的 `x86_64-pc-windows-msvc` 工具链、Visual Studio Build Tools 的“使用 C++ 的桌面开发”工作负载和 Windows SDK。

```powershell
cargo test --locked
cargo build --release --locked
.\target\release\english-companion-lite.exe
```

依赖由 `Cargo.toml` 声明、`Cargo.lock` 锁定。无需 Python、Node.js 或独立资源编译器。当前机器实际验证 GNU x64 工具链；MSVC 尚未验证。详细构建与发布打包命令见 [docs/BUILD.md](docs/BUILD.md)。

## 项目结构

```text
src/                 程序代码：UIA、翻译、设置、托盘、提示与缓存
assets/              SVG 图标源、ICO 与 Windows manifest
examples/            受控诊断和翻译探针
scripts/             开发检查、桌面测试和发布打包
tests/              配置和句子规则测试
docs/               构建、隐私与发布说明
third-party/         锁定依赖的许可证和声明
Cargo.toml           Rust 依赖与发布优化配置
Cargo.lock           可复现依赖版本
build.rs             内嵌图标与 manifest
config.example.toml  不含凭据的安全配置示例
```

`.tools/`、`target/`、`artifacts/` 和 `release-dist/` 仅供本地开发或发布，不提交源码仓库。

## 常见问题

**没有提示？** 确认已启用、授权实际输入应用、点击保存；先用记事本和 Mock 排查。至少输入 6 个汉字并结束组合输入。其他应用必须暴露所需无障碍接口；QQ 在不同环境下不保证可用。

**只有测试文字？** Mock 不翻译。真实翻译需要取消 Mock、选择服务并允许联网；DeepSeek 可直接在设置中保存有效密钥，Azure 使用环境变量。只有更改环境变量才需要重启程序。服务不会自动切换，失败时查看诊断。

**为什么消耗较多额度？** 无标点停顿可能在句子尚未完成时触发，可改为 3000–5000 ms。继续补写会形成新的请求；同句缓存只在本次进程内有效。

**为什么关闭窗口后还在运行？** 程序在托盘运行，使用托盘“退出”结束。

**如何反馈？** 提供系统版本、目标应用版本、复现步骤和脱敏诊断。不要上传密钥、真实聊天、实际配置或含个人信息的截图。安全问题处理见 [SECURITY.md](SECURITY.md)。

## 许可证与第三方资源

本项目采用 [MIT](LICENSE)，Copyright © 2026 zsp1029。依赖保留各自许可证，见 [第三方声明](THIRD_PARTY_NOTICES.md)。翻译服务不包含在 MIT 授权内，使用时遵守其条款；Windows 系统组件不随项目分发。

截图暂未纳入公开文档。可以提供“默认设置页”和“记事本中使用固定示例句的提示框”两张截图；请避开任务栏账户、实际路径、邮箱、聊天和密钥，不需要展示真实个人会话。


