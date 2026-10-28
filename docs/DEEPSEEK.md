# DeepSeek 翻译配置

v0.1.15 增加 DeepSeek 非推理翻译；原 v0.1.14 发布包不包含该功能。新配置默认选择 DeepSeek，现有配置中的 MyMemory / Azure 选择保持不变。

## 配置密钥

1. 在 [DeepSeek 开放平台](https://platform.deepseek.com/) 创建自己的 API Key，确认 API 余额和[计费规则](https://api-docs.deepseek.com/quick_start/pricing/)。
2. 在「常规设置」选择「DeepSeek（非推理翻译）」，下方会直接显示 API Key 输入框。粘贴自己的 Key，点击「保存并应用」，无需重启。
3. 输入内容默认遮蔽；密钥保存到当前 Windows 用户的凭据管理器，按配置文件路径区分，不写入 TOML。再次打开设置只显示「已保存」，不会把密钥填回输入框。留空保存保留原密钥，输入新值保存则替换。
4. 取消 Mock，打开「允许联网」，在「应用授权」授权记事本，启用自动翻译，保存并应用。
5. 在新的记事本中输入：`今天工作比较忙，晚上想早点休息。`，检查完整英文提示。保存本身只检查格式，不发送付费测试请求；Key 有效性由实际翻译请求确认。

不要将密钥发到聊天、提交到仓库或放进 TOML。需要删除已存密钥时，可在 Windows 凭据管理器的「普通凭据」中删除对应的 `EnglishCompanionLite/DeepSeek/…` 条目。

高级用法仍兼容 `DEEPSEEK_API_KEY` 环境变量，仅在当前配置没有已保存密钥时使用；设置页保存的密钥优先。更改环境变量后需从新进程启动，设置页保存则立即生效。以下是可选示例，切勿提交真实密钥：

```powershell
$env:DEEPSEEK_API_KEY = '<你的 API Key>'
& '.\target\release\english-companion-lite.exe'
```

已有配置可通过设置页切换，也可将 TOML 的 `provider` 改为 `'deepseek'` 后使用托盘「重新加载配置」。只更改服务不会自动打开联网或采集授权。

## 请求行为

- 固定官方地址：`https://api.deepseek.com/chat/completions`；不允许代理商端点，也不跟随 HTTP 重定向。
- 当前模型：`deepseek-flash`，`thinking.type = disabled`。不使用已停用的 `deepseek-chat` 别名。参见[官方模式说明](https://api-docs.deepseek.com/guides/thinking_mode/)。
- 单次独立请求：固定翻译提示词及当前中文句子，不携带聊天历史、窗口标题、进程路径、MyMemory 邮箱或白名单。
- 非流式；最多 768 个输出 tokens，全局输入上限仍为 300 个 Unicode 字符。只接受正常结束的完整译文，截断、拒绝、工具调用及无效响应不会当作译文显示。
- 不展示思考内容、不调用工具、不自动重试、不自动切换其他厂商。
- 复用有界内存缓存，模型及提示词版本进入缓存标识；继续输入使旧结果失效，排队任务仅保留最新一句。已发送请求无法撤回，执行中的同步 HTTP 仍需返回或超时后才能处理下一句。
- 保留已有网络超时及 64 KiB 响应限制；长时间无响应会失败。

## 常见错误

| 提示 | 处理 |
| --- | --- |
| 未设置 DeepSeek API Key | 在设置中填写 API Key 并保存。缺少 Key 时不会发送请求。 |
| 无法读取或保存 API Key | 检查 Windows 凭据管理器是否可用，失败时不会提示保存成功。 |
| 401 | 检查 Key 是否有效。 |
| 402 | API 余额不足，冷却 1 小时；充值后重启程序可清除冷却。 |
| 403 | 检查账号权限或网络策略。 |
| 429 | 限流，冷却 60 秒。 |
| 网络失败、超时或服务错误 | 冷却 10 秒；检查网络后输入新句子。 |
| 响应无效 | 可能为截断、拒绝或格式异常；不会展示半句、思考内容或服务端错误正文。 |

诊断不记录原句、Key 或原始服务响应。发给第三方的文字仍受 DeepSeek 服务条款和隐私政策约束。

## 验证

默认离线测试不访问 DeepSeek，也不消耗 API 余额：

```powershell
.\scripts\dev.ps1 -Action test
.\scripts\dev.ps1 -Action clippy
.\scripts\dev.ps1 -Action release
```

手工启用以下测试会使用当前配置已保存的密钥（否则使用 `DEEPSEEK_API_KEY`），向付费接口发送上面的固定示例句一次，不读取个人应用内容：

```powershell
cargo test --locked deepseek_live_worker -- --ignored --nocapture
```

返回译文只证明接口及处理流程可用；自然度与响应速度需要用脱敏句集进一步评估。
