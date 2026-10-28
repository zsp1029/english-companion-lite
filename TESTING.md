# 验证范围

本文件只记录公开可复现的步骤。历史开发记录、截图和机器诊断保留在被忽略的 `artifacts/`，不随源码发布。

## 离线验证

```powershell
.\scripts\dev.ps1 -Action test
.\scripts\dev.ps1 -Action fmt
.\scripts\dev.ps1 -Action clippy
.\scripts\dev.ps1 -Action release
```

`test` 默认跳过真实联网请求和独立桌面焦点测试。配置和句子规则测试在 `tests/`；缓存、并发请求失效、授权与网络隔离、诊断脱敏等测试在 `src/`。

## 桌面验收

先保存个人文档并通过托盘退出正在运行的本工具；脚本会检查已有实例，使用隔离配置。不要强制结束个人会话来通过测试。

```powershell
.\scripts\smoke-settings.ps1
.\scripts\smoke-settings.ps1 -VisualQa
.\scripts\smoke-notepad.ps1
```

记事本脚本需要没有已打开的记事本；脚本使用程序化控件消息，不等同于微软拼音人工测试。不要在无需联网时添加 `-LiveMymemory`，该选项会发送固定示例句。`examples/translation_probe.rs` 也会发出真实网络请求。

DeepSeek 的固定句真实 API 测试默认跳过，需自行配置 `DEEPSEEK_API_KEY` 后显式运行，见 [DeepSeek 配置](docs/DEEPSEEK.md)。该测试调用付费接口一次。

## DeepSeek 接入验证（2026-10-26）

### API Key 设置入口（同日后续改动）

- 选择 DeepSeek 显示原生密码输入框并隐藏 MyMemory 邮箱。保存到当前 Windows 用户的凭据管理器，留空保留原密钥，保存新值立即生效并重置旧密钥相关的缓存/冷却状态。
- 最新离线测试 36 项通过，6 项默认跳过；另行执行并通过 2 项隔离凭据存取/配置失败回滚测试、1 项独立隐藏窗口控件测试。上述测试使用合成密钥，不联网、不访问个人凭据目标。
- 控件测试覆盖密码遮蔽、DeepSeek / MyMemory 切换、导航离开和返回后字段可见性。格式检查、全目标 Clippy 和 Release 构建通过。
- 完整 `smoke-settings.ps1 -CredentialQa` 检测到现有英语伴学实例而在启动前停止，没有关闭个人实例，因此本次不声明完整应用设置回归通过。退出旧程序后可运行该脚本补充验证保存反馈、空白保留和重启识别；脚本使用隔离配置并清理合成测试凭据。
- 最新候选 EXE：`artifacts/deepseek-key/build/release/english-companion-lite.exe`；未覆盖正在使用的旧版。真实付费翻译仍未执行。

### 原始接入检查记录

- GNU x64 离线测试 35 项通过，3 项显式跳过（DeepSeek / MyMemory 真实接口及独立桌面焦点测试）。
- 格式检查、全目标 Clippy（`-D warnings`）、独立 Release 构建通过。
- 新增覆盖：关闭思考、原文与系统提示分离、拒绝截断/拒绝/工具响应、密钥头部校验、402 余额不足冷却、旧服务选择兼容。
- `smoke-settings.ps1` 对本次 Release 的桌面回归通过，验证 DeepSeek 默认选择、邮箱禁用、三家服务切换与保存、接收方提示以及不自动开启联网。
- 沙箱内无法通过 `FindWindow` 定位已启动窗口，设置回归改在具有桌面访问权限的执行环境中完成。设置截图人工检查通过。
- `smoke-notepad.ps1` 在前台验证处停止（目标记事本已创建，但 `GetForegroundWindow` 返回 0）；未运行到 Mock 和新增缺少 DeepSeek Key 的断言，因此不算记事本端到端通过。
- 当前执行进程没有 `DEEPSEEK_API_KEY`，未发送真实 DeepSeek 请求；翻译质量、实际延迟和微软拼音人工流程仍需配置 Key 后验收。

本次候选 EXE：`artifacts/deepseek/build/release/english-companion-lite.exe`。未覆盖原 `target/release` 或已发布的 v0.1.14 包。

## 人工验收清单

- 新配置默认暂停、联网关闭、授权为空。
- 记事本中使用微软拼音输入固定测试句，Mock 标记正确，不改变原文。
- 提示可选择、复制、关闭；悬停和主动操作时暂停消失计时。
- 托盘暂停、重载、退出，设置页保存与错误反馈工作正常。
- 实际使用的其他应用逐个确认，不凭接口探针推断完整兼容性。
- 真实多显示器 DPI、Windows 10 外观回退需要对应机器验证。

## 本次公开准备验证（2026-10-23）

已完成：GNU x64 / Rust 1.99.1 的离线测试，32 项通过、2 项显式跳过。

后续检查结果、候选 EXE 哈希和发布包审核保存在本地 `artifacts/open-source-prep/`，由发布准备报告给出。不得把历史验证当成本次重新验收。

尚未验证：Azure 有效密钥请求、MSVC 工具链、Rust 1.85 最低版本、其他机器 QQ 兼容性、真实跨屏 DPI。历史记录包含本机 QQ 的 Mock 人工通过结果，但不保证其他环境。

本次还完成格式检查、全目标 Clippy、独立 GNU Release 构建及候选 EXE 的隔离 `--check` 和安全默认值检查。候选构建使用 `--remap-path-prefix` 去除工作区绝对路径，原正式 EXE 未修改。

本次 `smoke-settings.ps1 -VisualQa` 因检测到已有本工具实例而在启动测试前停止，没有关闭个人实例。因此不将历史的桌面通过记录作为本次候选包验收结果。发布前仍建议退出本工具后运行设置回归，并人工确认微软拼音与目标应用提示。
