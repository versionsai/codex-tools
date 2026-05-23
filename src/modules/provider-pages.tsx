import React from "react";
import { CheckCircle2, GitBranch, Loader2, Play, RefreshCw, Save, Shuffle, Trash2 } from "lucide-react";
import type { ProviderConfig, Summary } from "./app-types";

export function ProviderDashboardPage({
  summary,
  providers,
  busy,
  toolCards,
  refreshAll,
  switchProvider,
  selectProvider,
  deleteProvider,
  setView,
}: {
  summary: Summary;
  providers: ProviderConfig[];
  busy: boolean;
  toolCards: React.ReactNode;
  refreshAll: () => Promise<void>;
  switchProvider: (providerId: string) => Promise<void>;
  selectProvider: (provider: ProviderConfig) => Promise<void>;
  deleteProvider: (providerId: string) => Promise<void>;
  setView: (view: "usage" | "wechat" | "webdav" | "logs") => void;
}) {
  return (
    <section className="provider-board">
      <div className="board-meta">
        <div>
          <h2>Codex Tools 工作台</h2>
          <p>一个围绕 Codex 的工具壳：上层统一入口，下层按模块扩展。</p>
        </div>
        <div className="stat-pills">
          <span>当前 {summary.provider}</span>
          <span>配置 {providers.length}</span>
          <span>活跃 {summary.active_sessions}</span>
          <span>归档 {summary.archived_sessions}</span>
          <button disabled={busy} onClick={() => void refreshAll()}><RefreshCw size={15} />刷新</button>
        </div>
      </div>

      {toolCards}

      <div className="provider-list-board">
        {providers.map((provider) => (
          <div className={`provider-list-card ${provider.id === summary.provider ? "current" : ""}`} key={provider.id}>
            <div className="drag-grip">••<br />••</div>
            <div className="provider-avatar">{provider.id.slice(0, 1).toUpperCase()}</div>
            <div className="provider-card-main">
              <strong>{provider.id}</strong>
              <span>{provider.id === "openai" ? "官方 ChatGPT 登录模式" : providerSummary(provider)}</span>
            </div>
            <div className="provider-card-actions">
              {provider.id === summary.provider ? (
                <span className="session-pill"><CheckCircle2 size={16} />使用中</span>
              ) : (
                <button className="enable-btn" disabled={busy} onClick={() => void switchProvider(provider.id)}>
                  {busy ? <Loader2 className="spin" size={16} /> : <Play size={16} />}
                  启用
                </button>
              )}
              <button className="icon-action" disabled={busy} onClick={() => void selectProvider(provider)}>编辑</button>
              <button className="icon-action danger-text" disabled={busy || provider.id === summary.provider || provider.id === "openai"} onClick={() => void deleteProvider(provider.id)}>删除</button>
            </div>
          </div>
        ))}
      </div>
    </section>
  );
}

