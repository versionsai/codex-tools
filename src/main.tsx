import React, { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import {
  Archive,
  CheckCircle2,
  CloudDownload,
  CloudUpload,
  Cpu,
  GitBranch,
  Loader2,
  Play,
  Plus,
  RefreshCw,
  Save,
  Settings2,
  Shuffle,
  TerminalSquare,
  Trash2,
} from "lucide-react";
import "./styles.css";

type Summary = {
  provider: string;
  active_sessions: number;
  archived_sessions: number;
  codex_dir: string;
};

type ProviderConfig = {
  id: string;
  name?: string;
  auth_type?: string;
  base_url?: string;
  api_key?: string;
  wire_api?: string;
  model?: string;
  model_reasoning_effort?: string;
  requires_openai_auth?: boolean;
};

type WebDavConfig = {
  base_url: string;
  username: string;
  password: string;
  verify_tls: boolean;
};

type LogLine = {
  id: number;
  message: string;
};

type ToastState = {
  id: number;
  message: string;
  variant: "info" | "success" | "error";
};

type View = "providers" | "provider-form" | "webdav" | "logs";
type ModelOption = { id: string };

const defaultSummary: Summary = {
  provider: "--",
  active_sessions: 0,
  archived_sessions: 0,
  codex_dir: "",
};

const emptyProvider: ProviderConfig = {
  id: "",
  name: "",
  auth_type: "api_key",
  base_url: "",
  api_key: "",
  wire_api: "responses",
  model: "gpt-5.4",
  model_reasoning_effort: "high",
  requires_openai_auth: true,
};

const builtinOpenAIProvider: ProviderConfig = {
  id: "openai",
  name: "Codex 默认 Provider",
  auth_type: "chatgpt",
  base_url: "https://api.openai.com/v1",
  api_key: "",
  wire_api: "responses",
  model: "gpt-5.4",
  model_reasoning_effort: "medium",
  requires_openai_auth: false,
};

function App() {
  const [summary, setSummary] = useState<Summary>(defaultSummary);
  const [providers, setProviders] = useState<ProviderConfig[]>([]);
  const [providerDraft, setProviderDraft] = useState<ProviderConfig>(emptyProvider);
  const [view, setView] = useState<View>("providers");
  const [webdav, setWebdav] = useState<WebDavConfig>({
    base_url: "",
    username: "",
    password: "",
    verify_tls: true,
  });
  const [busy, setBusy] = useState<string | null>(null);
  const [logs, setLogs] = useState<LogLine[]>([]);
  const [modelOptions, setModelOptions] = useState<string[]>([]);
  const [toast, setToast] = useState<ToastState | null>(null);
  const [editingProviderId, setEditingProviderId] = useState<string | null>(null);

  useEffect(() => {
    void bootstrap();
  }, []);

  async function bootstrap() {
    await refreshSummary();
    await refreshProviders();
    await loadWebdav();
  }

  async function refreshAll() {
    await run("刷新状态", async () => {
      await bootstrap();
      return "状态已刷新";
    });
  }

  async function refreshSummary() {
    try {
      const next = await invoke<Summary>("get_summary");
      setSummary(next);
    } catch (error) {
      appendLog(`读取 Codex 状态失败：${String(error)}`);
      setSummary((current) => ({ ...current, provider: "openai" }));
    }
  }

  async function refreshProviders() {
    let next: ProviderConfig[];
    try {
      next = ensureBuiltinOpenAI(await invoke<ProviderConfig[]>("list_providers"));
    } catch (error) {
      appendLog(`读取 Provider 列表失败：${String(error)}`);
      next = [builtinOpenAIProvider];
    }
    setProviders(next);
    setProviderDraft((current) => {
      if (current.id) return current;
      return toProviderDraft(next.find((item) => item.id === summary.provider) ?? next[0] ?? emptyProvider);
    });
  }

  async function loadWebdav() {
    try {
      const next = await invoke<WebDavConfig>("load_webdav_config");
      setWebdav(next);
    } catch (error) {
      appendLog(`读取 WebDAV 配置失败：${String(error)}`);
    }
  }

  async function run(label: string, action: () => Promise<string>) {
    if (busy) return;
    setBusy(label);
    appendLog(`开始：${label}`);
    showToast(`正在${label}...`, "info", 1600);
    try {
      const result = await action();
      const message = result.trim() || `${label}已完成`;
      appendLog(message);
      showToast(message, "success");
      await refreshSummary();
      if (label.includes("Provider")) {
        await refreshProviders();
      }
    } catch (error) {
      const message = `失败：${String(error)}`;
      appendLog(message);
      showToast(message, "error", 5200);
    } finally {
      setBusy(null);
    }
  }

  function appendLog(message: string) {
    setLogs((items) => [{ id: Date.now() + Math.random(), message }, ...items].slice(0, 80));
  }

  function showToast(message: string, variant: ToastState["variant"] = "success", duration = 2600) {
    const next = { id: Date.now(), message, variant };
    setToast(next);
    window.setTimeout(() => {
      setToast((current) => current?.id === next.id ? null : current);
    }, duration);
  }

  async function saveWebdav() {
    await run("保存 WebDAV 配置", async () => {
      await invoke("save_webdav_config", { config: webdav });
      return "WebDAV 配置已保存";
    });
  }

  async function switchProvider(providerId: string) {
    await run(`切换 Provider 到 ${providerId}`, async () => {
      await invoke("switch_provider", { providerId });
      const result = await invoke<string>("unify_thread_provider");
      return `Provider 已切换到 ${providerId}，${result}`;
    });
  }

  async function saveProvider() {
    const normalized = normalizeProvider(providerDraft);
    const duplicate = providers.some((provider) => provider.id === normalized.id && provider.id !== editingProviderId);
    if (duplicate) {
      showToast(`Provider ID「${normalized.id}」已存在，请换一个唯一 ID`, "error", 5200);
      appendLog(`Provider ID 重复：${normalized.id}`);
      return;
    }
    await run("保存 Provider 配置", async () => {
      await invoke("save_provider", { provider: normalized });
      setView("providers");
      setEditingProviderId(null);
      return `Provider 配置已保存：${providerDraft.id}`;
    });
  }

  async function fetchModels() {
    await run("获取模型列表", async () => {
      const models = await invoke<ModelOption[]>("fetch_provider_models", { provider: normalizeProvider(providerDraft) });
      const ids = models.map((model) => model.id);
      setModelOptions(ids);
      if (!providerDraft.model && ids[0]) {
        setProviderDraft((current) => ({ ...current, model: ids[0] }));
      }
      return `已获取 ${ids.length} 个模型`;
    });
  }

  async function deleteProvider(providerId = providerDraft.id) {
    if (!providerId) return;
    await run("删除 Provider 配置", async () => {
      await invoke("delete_provider", { providerId });
      setProviderDraft(emptyProvider);
      setView("providers");
      return `Provider 配置已删除：${providerId}`;
    });
  }

  async function selectProvider(provider: ProviderConfig) {
    if (busy) return;
    setBusy(`读取 Provider ${provider.id}`);
    showToast(`正在读取 Provider ${provider.id}...`, "info", 1600);
    try {
      const latest = await invoke<ProviderConfig>("get_provider", { providerId: provider.id });
      setProviderDraft(toProviderDraft(latest));
      setModelOptions(latest.model ? [latest.model] : []);
      setEditingProviderId(provider.id);
      showToast(`已打开 Provider：${provider.id}`, "success");
    } catch (error) {
      const message = `读取 Provider 详情失败：${String(error)}`;
      appendLog(message);
      setProviderDraft(toProviderDraft(provider));
      setModelOptions(provider.model ? [provider.model] : []);
      setEditingProviderId(provider.id);
      showToast(message, "error", 5200);
    } finally {
      setBusy(null);
    }
    setView("provider-form");
  }

  function newProvider() {
    setProviderDraft(emptyProvider);
    setModelOptions(emptyProvider.model ? [emptyProvider.model] : []);
    setEditingProviderId(null);
    setView("provider-form");
  }

  const isBuiltinOpenAI = providerDraft.id === "openai";
  const activeProvider = providers.find((item) => item.id === summary.provider);
  const providerCanSwitch = providerDraft.id && providerDraft.id !== summary.provider;
  const providerCanDelete = providerDraft.id && providerDraft.id !== "openai" && providerDraft.id !== summary.provider && providers.some((item) => item.id === providerDraft.id);
  const duplicateProviderId = Boolean(providerDraft.id.trim() && providers.some((provider) => provider.id === providerDraft.id.trim() && provider.id !== editingProviderId));

  return (
    <main className="app-shell">
      {toast ? <div className={`toast ${toast.variant}`}>{toast.message}</div> : null}
      <header className="switch-topbar">
        <div className="brand">
          <div className="brand-icon"><Cpu size={18} /></div>
          <div>
            <h1>Codex Tools</h1>
          </div>
        </div>
        <div className="toolbar">
          <button className="tip-button" data-tip="拉取远端线程" aria-label="拉取远端线程" title="拉取远端线程" disabled={!!busy} onClick={() => run("拉取远端线程", () => invoke("pull_threads"))}><CloudDownload size={18} /></button>
          <button className="tip-button" data-tip="推送本地线程" aria-label="推送本地线程" title="推送本地线程" disabled={!!busy} onClick={() => run("推送本地线程", () => invoke("push_threads"))}><CloudUpload size={18} /></button>
          <button className="tip-button" data-tip="合并 Provider 线程" aria-label="合并 Provider 线程" title="合并 Provider 线程" disabled={!!busy} onClick={() => run("合并 Provider 线程", () => invoke("unify_thread_provider"))}><Shuffle size={18} /></button>
          <button title="WebDAV 配置" aria-label="WebDAV 配置" data-tip="WebDAV 配置" className={`tip-button ${view === "webdav" ? "active" : ""}`} onClick={() => setView("webdav")}><Settings2 size={18} /></button>
          <button title="运行日志" aria-label="运行日志" data-tip="运行日志" className={`tip-button ${view === "logs" ? "active" : ""}`} onClick={() => setView("logs")}><TerminalSquare size={18} /></button>
        </div>
        <button className="add-provider-btn tip-button" data-tip="新建 Provider" aria-label="新建 Provider" title="新建 Provider" disabled={!!busy} onClick={newProvider}><Plus size={26} /></button>
      </header>

      {view === "providers" ? (
        <section className="provider-board">
          <div className="board-meta">
            <div>
              <h2>Codex Providers</h2>
              <p>当前 {summary.provider}，共 {providers.length} 个配置</p>
            </div>
            <div className="stat-pills">
              <span>活跃 {summary.active_sessions}</span>
              <span>归档 {summary.archived_sessions}</span>
              <button disabled={!!busy} onClick={refreshAll}><RefreshCw size={15} />刷新</button>
            </div>
          </div>

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
                    <button className="enable-btn" disabled={!!busy} onClick={() => switchProvider(provider.id)}>
                      {busy?.includes(provider.id) ? <Loader2 className="spin" size={16} /> : <Play size={16} />}
                      启用
                    </button>
                  )}
                  <button className="icon-action" disabled={!!busy} onClick={() => void selectProvider(provider)}>编辑</button>
                  <button className="icon-action danger-text" disabled={!!busy || provider.id === summary.provider || provider.id === "openai"} onClick={() => deleteProvider(provider.id)}>删除</button>
                </div>
              </div>
            ))}
          </div>
        </section>
      ) : null}

      {view === "provider-form" ? (
        <section className="detail-page">
          <div className="detail-card glass">
            <button className="back-btn" onClick={() => setView("providers")}>返回</button>
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
                  {!isBuiltinOpenAI ? <button type="button" disabled={!!busy} onClick={fetchModels}>{busy === "获取模型列表" ? "获取中" : "获取模型"}</button> : null}
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
              <button className="editor-action primary-action" disabled={!!busy || !providerDraft.id || duplicateProviderId} onClick={saveProvider}><Save size={16} />保存 Provider</button>
              <button className="editor-action" disabled={!!busy || !providerCanSwitch} onClick={() => switchProvider(providerDraft.id)}><Shuffle size={16} />切换并合并</button>
              <button className="editor-action danger-action" disabled={!!busy || !providerCanDelete} onClick={() => deleteProvider()}><Trash2 size={16} />删除</button>
            </div>
          </div>
        </section>
      ) : null}

      {view === "webdav" ? (
        <section className="detail-page">
          <div className="detail-card compact-card glass">
            <button className="back-btn" onClick={() => setView("providers")}>返回</button>
            <div className="detail-title">
              <Settings2 size={34} />
              <div>
                <h2>WebDAV 配置</h2>
                <p>配置一次即可，之后只通过顶部同步按钮使用。</p>
              </div>
            </div>
            <div className="form-grid">
              <label>服务地址<input value={webdav.base_url} onChange={(event) => setWebdav({ ...webdav, base_url: event.target.value })} /></label>
              <label>用户名<input value={webdav.username} onChange={(event) => setWebdav({ ...webdav, username: event.target.value })} /></label>
              <label>密码<input type="password" value={webdav.password} onChange={(event) => setWebdav({ ...webdav, password: event.target.value })} /></label>
              <label className="checkbox-row"><input type="checkbox" checked={webdav.verify_tls} onChange={(event) => setWebdav({ ...webdav, verify_tls: event.target.checked })} />校验证书 TLS</label>
            </div>
            <div className="detail-actions"><button className="editor-action primary-action" disabled={!!busy} onClick={saveWebdav}>保存 WebDAV 配置</button></div>
          </div>
        </section>
      ) : null}

      {view === "logs" ? (
        <section className="detail-page">
          <div className="detail-card compact-card glass">
            <button className="back-btn" onClick={() => setView("providers")}>返回</button>
            <div className="detail-title">
              <TerminalSquare size={34} />
              <div>
                <h2>运行日志</h2>
                <p>仅在排查同步或 Provider 问题时查看。</p>
              </div>
            </div>
            <div className="log-list standalone">
              {logs.length === 0 ? <p className="empty">暂无日志</p> : null}
              {logs.map((line) => <div className="log-line" key={line.id}>{line.message}</div>)}
            </div>
          </div>
        </section>
      ) : null}

      <footer>
        <Archive size={14} />
        <span>{summary.codex_dir || "Codex 目录未检测"}</span>
        <button onClick={refreshAll} disabled={!!busy}><RefreshCw size={13} />刷新</button>
      </footer>
    </main>
  );
}

