import SwiftUI

struct ContentView: View {
    @EnvironmentObject private var model: AppViewModel

    var body: some View {
        ZStack {
            LinearGradient(
                colors: [Color(nsColor: .windowBackgroundColor), Color(red: 0.94, green: 0.96, blue: 1.0)],
                startPoint: .topLeading,
                endPoint: .bottomTrailing
            )
            .ignoresSafeArea()

            VStack(spacing: 0) {
                toolbar
                VStack(spacing: 18) {
                    hero
                    actionsRow
                    statsRow
                    contentGrid
                }
                .padding(24)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            }
        }
        .alert("发生错误", isPresented: Binding(get: {
            model.lastError != nil
        }, set: { newValue in
            if !newValue {
                model.lastError = nil
            }
        })) {
            Button("知道了", role: .cancel) {}
        } message: {
            Text(model.lastError ?? "")
        }
    }

    private var toolbar: some View {
        HStack(spacing: 14) {
            Label {
                Text("Codex Sync")
                    .font(.system(size: 18, weight: .semibold))
            } icon: {
                Image(systemName: "shippingbox.fill")
                    .font(.system(size: 18, weight: .semibold))
                    .foregroundStyle(.white)
                    .frame(width: 30, height: 30)
                    .background(
                        RoundedRectangle(cornerRadius: 10, style: .continuous)
                            .fill(
                                LinearGradient(colors: [Color.accentColor, Color.blue.opacity(0.75)], startPoint: .topLeading, endPoint: .bottomTrailing)
                            )
                    )
            }

            Rectangle()
                .fill(Color.black.opacity(0.08))
                .frame(width: 1, height: 22)

            Text("安全、高效、可靠的线程与 Provider 同步管理平台")
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(.secondary)

            Spacer()

            Button {
                Task { await model.push() }
            } label: {
                Label("开始同步", systemImage: "play.fill")
                    .font(.system(size: 13, weight: .semibold))
            }
            .buttonStyle(PrimaryGlassButtonStyle())
            .disabled(model.isBusy)

            Button {
                Task { await model.refreshSummary() }
            } label: {
                Image(systemName: "bell")
                    .font(.system(size: 14, weight: .semibold))
                    .frame(width: 34, height: 34)
            }
            .buttonStyle(GlassIconButtonStyle())
            .disabled(model.isBusy)
        }
        .padding(.horizontal, 24)
        .padding(.vertical, 14)
        .background(.ultraThinMaterial)
        .overlay(alignment: .bottom) {
            Rectangle()
                .fill(Color.black.opacity(0.06))
                .frame(height: 1)
        }
    }

    private var hero: some View {
        HStack(alignment: .top, spacing: 18) {
            VStack(alignment: .leading, spacing: 10) {
                Text("概览")
                    .font(.system(size: 34, weight: .bold))
                Text("多设备线程同步与本地 Provider 整理")
                    .font(.system(size: 14, weight: .medium))
                    .foregroundStyle(.secondary)
            }

            Spacer()

            HStack(spacing: 12) {
                compactMetric("Provider", value: model.provider, tint: .blue)
                compactMetric("活跃", value: "\(model.activeSessions)", tint: .green)
                compactMetric("归档", value: "\(model.archivedSessions)", tint: .purple)
            }

            StatusCapsule(status: model.status)
        }
    }

    private var actionsRow: some View {
        HStack(spacing: 16) {
            ActionCard(title: "拉取远端线程", subtitle: "从 WebDAV 同步到本地", tint: .blue, icon: "arrow.down.circle.fill") {
                Task { await model.pull() }
            }
            .disabled(model.isBusy)

            ActionCard(title: "推送本地线程", subtitle: "把当前设备会话更新到远端", tint: .green, icon: "arrow.up.circle.fill") {
                Task { await model.push() }
            }
            .disabled(model.isBusy)

            ActionCard(title: "合并 Provider 线程", subtitle: "统一本地历史线程的 Provider", tint: .purple, icon: "arrow.triangle.branch") {
                Task { await model.unifyProvider() }
            }
            .disabled(model.isBusy)
        }
    }

    private var statsRow: some View {
        HStack(spacing: 16) {
            StatCard(title: "当前 Provider", value: model.provider, tint: .blue, symbol: "person.crop.circle.fill")
            StatCard(title: "活跃会话", value: "\(model.activeSessions)", tint: .teal, symbol: "message.fill")
            StatCard(title: "归档会话", value: "\(model.archivedSessions)", tint: .indigo, symbol: "archivebox.fill")
        }
    }

    private var contentGrid: some View {
        HStack(alignment: .top, spacing: 18) {
            configPanel
            logPanel
        }
        .frame(maxHeight: .infinity, alignment: .top)
    }

