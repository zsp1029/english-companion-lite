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