function toProviderDraft(provider: ProviderConfig): ProviderConfig {
  const defaults = providerDefaults(provider.id);
  return {
    id: provider.id ?? "",
    name: provider.id,
    auth_type: provider.auth_type ?? defaults.auth_type,
    base_url: provider.base_url ?? defaults.base_url,
    api_key: provider.api_key ?? defaults.api_key,
    wire_api: provider.wire_api ?? defaults.wire_api,
    model: provider.model ?? defaults.model,
    model_reasoning_effort: provider.model_reasoning_effort ?? defaults.model_reasoning_effort,
    requires_openai_auth: provider.requires_openai_auth ?? defaults.requires_openai_auth,
  };
}

function normalizeProvider(provider: ProviderConfig): ProviderConfig {
  return {
    id: provider.id.trim(),
    name: provider.id.trim(),
    auth_type: provider.auth_type,
    base_url: cleanOptional(provider.base_url),
    api_key: cleanOptional(provider.api_key),
    wire_api: cleanOptional(provider.wire_api),
    model: cleanOptional(provider.model),
    model_reasoning_effort: cleanOptional(provider.model_reasoning_effort),
    requires_openai_auth: !!provider.requires_openai_auth,
  };
}

function cleanOptional(value?: string) {
  const next = value?.trim();
  return next ? next : undefined;
}