    private var configPanel: some View {
        GlassPanel(title: "配置与操作", symbol: "gearshape.fill") {
            VStack(alignment: .leading, spacing: 18) {
                HStack {
                    VStack(alignment: .leading, spacing: 6) {
                        Text("WebDAV 配置")
                            .font(.system(size: 20, weight: .semibold))
                        Text("连接信息保存在本机配置中。")
                            .font(.system(size: 12, weight: .medium))
                            .foregroundStyle(.secondary)
                    }
                    Spacer()
                    Text("已连接")
                        .font(.system(size: 12, weight: .semibold))
                        .foregroundStyle(Color.green)
                        .padding(.horizontal, 12)
                        .padding(.vertical, 8)
                        .background(Color.green.opacity(0.10), in: Capsule())
                }

                Grid(alignment: .leading, horizontalSpacing: 14, verticalSpacing: 12) {
                    GridRow {
                        FormField(title: "服务地址", text: $model.config.baseURL)
                        FormField(title: "密码", text: $model.config.password, secure: true)
                    }
                    GridRow {
                        FormField(title: "用户名", text: $model.config.username)
                        FormField(title: "Codex 目录", text: $model.config.codexDirectory)
                    }
                }

                Toggle(isOn: $model.config.verifyTLS) {
                    Text("校验证书 TLS")
                        .font(.system(size: 13, weight: .medium))
                }
                .toggleStyle(.checkbox)

                HStack(spacing: 12) {
                    Button {
                        Task { await model.saveConfig() }
                    } label: {
                        Label("保存配置", systemImage: "square.and.arrow.down")
                            .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(PrimaryGlassButtonStyle())
                    .disabled(model.isBusy)

                    Button {
                        Task { await model.refreshSummary() }
                    } label: {
                        Label("刷新状态", systemImage: "arrow.clockwise")
                            .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(SecondaryGlassButtonStyle())
                    .disabled(model.isBusy)
                }
                Spacer(minLength: 0)
            }
        }
        .frame(maxHeight: .infinity)
    }

    private var logPanel: some View {
        GlassPanel(title: "运行日志", symbol: "waveform.path.ecg") {
            VStack(alignment: .leading, spacing: 14) {
                HStack {
                    Text("最近日志")
                        .font(.system(size: 20, weight: .semibold))
                    Spacer()
                    Button("清空日志") {
                        model.clearLogs()
                    }
                    .buttonStyle(SecondaryGlassButtonStyle(compact: true))
                }

                ScrollView {
                    VStack(alignment: .leading, spacing: 10) {
                        ForEach(model.logs.reversed()) { line in
                            VStack(alignment: .leading, spacing: 4) {
                                Text(line.timestamp.formatted(date: .omitted, time: .standard))
                                    .font(.system(size: 11, weight: .semibold, design: .monospaced))
                                    .foregroundStyle(.secondary)
                                Text(line.message)
                                    .font(.system(size: 13, weight: .medium, design: .monospaced))
                                    .textSelection(.enabled)
                            }
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(12)
                            .background(Color.white.opacity(0.72), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
                        }

                        if model.logs.isEmpty {
                            Text("暂无日志")
                                .font(.system(size: 13, weight: .medium))
                                .foregroundStyle(.secondary)
                                .frame(maxWidth: .infinity, minHeight: 180)
                        }
                    }
                    .padding(4)
                }
                .frame(maxHeight: .infinity)
                .background(Color.white.opacity(0.34), in: RoundedRectangle(cornerRadius: 18, style: .continuous))
            }
            .frame(maxHeight: .infinity)
        }
        .frame(maxHeight: .infinity)
    }

    private func compactMetric(_ title: String, value: String, tint: Color) -> some View {
        HStack(spacing: 8) {
            Circle()
                .fill(tint)
                .frame(width: 8, height: 8)
            Text(title)
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(.secondary)
            Text(value)
                .font(.system(size: 13, weight: .semibold))
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .background(.thinMaterial, in: Capsule())
    }
}

private struct ActionCard: View {
    let title: String
    let subtitle: String
    let tint: Color
    let icon: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 16) {
                Image(systemName: icon)
                    .font(.system(size: 24, weight: .semibold))
                    .foregroundStyle(tint)
                    .frame(width: 46, height: 46)
                    .background(tint.opacity(0.12), in: RoundedRectangle(cornerRadius: 16, style: .continuous))

                VStack(alignment: .leading, spacing: 6) {
                    Text(title)
                        .font(.system(size: 18, weight: .semibold))
                        .foregroundStyle(.primary)
                    Text(subtitle)
                        .font(.system(size: 12, weight: .medium))
                        .foregroundStyle(.secondary)
                }

                Spacer()

                Image(systemName: "chevron.right")
                    .font(.system(size: 15, weight: .bold))
                    .foregroundStyle(tint)
            }
            .padding(18)
            .frame(maxWidth: .infinity)
            .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 24, style: .continuous))
            .overlay {
                RoundedRectangle(cornerRadius: 24, style: .continuous)
                    .stroke(Color.white.opacity(0.70), lineWidth: 1)
            }
        }
        .buttonStyle(.plain)
    }
}

