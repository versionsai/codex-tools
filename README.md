# Codex Sync / Codex Tools

这个仓库里放了两个和 Codex 相关的小工具。

我最开始做它们，是因为自己在不同 Provider 之间切来切去之后，Codex 里的旧会话经常“看不见”了。后来又加了 WebDAV 同步、状态栏切换 Provider 这些东西，慢慢就拆成了两个项目：

- `Codex Sync`：macOS 原生 App，主要做 Codex 线程同步和本地线程归并。
- `Codex Tools`：Tauri 跨平台 App，主要做 Provider 管理、Provider 快速切换和线程归并。

它们都只服务 Codex，不打算做成一个“大而全”的 AI CLI 管理器。

## 下载

最新版本在 Releases 里：

[https://github.com/versionsai/codex-tools/releases](https://github.com/versionsai/codex-tools/releases)

目前会打包这些文件：

- `Codex.Sync-macos.zip`
- `Codex.Tools-macos.zip`
- `Codex.Tools_*.exe`
- `Codex.Tools_*.msi`

macOS 如果提示“无法验证开发者”，需要在系统设置里手动允许打开。现在还没有做签名和 notarization，这个后面再补。

## 平台状态

目前实际验证过的是 macOS。

`Codex Sync` 和 `Codex Tools` 都已经在 macOS 上跑通过，也做过本机使用验证。

Windows 现在只是能通过 GitHub Actions 编译出 `.exe` / `.msi` 安装包，还没有在真实 Windows 机器上完整验证 Provider 切换、Codex 配置写入、线程归并这些功能。所以 Windows 制品先当作实验性版本看待，不建议直接拿来放重要环境里用。

## 两个项目怎么选

如果你只想同步 Codex 线程，或者把本机不同 Provider 下的旧线程合并回来，用 `Codex Sync`。

如果你经常在官方 ChatGPT 登录和第三方 API Key Provider 之间切换，用 `Codex Tools`。它会保存不同 Provider 的配置，并在切换时写入 Codex 的 `config.toml` 和 `auth.json`。

## Codex Sync

`Codex Sync` 是 SwiftUI 写的 macOS App。

它现在主要做三件事：

- 通过 WebDAV 把本地 Codex 线程推到远端。
- 通过 WebDAV 从远端拉回 Codex 线程。
- 把本地线程里的 `model_provider` 合并成当前 Provider，避免旧会话因为 Provider 名变了而消失。

同步范围比较克制，只同步和线程相关的文件：

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

本地构建：

```bash
swift build -c release
./build_swiftui_app.sh
```

构建后的 App 在：

```text
dist-swiftui/Codex Sync.app
```

## Codex Tools

`Codex Tools` 是 Tauri + React + Rust 写的。macOS 已经实际验证过；Windows 目前能编译出安装包，但还没有做完整功能验证。

现在已经有这些功能：

- 管理多个 Codex Provider。
- 固定保留官方 `openai` Provider，用来对应 Codex 官方 ChatGPT 登录模式。
- 为第三方 API Key Provider 保存独立配置。
- 切换 Provider 时写入 `config.toml` 和 `auth.json`。
- 切换后自动合并本地线程 Provider。
- macOS 下常驻状态栏，可以从状态栏菜单直接切 Provider。

本地开发：

```bash
cd codex-tools
npm install
npm run tauri dev
```

打包：

```bash
npm run build
npm run tauri build
```

常见产物位置：

```text
codex-tools/src-tauri/target/release/bundle/macos/Codex Tools.app
codex-tools/src-tauri/target/release/bundle/dmg/*.dmg
codex-tools/src-tauri/target/release/bundle/nsis/*.exe
codex-tools/src-tauri/target/release/bundle/msi/*.msi
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

## 和 cc-switch、codex- 的关系

这个项目不是凭空冒出来的。

`Codex Tools` 的 Provider 管理、状态栏常驻、快速切换入口，参考了 `cc-switch` 的思路。`Codex Sync` 的线程同步、WebDAV 推拉、本地多 Provider 线程归并，参考了 `codex-` 方向上的探索。

这里不会假装这些想法都是自己发明的。这个仓库只是把这些需求重新收束到 Codex 这一个场景里，做成一个更顺手的小工具。

如果你需要 Claude Code / 多 CLI 的切换管理，可以去看 `cc-switch`。如果你关心 Codex 线程同步的早期方案，也建议看看 `codex-`。

## CI/CD

仓库里有两条 GitHub Actions：

- `CI`：每次 push / PR 跑 Swift、前端和 Rust 检查。
- `Release`：推送 `v*` tag 时自动构建 macOS / Windows 产物并发布 Release。这里的 Windows 目前只代表“能编译出包”，不代表已经完成真实环境验证。

发布一个新版本大概是这样：

```bash
git tag v0.1.0
git push origin v0.1.0
```

## 风险提示

这两个工具都会读写你的 Codex 配置或线程文件。虽然逻辑上会尽量克制，但第一次用之前，最好还是备份一下：

```text
~/.codex
```

尤其是你已经有很多长期会话、多个 Provider，或者正在多设备之间同步时，更建议先备份。

## 项目结构

```text
.
├── Package.swift
├── Sources/CodexSyncNative
├── build_swiftui_app.sh
└── codex-tools
    ├── package.json
    ├── src
    └── src-tauri
```

根目录是 `Codex Sync`，`codex-tools/` 是独立的 Tauri 项目。

仓库里还保留了一些早期 Python/Tkinter + PyInstaller 的探索脚本。现在标准入口以 SwiftUI 版 `Codex Sync` 和 Tauri 版 `Codex Tools` 为准。

## License

MIT License，见 [LICENSE](LICENSE)。