export function ProviderFormPage({
  providerDraft,
  editingProviderId,
  providers,
  currentProviderId,
  busy,
  modelOptions,
  onBack,
  setProviderDraft,
  fetchModels,
  saveProvider,
  switchProvider,
  deleteProvider,
}: {
  providerDraft: ProviderConfig;
  editingProviderId: string | null;
  providers: ProviderConfig[];
  currentProviderId: string;
  busy: string | null;
  modelOptions: string[];
  onBack: () => void;
  setProviderDraft: React.Dispatch<React.SetStateAction<ProviderConfig>>;
  fetchModels: () => Promise<void>;
  saveProvider: () => Promise<void>;
  switchProvider: (providerId: string) => Promise<void>;
  deleteProvider: (providerId?: string) => Promise<void>;
}) {
  const isBuiltinOpenAI = providerDraft.id === "openai";
  const providerCanSwitch = Boolean(providerDraft.id && providerDraft.id !== currentProviderId);
  const providerCanDelete = Boolean(
    providerDraft.id &&
    providerDraft.id !== "openai" &&
    providerDraft.id !== currentProviderId &&
    providers.some((item) => item.id === providerDraft.id)
  );
  const duplicateProviderId = Boolean(
    providerDraft.id.trim() &&
    providers.some((provider) => provider.id === providerDraft.id.trim() && provider.id !== editingProviderId)
  );

  return (
    <section className="detail-page">
      <div className="detail-card glass">
        <button className="back-btn" onClick={onBack}>返回</button>
        <div className="detail-title">
          <GitBranch size={34} />
          <div>
            <h2>{providerDraft.id ? "编辑 Codex Provider" : "新建 Codex Provider"}</h2>
            <p>写入 Codex 的 config.toml，可用于切换 Provider 并同步历史线程。</p>
          </div>
        </div>
        <div className="form-grid compact">
          <label className={duplicateProviderId ? "field-error" : ""}>Provider ID<input value={providerDraft.id ?? ""} placeholder="如 openai / custom / deepseek" onChange={(event) => setProviderDraft({ ...providerDraft, id: event.target.value })} />{duplicateProviderId ? <span className="field-hint">这个 Provider ID 已存在，请保持唯一。</span> : null}</label>
          <label>
            认证方式
            <select value={providerDraft.auth_type ?? (providerDraft.id === "openai" ? "chatgpt" : "api_key")} disabled={isBuiltinOpenAI} onChange={(event) => setProviderDraft({ ...providerDraft, auth_type: event.target.value })}>
              <option value="chatgpt">官方 ChatGPT 登录</option>
              <option value="api_key">API Key</option>
            </select>
          </label>
          {!isBuiltinOpenAI ? <label>Base URL<input value={providerDraft.base_url ?? ""} placeholder="https://api.example.com/v1" onChange={(event) => setProviderDraft({ ...providerDraft, base_url: event.target.value })} /></label> : null}
          {!isBuiltinOpenAI ? <label>API Key<input type="password" value={providerDraft.api_key ?? ""} placeholder="sk-..." onChange={(event) => setProviderDraft({ ...providerDraft, api_key: event.target.value })} /></label> : null}
          <label>
            模型
            <div className="select-with-action">
              <select value={providerDraft.model ?? ""} onChange={(event) => setProviderDraft({ ...providerDraft, model: event.target.value })}>
                {uniqueOptions(providerDraft.model, modelOptions).map((model) => <option value={model} key={model}>{model}</option>)}
              </select>
              {!isBuiltinOpenAI ? <button type="button" disabled={!!busy} onClick={() => void fetchModels()}>{busy === "获取模型列表" ? "获取中" : "获取模型"}</button> : null}
            </div>
          </label>
          <label>
            推理强度
            <select value={providerDraft.model_reasoning_effort ?? "medium"} onChange={(event) => setProviderDraft({ ...providerDraft, model_reasoning_effort: event.target.value })}>
              <option value="minimal">minimal</option>
              <option value="low">low</option>
              <option value="medium">medium</option>
              <option value="high">high</option>
              <option value="xhigh">xhigh</option>
            </select>
          </label>
          {!isBuiltinOpenAI ? (
            <label>
              Wire API
              <select value={providerDraft.wire_api ?? "responses"} onChange={(event) => setProviderDraft({ ...providerDraft, wire_api: event.target.value })}>
                <option value="responses">responses</option>
                <option value="chat">chat</option>
              </select>
            </label>
          ) : null}
        </div>
        <div className="detail-actions">
          <button className="editor-action primary-action" disabled={!!busy || !providerDraft.id || duplicateProviderId} onClick={() => void saveProvider()}><Save size={16} />保存 Provider</button>
          <button className="editor-action" disabled={!!busy || !providerCanSwitch} onClick={() => void switchProvider(providerDraft.id)}><Shuffle size={16} />切换并合并</button>
          <button className="editor-action danger-action" disabled={!!busy || !providerCanDelete} onClick={() => void deleteProvider()}><Trash2 size={16} />删除</button>
        </div>
      </div>
    </section>
  );
}

function providerSummary(provider?: ProviderConfig) {
  if (!provider) return "Codex 默认 Provider";
  return provider.base_url || provider.wire_api || "Codex 默认 Provider";
}

function uniqueOptions(current: string | undefined, options: string[]) {
  return Array.from(new Set([current || "gpt-5.4", ...options].filter(Boolean)));
}