private struct StatCard: View {
    let title: String
    let value: String
    let tint: Color
    let symbol: String

    var body: some View {
        HStack(spacing: 14) {
            Image(systemName: symbol)
                .font(.system(size: 24, weight: .semibold))
                .foregroundStyle(tint)
                .frame(width: 48, height: 48)
                .background(tint.opacity(0.12), in: RoundedRectangle(cornerRadius: 16, style: .continuous))

            VStack(alignment: .leading, spacing: 6) {
                Text(title)
                    .font(.system(size: 13, weight: .medium))
                    .foregroundStyle(.secondary)
                Text(value)
                    .font(.system(size: 21, weight: .bold))
            }
            Spacer()
        }
        .padding(18)
        .frame(maxWidth: .infinity)
        .background(.thinMaterial, in: RoundedRectangle(cornerRadius: 22, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: 22, style: .continuous)
                .stroke(Color.white.opacity(0.75), lineWidth: 1)
        }
    }
}

private struct GlassPanel<Content: View>: View {
    let title: String
    let symbol: String
    @ViewBuilder var content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            Label(title, systemImage: symbol)
                .font(.system(size: 15, weight: .semibold))
                .foregroundStyle(.primary)
            content
        }
        .padding(20)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 28, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: 28, style: .continuous)
                .stroke(Color.white.opacity(0.80), lineWidth: 1)
        }
    }
}

private struct FormField: View {
    let title: String
    @Binding var text: String
    var secure: Bool = false

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title)
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(.secondary)
            Group {
                if secure {
                    SecureField("", text: $text)
                } else {
                    TextField("", text: $text)
                }
            }
            .textFieldStyle(.plain)
            .font(.system(size: 14, weight: .medium))
            .padding(.horizontal, 14)
            .padding(.vertical, 11)
            .background(Color.white.opacity(0.72), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
            .overlay {
                RoundedRectangle(cornerRadius: 14, style: .continuous)
                    .stroke(Color.black.opacity(0.06), lineWidth: 1)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

private struct StatusCapsule: View {
    let status: StatusBadge

    var body: some View {
        Text(status.rawValue)
            .font(.system(size: 13, weight: .semibold))
            .foregroundStyle(statusColor)
            .padding(.horizontal, 14)
            .padding(.vertical, 10)
            .background(statusColor.opacity(0.12), in: Capsule())
    }

    private var statusColor: Color {
        switch status {
        case .failed:
            return .red
        case .success:
            return .green
        case .pulling, .pushing, .merging, .saving:
            return .blue
        case .ready:
            return .secondary
        }
    }
}

private struct PrimaryGlassButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 13, weight: .semibold))
            .foregroundStyle(.white)
            .padding(.horizontal, 18)
            .padding(.vertical, 10)
            .background(
                LinearGradient(colors: [Color.blue, Color.blue.opacity(0.72)], startPoint: .topLeading, endPoint: .bottomTrailing),
                in: RoundedRectangle(cornerRadius: 14, style: .continuous)
            )
            .overlay {
                RoundedRectangle(cornerRadius: 14, style: .continuous)
                    .stroke(Color.white.opacity(configuration.isPressed ? 0.25 : 0.45), lineWidth: 1)
            }
            .scaleEffect(configuration.isPressed ? 0.985 : 1.0)
    }
}

private struct SecondaryGlassButtonStyle: ButtonStyle {
    var compact: Bool = false

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: compact ? 12 : 13, weight: .semibold))
            .foregroundStyle(.primary)
            .padding(.horizontal, compact ? 12 : 18)
            .padding(.vertical, compact ? 8 : 10)
            .background(.thinMaterial, in: RoundedRectangle(cornerRadius: compact ? 12 : 14, style: .continuous))
            .overlay {
                RoundedRectangle(cornerRadius: compact ? 12 : 14, style: .continuous)
                    .stroke(Color.white.opacity(configuration.isPressed ? 0.45 : 0.75), lineWidth: 1)
            }
            .scaleEffect(configuration.isPressed ? 0.99 : 1.0)
    }
}

private struct GlassIconButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .background(.thinMaterial, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
            .overlay {
                RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .stroke(Color.white.opacity(configuration.isPressed ? 0.35 : 0.75), lineWidth: 1)
            }
            .scaleEffect(configuration.isPressed ? 0.98 : 1.0)
    }
}
