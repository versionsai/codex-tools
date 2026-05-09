# Codex Tools

Codex Tools 是给 Codex 用的 Provider 管理工具。

它解决的问题很具体：当你在官方 ChatGPT 登录和第三方 API Key Provider 之间来回切换时，Codex 的配置、登录状态和历史线程很容易变得不一致。这个工具把 Provider 配置保存下来，切换时自动写回 Codex，并顺手把本地线程里的 Provider 也合并好。

它不是通用 AI CLI 管理器，也不准备去管 Claude Code、Gemini CLI 或其他工具。就先把 Codex 这一件事做好。

## 现在能做什么

- 添加、编辑、删除 Codex Provider。
- 保留一个不能删除的官方 `openai` Provider，对应 Codex 的 ChatGPT 登录模式。
- 支持第三方 API Key Provider。
- 切换 Provider 时写入 `~/.codex/config.toml` 和 `~/.codex/auth.json`。
- 切换后自动合并本地线程 Provider，避免旧会话消失。
- 编辑 Provider 时，可以从当前 Codex 配置反向同步模型和推理强度。
- macOS 下常驻状态栏，不需要一直占着 Dock。
- 状态栏菜单里可以直接切换 Provider。

WebDAV 配置入口已经放进去了，但跨平台同步能力还在继续补。

## 下载

直接去仓库 Releases 下载：

[https://github.com/versionsai/codex-tools/releases](https://github.com/versionsai/codex-tools/releases)

macOS 可以用 `Codex.Tools-macos.zip`。

Windows 可以用 `.exe` 安装包，或者 `.msi`。

## Provider 是怎么处理的

Codex 现在大致有两种使用方式。

第一种是官方 ChatGPT 登录。也就是 Codex 自己登录 OpenAI 账号，这时 Provider 是内建的 `openai`。Codex Tools 不会写 `[model_providers.openai]`，因为这个 ID 是 Codex 保留的，覆盖它会导致 Codex 报错。

第二种是第三方 API Key。比如你自己填一个 Provider ID、Base URL、API Key、模型和推理强度。切换到这种 Provider 时，Codex Tools 会写入类似这样的配置：

```toml
model_provider = "custom"
model = "gpt-5.5"
model_reasoning_effort = "high"
disable_response_storage = true

[model_providers.custom]
name = "custom"
wire_api = "responses"
requires_openai_auth = true
base_url = "https://example.com/v1"
```

对应的 `auth.json` 会尽量保持简单：

```json
{
  "OPENAI_API_KEY": "sk-..."
}
```

切换 Provider 前，工具会先把当前 Provider 的配置保存成快照，再应用目标 Provider。这样你在 Codex 里改了模型或推理强度，回到 Codex Tools 编辑当前 Provider 时，也可以同步回来。

## Codex 目录

默认读取：

```text
~/.codex
```

Windows 下通常是：

```text
C:\Users\<用户名>\.codex
```

如果你用自定义目录，设置 `CODEX_HOME`：

```bash
export CODEX_HOME="/path/to/.codex"
```

```powershell
$env:CODEX_HOME="D:\Codex\.codex"
```

Provider 快照保存在应用自己的配置目录里，不直接塞进 Codex 目录。常见位置是：

```text
~/Library/Application Support/codex-tools/providers.json
%APPDATA%\codex-tools\providers.json
```

## 本地开发

需要这些东西：

- Node.js
- npm
- Rust
- macOS 打包需要 Xcode Command Line Tools
- Windows 打包建议装 Visual Studio Build Tools

安装依赖：

```bash
npm install
```

跑开发版：

```bash
npm run tauri dev
```

只看前端：

```bash
npm run dev
```

## 打包

```bash
npm run build
npm run tauri build
```

macOS 常见产物：

```text
src-tauri/target/release/bundle/macos/Codex Tools.app
src-tauri/target/release/bundle/dmg/*.dmg
```

Windows 常见产物：

```text
src-tauri/target/release/codex-tools.exe
src-tauri/target/release/bundle/nsis/*.exe
src-tauri/target/release/bundle/msi/*.msi
```

## 还没做完的地方

- WebDAV 推送线程到远端。
- WebDAV 拉取远端线程。
- 同步冲突检测。
- 更完整的 Windows 桌面体验验证。
- 依赖许可证和第三方项目致谢整理。

## 和 cc-switch、codex- 的关系

这个工具的 Provider 切换、状态栏入口、Provider 列表管理，参考了 `cc-switch` 的设计思路。

线程归并和 WebDAV 同步方向，来自 `codex-` 相关探索。

我不想把这些来源藏起来。Codex Tools 只是把这些已经被验证过的思路，重新整理到 Codex 这个更窄的场景里。

如果你需要更完整的 Claude Code / 多 CLI 切换，建议直接看 `cc-switch`。如果你想看 Codex 同步方向的早期探索，可以看 `codex-`。

## 使用前提醒

这个工具会读写：

```text
~/.codex/config.toml
~/.codex/auth.json
```

也会修改本地线程文件里的 Provider 字段。第一次用之前建议备份整个 `.codex` 目录，尤其是你已经有很多历史会话的时候。

## License

MIT License，见仓库根目录的 `LICENSE`。
