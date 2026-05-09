# Codex Sync

Codex Sync 是一个 macOS 原生 SwiftUI App，用于同步和修复 Codex 本地线程。

## 功能

- 推送本地线程到 WebDAV。
- 拉取远端线程到本地。
- 将本地线程统一到当前 Codex Provider。
- 只同步线程文件，不同步 sqlite 数据库和 WAL/SHM 临时文件。

## 构建

在仓库根目录运行：

```bash
swift build -c release
```

打包为 macOS App：

```bash
./build_swiftui_app.sh
```

产物：

```text
dist-swiftui/Codex Sync.app
```

## 同步范围

会同步：

- `sessions/**/rollout-*.jsonl`
- `archived_sessions/**/rollout-*.jsonl`
- `session_index.jsonl`

不会同步：

- `state_5.sqlite`
- `logs_2.sqlite`
- `*.wal`
- `*.shm`

## 注意

Codex Sync 会修改本地线程的 `model_provider` 元数据。建议首次使用前备份 `~/.codex`。
