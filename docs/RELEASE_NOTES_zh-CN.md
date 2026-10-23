# 英语伴学 Lite v0.1.14

首个公开预览版本。面向 Windows 10/11 x64，在授权的编辑控件中输入中文后，显示英文学习提示，保留原输入内容和微软拼音行为。

## 功能

- MyMemory 翻译（无需 API Key）、Azure Translator，以及明确标记的 Mock 测试。
- 原生设置与托盘操作；按 EXE 完整路径授权额外应用。
- 可选择及复制的提示文字、触发延迟设置、进程内缓存、诊断导出。
- 默认关闭自动翻译和联网，首次启动打开设置。

## 下载与使用

1. 下载本版本附件 `english-companion-lite-v0.1.14-windows-x64.zip`，解压至自己的目录。
2. 双击 `english-companion-lite.exe`。不需要安装 Rust 或 Python。
3. 先授权记事本，打开 Mock 和自动翻译，保持联网关闭，保存后在新记事本中输入一段中文进行本地测试。
4. 需要真实翻译时选择 MyMemory，关闭 Mock 并明确允许联网。Azure 的密钥配置见 README。
5. 右下角托盘菜单可暂停、打开设置或退出。关闭设置窗口不会退出程序。

## 兼容性与注意事项

- Windows 10/11 x64；当前机器验证 GNU 工具链，MSVC 尚未验证。
- 各应用的无障碍接口实现存在差异，QQ 跨机器兼容性仍需反馈。
- 联网后符合规则的中文会发送给所选服务；MyMemory 原文放在 HTTPS URL 查询参数中，可选联系邮箱也会发送。不要授权敏感业务应用。
- Mock 不产生真实译文；Azure 真实请求未完成有效密钥验收。
- 本版本未签名，可能出现 SmartScreen 提示。可使用 `Get-FileHash` 与发布的 `SHA256SUMS.txt` 核对完整性；哈希不能代替可信来源或代码签名。
- 配置保存在 `%LOCALAPPDATA%\EnglishCompanionLite\config.toml`；升级前退出旧程序并自行备份配置。
- 已发出的网络请求无法撤回；暂停后正在执行的调用仍需返回。

采用 MIT 许可证；依赖及运行库声明随压缩包附带。验证范围见 `TESTING.md`。
