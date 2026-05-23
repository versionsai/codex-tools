# 更新日志

所有重要变更都会记录在这个文件里。

格式参考 [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)。已发布版本使用对应的 Git tag 标题，例如 `v0.1.6`；尚未发布的变更先写在 `未发布` 下，发版时再移动到新的 tag 区块。

## v0.1.10 - 2026-05-23

### 新增

- 新增“微信连接”实验页，参考 `cc-connect` 的 Weixin ilink 连接、项目配置和 Codex agent 运行逻辑，内置微信连接引擎，支持读取 Codex 已配置项目、扫码登录微信、自动启动微信服务并展示通信会话状态。
- 新增微信连接权限模式选择：只读、可编辑、完全自动，分别写入当前项目的 Codex agent 配置，方便通过微信控制 Codex 是否允许编辑文件。

### 变更

- 首页进一步收口为“Codex Tools 工作台”：先展示当前已接入的工具模块，再展示 Provider 列表，让产品形态更明确地朝“可扩展工具壳”对齐。
- 工作台模块卡片补充各自状态摘要：用量页展示预估 Cost 与事件数，微信连接展示连接安装/项目状态，WebDAV 展示是否已配置，日志页展示最近日志摘要。
- 工具模块的入口定义与首页摘要构造进一步集中到 `tool-registry`，减少主页面散落的模块判断，方便后续继续接新工具。
- 工具详情页的标题、说明和公共外壳也统一收进模块注册体系，主页面只负责根据模块定义渲染对应页头与容器。
- 将用量统计、微信连接、WebDAV、日志页拆到独立模块文件，`main.tsx` 主要保留状态装配、动作和视图切换，方便继续扩展更多工具页。
- 为工具模块补充显式能力元数据（是否展示在工作台、是否有详情页、是否带配置、模块类别），首页和工具栏开始消费这套定义，减少对隐式规则的依赖。
- README 更新为工具链壳子的定位说明，明确当前内置模块和后续可扩展方向，避免文档仍停留在“仅 Provider 管理 + 线程同步”的旧口径。
- 统一微信模块的对外文案，尽量使用“微信连接 / 微信服务 / 登录微信”这类用户可理解的表述，减少 `bridge` / `daemon` 这类开发者术语直接暴露。
- 微信连接数据目录迁移到 Codex Tools 自己的 `wechatbot` 目录，不再读取或写入 `~/.cc-connect`。
- 微信连接项目改为只保留当前选择的一个项目，点击 Codex 项目即自动保存切换，避免历史项目干扰微信和 Codex 的通信。
- 微信连接进程启动时补齐常见 CLI 路径，解决 macOS 图形应用环境里找不到 `codex` 命令导致扫码后无响应的问题。
- 微信连接 UI 调整为“顶部状态 + 登录卡片 + 项目/权限侧栏”的结构；顶部只保留一个聚合状态，登录卡片将二维码和操作按钮分行展示，并在顶部展示当前选择项目。
- 微信连接状态以配置文件为权威来源，页面打开和窗口聚焦时会重新读取配置；用户手动修改权限模式后也能正确回显。

## v0.1.11 - 2026-05-23

### 变更

- Provider 切换后会自动重启本机 Codex 桌面应用，避免 Codex Tools 已切换成功但 Codex 主程序仍停留在旧 Provider 的状态。
- 主窗口内切换 Provider 时，在写入 Codex 配置并统一线程 Provider 后，会继续触发 Codex 重启，让切换动作形成闭环。
- macOS 状态栏菜单切换 Provider 时，同样会自动执行 Codex 重启，保证状态栏入口与主窗口行为一致。
- Codex 桌面应用查找逻辑改为按平台查找常见安装位置：macOS 支持 `/Applications/Codex.app` 与 `~/Applications/Codex.app`，Windows 支持常见的 `Codex.exe` 安装目录。
- 当前版本的平台目标进一步明确为 macOS 与 Windows；Linux 暂未实现 Provider 切换后的 Codex 自动重启。

### 验证

- `cargo check`
- `npm run build`
- `npm run tauri build`

## v0.1.9 - 2026-05-18

### 新增

- 新增 Codex 用量统计页，离线扫描本地 `sessions` JSONL 中的 `token_count` 事件，按日期和 Provider 汇总输入、缓存输入、输出、推理输出、总 token，并按 OpenAI 官方价格估算 Cost；统计口径对齐 `ccusage codex daily --offline`。

## v0.1.8 - 2026-05-15

### 变更

- macOS Release zip 新增 `install.command`，支持双击安装 `Codex Tools.app` 到 `/Applications` 并自动移除 quarantine 属性。
- 更新 README，说明 macOS 未公证版本的 `install.command` 安装流程。

## v0.1.7 - 2026-05-11

### 变更

- 将 `Windows Build` 工作流改为仅支持手动触发，避免每次推送 `main` 时重复打 Windows 包；正式发布包只由 tag 触发的 `Release` 工作流生成。
- 修复通过 macOS 状态栏快捷切换 Provider 后，重新打开主窗口时界面状态没有自动刷新的问题。

## v0.1.6 - 2026-05-11

### 变更

- 将 Codex Tools 从 `codex-tools/` 子目录提升到仓库根目录，根目录现在就是 Tauri 项目。
- 移除独立的 Codex Sync SwiftUI 应用和早期 Python/Tkinter 原型，仓库只保留 Codex Tools。
- 更新 GitHub Actions，让 CI、Release 和 Windows Build 都从仓库根目录构建。
- 更新根目录 `README.md` 和 `.gitignore`，匹配新的单项目结构。
- 修复 macOS 下应用启动和重复点击时窗口不显示的问题。
- 优化 WebDAV 推送：缓存已确认的远端目录，避免重复 `MKCOL`；移除推送结束时仅用于展示统计的第二次远端全量扫描。
- 新增 `CHANGELOG.md`，后续重要变更按版本记录。
- 将项目版本提升到 `0.1.6`，对应 Git tag `v0.1.6`。

### 移除

- 移除 `Package.swift`。
- 移除 `Sources/CodexSyncNative/`。
- 移除 `build_swiftui_app.sh`。
- 移除 `build_mac_app.sh`。
- 移除 `codex_sync_gui.py`。
- 移除 `docs/codex-sync.md`。
- 移除 `generate_app_icon.py`。

### 验证

- `npm ci && npm run build`
- `cargo check`
- `npm run tauri build -- --bundles app`

## v0.1.5

### 说明

- 对应 Git tag `v0.1.5`。
- 该版本仍使用旧的 `codex-tools/` 子目录结构，并同时保留 Codex Sync 相关文件。
