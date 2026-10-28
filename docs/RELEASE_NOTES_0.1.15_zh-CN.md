# 英语伴学 Lite v0.1.15 · 预览版

本次预览新增 DeepSeek 翻译，面向 Windows 10/11 x64。

## 更新

- 支持 DeepSeek 官方 API 的短句翻译，使用 `deepseek-flash` 并关闭思考模式。
- 可在设置中输入 API Key；密钥存储在当前 Windows 用户的凭据管理器中，也支持通过 `DEEPSEEK_API_KEY` 提供。
- 新配置默认选择 DeepSeek；原有配置保留已选服务。自动翻译、应用授权和联网仍需用户开启。
- 增加余额不足提示和冷却，不会自动切换翻译服务。

## 下载与使用

下载 Windows x64 ZIP 和 `SHA256SUMS.txt`，校验后解压运行。升级前退出旧版本并备份个人配置。选择 DeepSeek、填写 API Key、授权目标应用并允许联网后可使用真实翻译。API 按量计费；详见 `docs/DEEPSEEK.md`。

## 已知限制

- DeepSeek 真实付费接口及本次候选包的完整桌面流程尚未验收；请参阅 `TESTING.md` 的验证范围。
- 程序未签名，可能触发 SmartScreen。哈希只能核对文件完整性，不能替代可信来源或代码签名。
- 目标应用的 Windows UI Automation 兼容性存在差异；MSVC 构建尚未独立验证。
