# 首次 GitHub 发布操作

这些命令是供维护者审阅的操作说明，不会自动执行。用户最终确认公开仓库、文件清单、名称、版本和发布包后，才能执行创建、push 和 Release 上传。

## 建议仓库信息

- Owner: `zsp1029`
- Name: `english-companion-lite`
- Description: `A lightweight Windows desktop companion that turns newly typed Chinese into English learning hints using UI Automation.`
- Topics: `windows`, `rust`, `win32`, `uiautomation`, `translation`, `english-learning`, `desktop-app`, `chinese`
- License: MIT（已由作者确认），首个预览标签 `v0.1.14`。

## 登录

安装 [GitHub CLI](https://cli.github.com/)，或使用本机已校验的便携版。`gh` 未进入 PATH 时，将下面 `gh` 替换为便携 EXE 的完整路径。不要把 `.tools` 提交到仓库。

```powershell
gh auth login --hostname github.com --git-protocol https --web
gh auth status --hostname github.com
gh api user --jq .login
```

在浏览器里登录并授权，确认当前账户是 `zsp1029`。密码或令牌不应发送到聊天。设备授权码仅在官方授权页面输入。

## 最终确认前

1. 审阅上传清单、敏感信息检查和包内容，确认图标与代码有权公开。
2. 检查是否已有远程同名仓库；若存在，先读取内容，不覆盖。
3. 只在本地初始化和审阅暂存文件；不要使用 `git add .` 来跳过文件审核。
4. 使用 GitHub 的 noreply 邮箱提交，先在 GitHub Settings → Emails 核对自己的实际 noreply 地址，不猜测账户 ID，也不修改全局 Git 配置。
5. README 的发布状态和下载链接必须以实际结果更新；发布前不要宣称已有下载。

## 获得最终确认后

以下示意命令假设本地已经审核并完成首次提交、分支为 `main`，且没有 origin。实际执行前重新检查状态：

```powershell
gh repo view zsp1029/english-companion-lite
```

只有确认 GitHub 返回“仓库不存在”，才执行创建。网络错误或权限错误不能当作不存在：

```powershell
gh repo create zsp1029/english-companion-lite --public --source . --remote origin
git push -u origin main
gh repo edit zsp1029/english-companion-lite --description 'A lightweight Windows desktop companion that turns newly typed Chinese into English learning hints using UI Automation.' --add-topic windows,rust,win32,uiautomation,translation,english-learning,desktop-app,chinese
gh release create v0.1.14 .\release-dist\english-companion-lite-v0.1.14-windows-x64.zip .\release-dist\SHA256SUMS.txt --repo zsp1029/english-companion-lite --target main --title '英语伴学 Lite v0.1.14 · 首个公开预览版' --notes-file .\docs\RELEASE_NOTES_zh-CN.md --prerelease
```

README 更新为实际链接后，应在上传附件之前重新打包，让 ZIP 内说明保持一致。使用 `package-release.ps1 -OutputDirectory .\release-dist\final` 保留旧候选；上述上传命令也需改为最终目录。完成源码更新提交和 push 后，才执行 Release 命令。

## 发布后核验

```powershell
gh repo view zsp1029/english-companion-lite --json url,visibility,defaultBranchRef,licenseInfo
gh release view v0.1.14 --repo zsp1029/english-companion-lite --json url,isPrerelease,assets,tagName
```

检查公开主页、中英文 README 链接、MIT 识别、标签指向、附件下载与 SHA-256。必须实际下载到新目录核对哈希，不能只凭上传命令成功认定下载正常。若账号尚未授权或网络不可用，应明确记录未完成的步骤。
