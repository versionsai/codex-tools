# Codex Tools

`Codex Tools` 是给 Codex 用的 Provider 管理和线程同步工具。

它解决的问题很具体：当你在官方 ChatGPT 登录和第三方 API Key Provider 之间来回切换时，Codex 的配置、登录状态和历史线程很容易变得不一致。这个工具把 Provider 配置保存下来，切换时自动写回 Codex，并顺手把本地线程里的 Provider 也合并好。

它不是通用 AI CLI 管理器，也不准备去管 Claude Code、Gemini CLI 或其他工具。这里先把 Codex 这一件事做好。

## 下载

最新版本在 Releases 里：

[https://github.com/versionsai/codex-tools/releases](https://github.com/versionsai/codex-tools/releases)

目前会打包这些文件：

- `Codex Tools-macos.zip`
- `Codex Tools_*.exe`
- `Codex Tools_*.msi`

macOS 版本暂时没有做 Apple Developer ID 签名和 notarization。下载 `Codex Tools-macos.zip` 后，解压会得到：

```text
Codex Tools-macos/
├── Codex Tools.app
└── install.command
```

双击 `install.command` 会自动把 `Codex Tools.app` 安装到 `/Applications`，并执行：

```bash
xattr -dr com.apple.quarantine "/Applications/Codex Tools.app"
```

如果 macOS 提示是否允许运行脚本，请选择允许。这个脚本只做三件事：复制 App 到 `/Applications`、移除 quarantine 标记、打开 Codex Tools。

## 平台状态

目前实际验证过的是 macOS。

macOS 下 Provider 切换、Codex 配置写入、线程 Provider 合并、状态栏入口和 WebDAV 推拉都已经做过本机验证。

Windows 现在只是能通过 GitHub Actions 编译出 `.exe` / `.msi` 安装包，还没有在真实 Windows 机器上完整验证 Provider 切换、Codex 配置写入、线程归并和托盘行为。所以 Windows 制品先当作实验性版本看待，不建议直接拿来放重要环境里用。

## 功能

- 管理多个 Codex Provider。
- 固定保留官方 `openai` Provider，用来对应 Codex 官方 ChatGPT 登录模式。
- 为第三方 API Key Provider 保存独立配置。
- 切换 Provider 时写入 `~/.codex/config.toml` 和 `~/.codex/auth.json`。
- 切换后自动合并本地线程 Provider。
- 查看 Codex 本地用量统计，按日期和 Provider 汇总 token 与预估 Cost。
- 通过 WebDAV 推送和拉取 Codex 线程文件。
- macOS 下常驻状态栏，可以从状态栏菜单直接切 Provider。

## 用量统计

`Codex Tools` 可以离线读取本机 Codex 线程日志，展示每天的输入、缓存输入、输出、推理输出、总 token 和预估 Cost，也会按 Provider 汇总总量。

统计口径对齐：

```bash
npx ccusage@latest codex daily --config /tmp/no-such-ccusage.json --offline
```

具体规则：

- 只读取 `~/.codex/sessions/**/*.jsonl`，不把 `archived_sessions` 纳入用量页，避免历史归档重复计入。
- 优先使用日志里的 `last_token_usage`；缺失时再用 `total_token_usage` 和上一条累计值计算增量。
- 使用 `timestamp + model + token usage` 去重，避免同一条 token 事件重复计数。
- 日期按本机时区聚合。
- Cost 按 OpenAI 官方模型价格估算，用来统一比较不同 Provider 下的消耗。

这个页面只做本地离线统计，不会上传日志，也不会调用远端接口查询用量。

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

## 同步范围

WebDAV 线程同步只处理和线程相关的文件：

```text
~/.codex/sessions/**/rollout-*.jsonl
~/.codex/archived_sessions/**/rollout-*.jsonl
~/.codex/session_index.jsonl
```

不会同步这些本地状态和日志：

```text
~/.codex/state_5.sqlite
~/.codex/logs_2.sqlite
*.wal
*.shm
```

## Codex 目录

工具默认读取当前用户的 Codex 目录：

```text
~/.codex
```

Windows 下通常是：

```text
C:\Users\<用户名>\.codex
```

如果你用的是自定义目录，可以设置 `CODEX_HOME`：

```bash
export CODEX_HOME="/path/to/.codex"
```

```powershell
$env:CODEX_HOME="D:\Codex\.codex"
```

代码里不应该写死任何人的本机路径。如果你发现哪里写死了，欢迎直接提 issue。

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

## CI/CD

仓库里有这些 GitHub Actions：

- `CI`：每次 push / PR 跑前端构建和 Rust 检查。
- `Release`：推送 `v*` tag 时自动构建 macOS / Windows 产物并发布 Release。
- `Windows Build`：手动或相关路径变更时构建 Windows 安装包。

发布一个新版本大概是这样：

```bash
git tag v0.1.0
git push origin v0.1.0
```

每次重要变更会记录在 [CHANGELOG.md](CHANGELOG.md)。

## 风险提示

这个工具会读写：

```text
~/.codex/config.toml
~/.codex/auth.json
```

也会修改本地线程文件里的 Provider 字段，并通过 WebDAV 推拉线程文件。第一次用之前建议备份整个 `.codex` 目录，尤其是你已经有很多历史会话的时候。

## 项目结构

```text
.
├── package.json
├── src
├── src-tauri
├── .github/workflows
└── README.md
```

## 和 cc-switch、codex- 的关系

这个工具的 Provider 切换、状态栏入口、Provider 列表管理，参考了 `cc-switch` 的设计思路。

线程归并和 WebDAV 同步方向，来自 `codex-` 相关探索。

我不想把这些来源藏起来。Codex Tools 只是把这些思路重新整理到 Codex 这个更窄的场景里。

如果你需要更完整的 Claude Code / 多 CLI 的切换，建议直接看 `cc-switch`。如果你想看 Codex 同步方向的早期探索，可以看 `codex-`。

## License

MIT License，见 [LICENSE](LICENSE)。
