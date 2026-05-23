import React from "react";
import { CheckCircle2, RefreshCw } from "lucide-react";
import { getToolDefinition } from "./tool-registry";
import type { BridgeStatus, CodexProjectStatus, LogLine, UsageSummary, WebDavConfig } from "./app-types";

const wechatPermissionModes = [
  { value: "plan", label: "只读", desc: "只看和规划" },
  { value: "acceptEdits", label: "可编辑", desc: "自动改文件" },
  { value: "bypassPermissions", label: "完全自动", desc: "无需确认" },
];

export function ToolPageShell({
  toolId,
  onBack,
  children,
  actions,
  pageClassName,
  cardClassName = "",
}: {
  toolId: "usage" | "wechat" | "webdav" | "logs";
  onBack: () => void;
  children: React.ReactNode;
  actions?: React.ReactNode;
  pageClassName: string;
  cardClassName?: string;
}) {
  const tool = getToolDefinition(toolId);
  const Icon = tool.icon;
  return (
    <section className={pageClassName}>
      <div className={`detail-card glass ${cardClassName}`.trim()}>
        <button className="back-btn" onClick={onBack}>返回</button>
        <div className="tool-page-header">
          <div className="detail-title">
            <Icon size={34} />
            <div>
              <h2>{tool.detailTitle}</h2>
              <p>{tool.detailDescription}</p>
            </div>
          </div>
          {actions}
        </div>
        {children}
      </div>
    </section>
  );
}

export function UsagePage({
  usage,
  busy,
  onBack,
  onRefresh,
  formatUsd,
  formatToken,
}: {
  usage: UsageSummary;
  busy: boolean;
  onBack: () => void;
  onRefresh: () => void;
  formatUsd: (value: number) => string;
  formatToken: (value: number) => string;
}) {
  return (
    <ToolPageShell
      toolId="usage"
      pageClassName="usage-page"
      onBack={onBack}
      actions={(
        <div className="stat-pills">
          <span>文件 {usage.files_scanned}</span>
          <span>事件 {usage.usage_events}</span>
          <button disabled={busy} onClick={onRefresh}><RefreshCw size={15} />刷新</button>
        </div>
      )}
    >
      <div className="usage-summary-grid">
        <div className="usage-metric-card">
          <span>估算 Cost</span>
          <strong>{formatUsd(usage.total_cost_usd)}</strong>
        </div>
        <div className="usage-metric-card">
          <span>总 Token</span>
          <strong>{formatToken(usage.total.total_tokens)}</strong>
        </div>
        <div className="usage-metric-card">
          <span>输入 Token</span>
          <strong>{formatToken(usage.total.input_tokens)}</strong>
        </div>
        <div className="usage-metric-card">
          <span>缓存输入</span>
          <strong>{formatToken(usage.total.cached_input_tokens)}</strong>
        </div>
      </div>

      <div className="usage-table-card glass provider-usage-card">
        <div className="usage-table-title">Provider 汇总</div>
        <div className="usage-table-head provider-head">
          <span>Provider</span>
          <span>Cost</span>
          <span>总量</span>
          <span>输入</span>
          <span>缓存</span>
          <span>输出</span>
          <span>事件</span>
        </div>
        <div className="usage-table-body provider-body">
          {usage.providers.length === 0 ? <p className="empty">暂无 Provider 维度数据</p> : null}
          {usage.providers.map((provider) => (
            <div className="usage-row provider-row" key={provider.provider}>
              <span>{provider.provider}</span>
              <strong>{formatUsd(provider.cost_usd)}</strong>
              <span>{formatToken(provider.total_tokens)}</span>
              <span>{formatToken(provider.input_tokens)}</span>
              <span>{formatToken(provider.cached_input_tokens)}</span>
              <span>{formatToken(provider.output_tokens)}</span>
              <span>{provider.events}</span>
            </div>
          ))}
        </div>
      </div>

      <div className="usage-table-card glass">
        <div className="usage-table-title">每日用量</div>
        <div className="usage-table-head">
          <span>日期</span>
          <span>Cost</span>
          <span>总量</span>
          <span>输入</span>
          <span>缓存</span>
          <span>输出</span>
          <span>推理</span>
          <span>事件</span>
        </div>
        <div className="usage-table-body">
          {usage.days.length === 0 ? <p className="empty">暂无可统计的 Codex token_count 记录</p> : null}
          {usage.days.map((day) => (
            <div className="usage-row" key={day.date}>
              <span>{day.date}</span>
              <strong>{formatUsd(day.cost_usd)}</strong>
              <span>{formatToken(day.total_tokens)}</span>
              <span>{formatToken(day.input_tokens)}</span>
              <span>{formatToken(day.cached_input_tokens)}</span>
              <span>{formatToken(day.output_tokens)}</span>
              <span>{formatToken(day.reasoning_output_tokens)}</span>
              <span>{day.events}</span>
            </div>
          ))}
        </div>
      </div>
    </ToolPageShell>
  );
}