function ensureBuiltinOpenAI(providers: ProviderConfig[]) {
  if (providers.some((provider) => provider.id === "openai")) {
    return providers;
  }
  return [builtinOpenAIProvider, ...providers];
}

function providerSummary(provider?: ProviderConfig) {
  if (!provider) return "Codex 默认 Provider";
  return provider.base_url || provider.wire_api || "Codex 默认 Provider";
}

function providerDefaults(providerId?: string): Required<Omit<ProviderConfig, "id">> {
  if (!providerId || providerId === "openai") {
    return {
      name: providerId === "openai" ? "Codex 默认 Provider" : "",
      auth_type: providerId === "openai" ? "chatgpt" : "api_key",
      base_url: "https://api.openai.com/v1",
      api_key: "",
      wire_api: "responses",
      model: "gpt-5.4",
      model_reasoning_effort: "medium",
      requires_openai_auth: false,
    };
  }
  return {
    name: "",
    auth_type: "api_key",
    base_url: "",
    api_key: "",
    wire_api: "responses",
    model: "gpt-5.4",
    model_reasoning_effort: "high",
    requires_openai_auth: true,
  };
}

function uniqueOptions(current: string | undefined, options: string[]) {
  return Array.from(new Set([current || "gpt-5.4", ...options].filter(Boolean)));
}

function Metric({ label, value, accent }: { label: string; value: string | number; accent: "blue" | "green" | "purple" }) {
  return (
    <div className={`metric ${accent}`}>
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function ActionCard({ icon, title, desc, busy, onClick }: { icon: React.ReactNode; title: string; desc: string; busy: boolean; onClick: () => void }) {
  return (
    <button className="action-card glass" onClick={onClick} disabled={busy}>
      <div className="action-icon">{busy ? <Loader2 className="spin" /> : icon}</div>
      <div>
        <strong>{title}</strong>
        <span>{desc}</span>
      </div>
    </button>
  );
}

function PanelTitle({ icon, title }: { icon: React.ReactNode; title: string }) {
  return (
    <div className="panel-title">
      {icon}
      <h3>{title}</h3>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
