# Azure Translator 配置

此步骤会启用第三方联网服务，需要你自己的 Azure 资源；本项目尚未使用有效密钥完成真实请求验收。

1. 在 [Azure 门户](https://portal.azure.com/) 创建 Translator，确认计费、端点、密钥与 Region。参见 [官方身份验证说明](https://learn.microsoft.com/en-us/azure/ai-services/translator/text-translation/reference/authentication)。
2. 从托盘退出英语伴学。在 EXE 所在目录打开 PowerShell，安全输入密钥，不要把密钥粘贴到代码或聊天中：

```powershell
$translatorSecret = Read-Host 'Azure Translator Key' -AsSecureString
try {
    $env:AZURE_TRANSLATOR_KEY = [Net.NetworkCredential]::new('', $translatorSecret).Password
    # 改成资源页面上的实际 Region；不需要 Region 的全球单服务资源可省略此行。
    $env:AZURE_TRANSLATOR_REGION = 'eastasia'
    Start-Process .\english-companion-lite.exe
} finally {
    Remove-Item Env:AZURE_TRANSLATOR_KEY -ErrorAction SilentlyContinue
    Remove-Item Env:AZURE_TRANSLATOR_REGION -ErrorAction SilentlyContinue
    Remove-Variable translatorSecret
}
```

子进程继承环境变量；此示例适合此前没有设置这些变量的独立终端。以后重新启动需再次提供。程序不读取 `.env`。

3. 在设置中选择 Azure，取消 Mock，明确允许联网，授权目标应用并启用，保存应用。

默认端点为 `https://api.cognitive.microsofttranslator.com`。自定义资源主机示例为 `https://your-resource.cognitiveservices.azure.com`，自动补 `/translator/text/v3.0`；不要加入查询参数。只支持代码允许的 Azure HTTPS 域名，禁止重定向。私有网络/VNet 未验证。

缺少密钥时不会发送请求或切换到 Mock。401/403 检查密钥、Region 和权限；429 冷却 60 秒；网络和服务错误冷却 10 秒。修改配置后输入新的非敏感测试句重试，不会自动反复请求。