export function WechatPage({
  bridge,
  currentWorkDir,
  busy,
  onBack,
  selectCodexProject,
  permissionMode,
  changePermissionMode,
  refreshWechatQr,
  reloginWechat,
  startWechatConnection,
  stopWechatConnection,
}: {
  bridge: BridgeStatus;
  currentWorkDir: string;
  busy: boolean;
  onBack: () => void;
  selectCodexProject: (project: CodexProjectStatus) => Promise<void>;
  permissionMode: string;
  changePermissionMode: (permissionMode: string) => Promise<void>;
  refreshWechatQr: () => Promise<void>;
  reloginWechat: () => Promise<void>;
  startWechatConnection: () => Promise<void>;
  stopWechatConnection: () => Promise<void>;
}) {
  const statusText = bridge.service_running
    ? "已连接"
    : bridge.login_running
      ? "等待扫码确认"
      : bridge.has_logged_in_wechat_session
        ? "已登录微信"
        : bridge.qr_image_exists
          ? "等待完成登录"
          : "尚未连接";
  const currentProject = bridge.codex_projects.find((project) => project.work_dir === currentWorkDir)
    || bridge.projects.find((project) => project.work_dir === currentWorkDir);

  return (
    <ToolPageShell toolId="wechat" pageClassName="detail-page" cardClassName="wechat-flow-page" onBack={onBack}>
      <div className="wechat-flow-hero">
        <div>
          <div className="wechat-flow-label">微信连接</div>
          <h3>用一个项目目录，连上你的微信</h3>
          <p>先选择要让 Codex 工作的项目目录，再扫码登录。成功后直接启动微信连接。</p>
        </div>
        <div className="wechat-hero-side">
          <div className={`wechat-flow-badge ${bridge.service_running ? "success" : bridge.login_running ? "pending" : ""}`}>
            {bridge.service_running ? <CheckCircle2 size={16} /> : <RefreshCw size={16} />}
            <span>{statusText}</span>
          </div>
          <div className="wechat-current-project" title={currentWorkDir || "未选择项目"}>
            <span>当前项目</span>
            <strong>{currentProject?.name || "未选择"}</strong>
          </div>
        </div>
      </div>

      <div className="wechat-flow-grid">
        <div className="wechat-main-panel glass">
          <div className="wechat-section-head compact">
            <span>1</span>
            <div>
              <strong>微信登录</strong>
              <p>{bridge.login_running ? "请用微信扫码完成登录。" : bridge.has_logged_in_wechat_session ? "当前已经存在可复用的微信登录态，切换项目时不会重新出码。" : "首次使用时会自动生成二维码，并持续刷新状态。用微信扫一扫完成绑定。"}</p>
            </div>
          </div>

          {(bridge.login_running || bridge.qr_image_exists) ? (
            <div className="wechat-qr-block">
              <div className="wechat-qr-copy">
                <strong>{bridge.service_running ? "微信已连接" : bridge.login_running ? "请使用微信扫码" : "二维码已准备好"}</strong>
                <p>{bridge.service_running ? "扫码和登录已经完成，可以直接在微信里使用。" : "扫码完成后会自动启动微信连接。"}</p>
              </div>
              <div className="wechat-qr-frame">
                <img src={bridge.qr_image_data_url || ""} alt="微信扫码二维码" />
              </div>
              <div className="wechat-primary-actions">
                <button className="editor-action" disabled={busy} onClick={() => void refreshWechatQr()}>重新扫码</button>
                <button className="editor-action primary-action" disabled={busy || bridge.service_running || !bridge.has_logged_in_wechat_session} onClick={() => void startWechatConnection()}>连接微信</button>
                <button className="editor-action" disabled={busy || !bridge.service_running} onClick={() => void stopWechatConnection()}>停止连接</button>
              </div>
            </div>
          ) : bridge.has_logged_in_wechat_session ? (
            <div className="wechat-empty-state">
              {bridge.service_running
                ? "已检测到可复用的微信登录态，切换项目不会要求重新扫码。"
                : "微信仍然处于已登录状态，停止连接后不需要重新扫码，可以直接重连。"}
            </div>
          ) : (
            <div className="wechat-empty-state">还没有二维码。首次选择项目后，系统会自动生成。</div>
          )}
        </div>

        <div className="wechat-side-panel glass">
          <div className="wechat-side-card">
            <strong>选择项目</strong>
            {bridge.codex_projects.length > 0 ? (
              <div className="wechat-project-list">
                {bridge.codex_projects.map((project) => {
                  const active = currentWorkDir === project.work_dir;
                  return (
                    <button
                      key={`${project.name}-${project.work_dir}`}
                      className={`wechat-project-row ${active ? "active" : ""}`}
                      disabled={!!busy}
                      onClick={() => void selectCodexProject(project)}
                    >
                      <span>{project.name}</span>
                      <small>{active ? "当前使用中" : "点击切换"}</small>
                    </button>
                  );
                })}
              </div>
            ) : (
              <div className="wechat-empty-state">还没有读取到 Codex 项目。</div>
            )}
          </div>
          <div className="wechat-side-card">
            <strong>权限模式</strong>
            <div className="wechat-permission-list">
              {wechatPermissionModes.map((mode) => {
                const active = permissionMode === mode.value;
                return (
                  <button
                    key={mode.value}
                    className={`wechat-permission-option ${active ? "active" : ""}`}
                    disabled={busy || !currentWorkDir}
                    onClick={() => void changePermissionMode(mode.value)}
                  >
                    <span>{mode.label}</span>
                    <small>{mode.desc}</small>
                  </button>
                );
              })}
            </div>
          </div>
        </div>
      </div>
    </ToolPageShell>
  );
}

export function WebdavPage({
  webdav,
  busy,
  onBack,
  onSave,
  setWebdav,
}: {
  webdav: WebDavConfig;
  busy: boolean;
  onBack: () => void;
  onSave: () => Promise<void>;
  setWebdav: React.Dispatch<React.SetStateAction<WebDavConfig>>;
}) {
  return (
    <ToolPageShell toolId="webdav" pageClassName="detail-page" cardClassName="compact-card" onBack={onBack}>
      <div className="form-grid">
        <label>服务地址<input value={webdav.base_url} onChange={(event) => setWebdav({ ...webdav, base_url: event.target.value })} /></label>
        <label>用户名<input value={webdav.username} onChange={(event) => setWebdav({ ...webdav, username: event.target.value })} /></label>
        <label>密码<input type="password" value={webdav.password} onChange={(event) => setWebdav({ ...webdav, password: event.target.value })} /></label>
        <label className="checkbox-row"><input type="checkbox" checked={webdav.verify_tls} onChange={(event) => setWebdav({ ...webdav, verify_tls: event.target.checked })} />校验证书 TLS</label>
      </div>
      <div className="detail-actions"><button className="editor-action primary-action" disabled={busy} onClick={() => void onSave()}>保存 WebDAV 配置</button></div>
    </ToolPageShell>
  );
}

export function LogsPage({
  logs,
  onBack,
}: {
  logs: LogLine[];
  onBack: () => void;
}) {
  return (
    <ToolPageShell toolId="logs" pageClassName="detail-page" cardClassName="compact-card" onBack={onBack}>
      <div className="log-list standalone">
        {logs.length === 0 ? <p className="empty">暂无日志</p> : null}
        {logs.map((line) => <div className="log-line" key={line.id}>{line.message}</div>)}
      </div>
    </ToolPageShell>
  );
}
