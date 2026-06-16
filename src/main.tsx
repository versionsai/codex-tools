import React, { useEffect, useRef, useState } from "react";
import ReactDOM from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Archive, CloudDownload, CloudUpload, Cpu, Plus, RefreshCw, Shuffle } from "lucide-react";
import { buildToolOverviewStates, getWorkspaceTools, type View } from "./modules/tool-registry";
import type {
  BridgeProjectDraft,
  BridgeProjectStatus,
  BridgeStatus,
  CodexProjectStatus,
  LogLine,
  ProviderConfig,
  Summary,
  UsageSummary,
  WebDavConfig,
} from "./modules/app-types";
import { LogsPage, UsagePage, WechatPage, WebdavPage } from "./modules/tool-pages";
import { ProviderDashboardPage, ProviderFormPage } from "./modules/provider-pages";
import "./styles.css";

type ToastState = {
  id: number;
  message: string;
  variant: "info" | "success" | "error";
};

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

const emptyUsage: UsageSummary = {
  codex_dir: "",
  days: [],
  providers: [],
  total: {
    input_tokens: 0,
    cached_input_tokens: 0,
    output_tokens: 0,
    reasoning_output_tokens: 0,
    total_tokens: 0,
  },
  total_cost_usd: 0,
  files_scanned: 0,
  usage_events: 0,
};

const emptyBridge: BridgeStatus = {
  installed: false,
  cc_connect_path: "",
  version: "",
  config_path: "",
  config_exists: false,
  daemon_status: "",
  qr_image_path: "",
  qr_image_exists: false,
  qr_image_data_url: "",
  service_running: false,
  login_running: false,
  suggested_project_name: "codex-tools-wechat",
  suggested_snippet: "",
  weixin_setup_command: "",
  start_command: "",
  has_logged_in_wechat_session: false,
  communication_ready: false,
  communication_hint: "",
  projects: [],
  codex_projects: [],
};

const emptyBridgeDraft: BridgeProjectDraft = {
  name: "codex-tools-wechat",
  work_dir: "",
  allow_from: "*",
  admin_from: "",
  model: "gpt-5.5",
  permission_mode: "plan",
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
  const [usage, setUsage] = useState<UsageSummary>(emptyUsage);
  const [bridge, setBridge] = useState<BridgeStatus>(emptyBridge);
  const [bridgeProjectName, setBridgeProjectName] = useState("codex-tools-wechat");
  const [bridgeDraft, setBridgeDraft] = useState<BridgeProjectDraft>(emptyBridgeDraft);
  const [syncBusy, setSyncBusy] = useState<string | null>(null);
  const [providerBusy, setProviderBusy] = useState<string | null>(null);
  const [bridgeBusy, setBridgeBusy] = useState<string | null>(null);
  const [logs, setLogs] = useState<LogLine[]>([]);
  const [modelOptions, setModelOptions] = useState<string[]>([]);
  const [toast, setToast] = useState<ToastState | null>(null);
  const [editingProviderId, setEditingProviderId] = useState<string | null>(null);
  const bridgeStatusRef = useRef<BridgeStatus>(emptyBridge);
  const autoLoginProjectRef = useRef("");
  const autoStartingBridgeRef = useRef(false);
  const autoSetupBusyRef = useRef(false);
  const autoQrRetryRef = useRef(false);

  useEffect(() => {
    void bootstrap();
  }, []);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;

    void listen("codex-tools-refresh", () => {
      void bootstrap();
    }).then((nextUnlisten) => {
      if (disposed) {
        nextUnlisten();
        return;
      }
      unlisten = nextUnlisten;
    });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    if (view !== "wechat") return;
    void loadBridge(false);
    const refreshOnFocus = () => {
      void loadBridge(false);
    };
    window.addEventListener("focus", refreshOnFocus);
    return () => window.removeEventListener("focus", refreshOnFocus);
  }, [view]);

  useEffect(() => {
    if (view !== "wechat") return;
    const shouldPoll = bridge.login_running || (!bridge.service_running && (!!bridge.qr_image_exists || !!bridgeBusy));
    if (!shouldPoll) return;
    const timer = window.setInterval(() => {
      void loadBridge();
    }, 1500);
    return () => window.clearInterval(timer);
  }, [view, bridge.login_running, bridge.service_running, bridge.qr_image_exists, bridgeBusy]);

  useEffect(() => {
    if (view !== "wechat") return;
    const ready = bridgeDraft.name.trim() && bridgeDraft.work_dir.trim();
    if (!ready || bridge.has_logged_in_wechat_session || bridge.service_running || bridge.login_running || bridgeBusy) return;
    if (autoLoginProjectRef.current === bridgeDraft.work_dir && bridge.qr_image_exists) return;
    void ensureWechatLoginFlow(bridgeDraft.name.trim(), bridgeDraft.work_dir.trim(), true);
  }, [
    view,
    bridgeBusy,
    bridge.has_logged_in_wechat_session,
    bridge.service_running,
    bridge.login_running,
    bridge.qr_image_exists,
    bridgeDraft.name,
    bridgeDraft.work_dir,
  ]);

  useEffect(() => {
    if (view !== "wechat") return;
    const ready = bridgeDraft.name.trim() && bridgeDraft.work_dir.trim();
    if (!ready || bridge.service_running || bridge.login_running || bridgeBusy) return;
    if (!bridge.has_logged_in_wechat_session) return;
    void startWechatConnectionSilently("检测到已登录微信，自动补启动连接");
  }, [
    view,
    bridgeBusy,
    bridge.service_running,
    bridge.login_running,
    bridge.has_logged_in_wechat_session,
    bridgeDraft.name,
    bridgeDraft.work_dir,
  ]);

  async function bootstrap() {
    await refreshSummary();
    await refreshProviders();
    await loadUsage();
    const bridgeStatus = await ensureBridgeReady(true);
    await autoStartWechatConnection(bridgeStatus);
    await loadWebdav();
  }

  async function refreshAll() {
    await runScoped("sync", "刷新状态", async () => {
      await bootstrap();
      return "状态已刷新";
    });
  }

  async function refreshSummary() {
    try {
      setSummary(await invoke<Summary>("get_summary"));
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
      setWebdav(await invoke<WebDavConfig>("load_webdav_config"));
    } catch (error) {
      appendLog(`读取 WebDAV 配置失败：${String(error)}`);
    }
  }

  async function loadUsage() {
    try {
      setUsage(await invoke<UsageSummary>("get_usage_summary"));
    } catch (error) {
      appendLog(`读取 Codex 用量失败：${String(error)}`);
      setUsage(emptyUsage);
    }
  }

  async function loadBridge(processTransitions = true) {
    try {
      const next = await invoke<BridgeStatus>("get_bridge_status");
      const previous = bridgeStatusRef.current;
      bridgeStatusRef.current = next;
      setBridge(next);
      const savedProject = next.projects[0];
      setBridgeProjectName((current) => savedProject?.name || (current.trim() ? current : next.suggested_project_name));
      setBridgeDraft((current) => {
        if (savedProject) {
          return bridgeDraftFromProject(savedProject);
        }
        return {
          ...current,
          name: current.name.trim() ? current.name : next.suggested_project_name,
        };
      });
      if (processTransitions) {
        void handleBridgeStatusTransition(previous, next);
      }
      return next;
    } catch (error) {
      appendLog(`读取微信连接状态失败：${String(error)}`);
      setBridge(emptyBridge);
      bridgeStatusRef.current = emptyBridge;
      return null;
    }
  }

  async function ensureBridgeReady(silent = false) {
    const current = await loadBridge();
    if (!current || current.installed) return current;
    try {
      const message = await invoke<string>("install_cc_connect");
      appendLog(message);
      if (!silent) showToast(message, "success");
    } catch (error) {
      const message = `初始化微信连接失败：${String(error)}`;
      appendLog(message);
      if (!silent) showToast(message, "error", 5200);
      return current;
    }
    return loadBridge();
  }

  async function autoStartWechatConnection(status?: BridgeStatus | null) {
    const current = status ?? await loadBridge();
    if (!current) return;
    const hasProject = current.projects.length > 0;
    if (!current.installed || !current.config_exists || !hasProject || current.service_running) {
      return;
    }
    try {
      const message = await invoke<string>("open_cc_connect_terminal");
      appendLog(`自动启动微信连接：${message}`);
      await loadBridge();
    } catch (error) {
      appendLog(`自动启动微信连接失败：${String(error)}`);
    }
  }

  async function runScoped(
    scope: "sync" | "provider" | "bridge",
    label: string,
    action: () => Promise<string>,
  ) {
    const busyValue = scope === "sync" ? syncBusy : scope === "provider" ? providerBusy : bridgeBusy;
    if (busyValue) return;
    const setBusyValue = scope === "sync" ? setSyncBusy : scope === "provider" ? setProviderBusy : setBridgeBusy;
    setBusyValue(label);
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
      setBusyValue(null);
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

  async function startWechatConnection() {
    await runScoped("bridge", "启动微信连接", async () => {
      await ensureBridgeReady(true);
      const message = await invoke<string>("open_cc_connect_terminal");
      await loadBridge();
      return message;
    });
  }

  async function stopWechatConnection() {
    await runScoped("bridge", "停止微信连接", async () => {
      const message = await invoke<string>("run_bridge_daemon_command", { action: "stop" });
      await loadBridge();
      return message;
    });
  }

  async function reloginWechat() {
    const projectName = bridgeDraft.name.trim();
    const workDir = bridgeDraft.work_dir.trim();
    if (!projectName || !workDir) {
      showToast("请先选择项目", "error", 2600);
      return;
    }
    await runScoped("bridge", "重新登录微信", async () => {
      autoLoginProjectRef.current = "";
      autoQrRetryRef.current = false;
      const next = await ensureWechatLoginFlow(projectName, workDir, false);
      return next?.qr_image_exists ? "已重新生成二维码，请用微信扫码" : "已重新开始微信登录流程";
    });
  }

  async function refreshWechatQr() {
    await reloginWechat();
  }

  function editBridgeProject(project: BridgeProjectStatus) {
    setBridgeDraft({
      name: project.name,
      work_dir: project.work_dir,
      allow_from: project.allow_from || "*",
      admin_from: project.admin_from || "",
      model: project.model || "gpt-5.5",
      permission_mode: project.permission_mode || "plan",
    });
    setBridgeProjectName(project.name);
  }

  async function ensureWechatLoginFlow(projectName: string, workDir: string, silent = false) {
    if (!projectName || !workDir || autoSetupBusyRef.current) return null;
    autoSetupBusyRef.current = true;
    try {
      await ensureBridgeReady(true);
      const message = await invoke<string>("open_wechat_setup_terminal", { projectName });
      autoLoginProjectRef.current = workDir;
      appendLog(message);
      if (!silent) showToast("已更新二维码，请用微信扫码", "success", 2200);
      return await loadBridge(false);
    } catch (error) {
      const message = `自动生成二维码失败：${String(error)}`;
      appendLog(message);
      if (!silent) showToast(message, "error", 4200);
      return null;
    } finally {
      autoSetupBusyRef.current = false;
    }
  }

  async function startWechatConnectionSilently(reason?: string) {
    if (autoStartingBridgeRef.current) return null;
    autoStartingBridgeRef.current = true;
    try {
      await ensureBridgeReady(true);
      const message = await invoke<string>("open_cc_connect_terminal");
      appendLog(reason ? `${reason}：${message}` : message);
      return await loadBridge(false);
    } catch (error) {
      const message = `自动启动微信连接失败：${String(error)}`;
      appendLog(message);
      showToast(message, "error", 4200);
      return null;
    } finally {
      autoStartingBridgeRef.current = false;
    }
  }

  async function handleBridgeStatusTransition(previous: BridgeStatus, next: BridgeStatus) {
    if (view !== "wechat") return;
    if (next.service_running && !previous.service_running) {
      showToast("微信连接已启动", "success", 2200);
      autoLoginProjectRef.current = next.projects[0]?.work_dir ?? autoLoginProjectRef.current;
      autoQrRetryRef.current = false;
      return;
    }
    const scanCompleted = previous.login_running
      && !next.login_running
      && (next.has_logged_in_wechat_session || next.service_running || next.qr_image_exists);
    if (scanCompleted) {
      showToast(
        next.service_running ? "扫码成功，微信连接已启动" : "扫码成功，正在启动微信连接",
        "success",
        2600,
      );
      if (!next.service_running) {
        await startWechatConnectionSilently("扫码成功后自动启动");
      }
      return;
    }
    const loginEndedWithoutConnection = previous.login_running && !next.login_running && !next.service_running;
    if (loginEndedWithoutConnection && !autoQrRetryRef.current && bridgeDraft.name.trim() && bridgeDraft.work_dir.trim()) {
      autoQrRetryRef.current = true;
      showToast("二维码可能已过期，正在自动刷新", "info", 2400);
      await ensureWechatLoginFlow(bridgeDraft.name.trim(), bridgeDraft.work_dir.trim(), true);
    }
  }

  async function selectCodexProject(project: CodexProjectStatus) {
    const existing = bridge.projects.find((item) => item.work_dir === project.work_dir);
    const payload: BridgeProjectDraft = existing ? {
      name: existing.name,
      work_dir: existing.work_dir,
      allow_from: existing.allow_from || "*",
      admin_from: existing.admin_from || "",
      model: existing.model || "gpt-5.5",
      permission_mode: existing.permission_mode || "plan",
    } : {
      ...bridgeDraft,
      name: project.name || bridgeDraft.name || bridgeProjectName || "codex-tools-wechat",
      work_dir: project.work_dir,
      allow_from: bridgeDraft.allow_from.trim() || "*",
      admin_from: bridgeDraft.admin_from.trim(),
      model: bridgeDraft.model.trim() || "gpt-5.5",
      permission_mode: bridgeDraft.permission_mode || "plan",
    };

    setBridgeDraft(payload);
    setBridgeProjectName(payload.name);

    if (existing) {
      editBridgeProject(existing);
    }

    await runScoped("bridge", `切换项目到 ${project.name}`, async () => {
      const message = await invoke<string>("save_bridge_project", { project: payload });
      const next = await loadBridge(false);
      const saved = next?.projects.find((item) => item.work_dir === payload.work_dir);
      if (saved) {
        setBridgeDraft({
          name: saved.name,
          work_dir: saved.work_dir,
          allow_from: saved.allow_from || "*",
          admin_from: saved.admin_from || "",
          model: saved.model || "gpt-5.5",
          permission_mode: saved.permission_mode || "plan",
        });
        setBridgeProjectName(saved.name);
      }
      if (!next?.has_logged_in_wechat_session) {
        await ensureWechatLoginFlow(payload.name, payload.work_dir, false);
      } else if (!next.service_running) {
        await startWechatConnectionSilently("切换项目后自动启动");
      }
      return message || `已切换到项目：${project.name}`;
    });
  }

  async function changeBridgePermissionMode(permissionMode: string) {
    const payload: BridgeProjectDraft = {
      ...bridgeDraft,
      permission_mode: permissionMode,
    };
    if (!payload.name.trim() || !payload.work_dir.trim()) {
      showToast("请先选择项目", "error", 2600);
      return;
    }
    setBridgeDraft(payload);
    await runScoped("bridge", "更新微信权限模式", async () => {
      const message = await invoke<string>("save_bridge_project", { project: payload });
      const next = await loadBridge(false);
      if (next?.service_running) {
        await invoke<string>("run_bridge_daemon_command", { action: "restart" });
      }
      await loadBridge(false);
      return message;
    });
  }

  async function saveWebdav() {
    await runScoped("sync", "保存 WebDAV 配置", async () => {
      await invoke("save_webdav_config", { config: webdav });
      return "WebDAV 配置已保存";
    });
  }

  async function pullThreads() {
    await runScoped("sync", "拉取远端线程", async () => {
      const result = await invoke<string>("pull_threads");
      const restartMessage = await invoke<string>("restart_codex_app");
      return `${result}，${restartMessage}`;
    });
  }

  async function repairThreadVisibilityIndex() {
    await runScoped("sync", "修复线程可见性", async () => {
      const result = await invoke<string>("repair_thread_visibility_index");
      const restartMessage = await invoke<string>("restart_codex_app");
      return `${result}，${restartMessage}`;
    });
  }

  async function switchProvider(providerId: string) {
    await runScoped("provider", `切换 Provider 到 ${providerId}`, async () => {
      await invoke("switch_provider", { providerId });
      const result = await invoke<string>("unify_thread_provider");
      const restartMessage = await invoke<string>("restart_codex_app");
      return `Provider 已切换到 ${providerId}，${result}，${restartMessage}`;
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
    await runScoped("provider", "保存 Provider 配置", async () => {
      await invoke("save_provider", { provider: normalized });
      setView("providers");
      setEditingProviderId(null);
      return `Provider 配置已保存：${providerDraft.id}`;
    });
  }

  async function fetchModels() {
    await runScoped("provider", "获取模型列表", async () => {
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
    await runScoped("provider", "删除 Provider 配置", async () => {
      await invoke("delete_provider", { providerId });
      setProviderDraft(emptyProvider);
      setView("providers");
      return `Provider 配置已删除：${providerId}`;
    });
  }

  async function selectProvider(provider: ProviderConfig) {
    if (providerBusy) return;
    setProviderBusy(`读取 Provider ${provider.id}`);
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
      setProviderBusy(null);
    }
    setView("provider-form");
  }

  function newProvider() {
    setProviderDraft(emptyProvider);
    setModelOptions(emptyProvider.model ? [emptyProvider.model] : []);
    setEditingProviderId(null);
    setView("provider-form");
  }

  const toolStates = buildToolOverviewStates({
    usage: {
      totalCostUsd: usage.total_cost_usd,
      providerCount: usage.providers.length,
      usageEvents: usage.usage_events,
    },
    wechat: {
      installed: bridge.installed,
      projectCount: bridge.projects.length,
      daemonStatus: bridge.daemon_status,
    },
    webdav: {
      baseUrl: webdav.base_url,
    },
    logs: {
      count: logs.length,
      latestMessage: logs[0]?.message ?? "",
    },
    formatUsd,
  });
  const workspaceTools = getWorkspaceTools();

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
          <button className="tip-button" data-tip="拉取远端线程" aria-label="拉取远端线程" title="拉取远端线程" disabled={!!syncBusy} onClick={() => void pullThreads()}><CloudDownload size={18} /></button>
          <button className="tip-button" data-tip="推送本地线程" aria-label="推送本地线程" title="推送本地线程" disabled={!!syncBusy} onClick={() => void runScoped("sync", "推送本地线程", () => invoke("push_threads"))}><CloudUpload size={18} /></button>
          <button className="tip-button" data-tip="合并 Provider 线程" aria-label="合并 Provider 线程" title="合并 Provider 线程" disabled={!!syncBusy} onClick={() => void runScoped("sync", "合并 Provider 线程", () => invoke("unify_thread_provider"))}><Shuffle size={18} /></button>
          <button className="tip-button" data-tip="修复线程可见性" aria-label="修复线程可见性" title="修复线程可见性" disabled={!!syncBusy} onClick={() => void repairThreadVisibilityIndex()}><RefreshCw size={18} /></button>
          {workspaceTools.filter((tool) => tool.capabilities.hasDetailPage).map((tool) => {
            const Icon = tool.icon;
            return (
              <button
                key={tool.id}
                title={tool.title}
                aria-label={tool.title}
                data-tip={tool.tip}
                className={`tip-button ${view === tool.id ? "active" : ""}`}
                onClick={() => setView(tool.id)}
              >
                <Icon size={18} />
              </button>
            );
          })}
        </div>
        <button className="add-provider-btn tip-button" data-tip="新建 Provider" aria-label="新建 Provider" title="新建 Provider" disabled={!!providerBusy} onClick={newProvider}><Plus size={26} /></button>
      </header>

      {view === "providers" ? (
        <ProviderDashboardPage
          summary={summary}
          providers={providers}
          busy={!!providerBusy}
          refreshAll={refreshAll}
          switchProvider={switchProvider}
          selectProvider={selectProvider}
          deleteProvider={deleteProvider}
          setView={setView}
          toolCards={(
            <div className="tool-overview-grid">
              {workspaceTools.map((tool) => {
                const Icon = tool.icon;
                return (
                  <button
                    key={tool.id}
                    className="tool-overview-card glass"
                    disabled={!!providerBusy}
                    onClick={() => setView(tool.id)}
                  >
                    <div className="tool-overview-icon">
                      <Icon size={20} />
                    </div>
                    <div className="tool-overview-copy">
                      <strong>{tool.title}</strong>
                      <p>{tool.description}</p>
                    </div>
                    <div className="tool-overview-meta">
                      <span>{toolStates[tool.id]?.label}</span>
                      <small>{toolStates[tool.id]?.detail}</small>
                    </div>
                  </button>
                );
              })}
            </div>
          )}
        />
      ) : null}

      {view === "provider-form" ? (
        <ProviderFormPage
          providerDraft={providerDraft}
          editingProviderId={editingProviderId}
          providers={providers}
          currentProviderId={summary.provider}
          busy={providerBusy}
          modelOptions={modelOptions}
          onBack={() => setView("providers")}
          setProviderDraft={setProviderDraft}
          fetchModels={fetchModels}
          saveProvider={saveProvider}
          switchProvider={switchProvider}
          deleteProvider={deleteProvider}
        />
      ) : null}

      {view === "usage" ? (
        <UsagePage
          usage={usage}
          busy={!!syncBusy}
          onBack={() => setView("providers")}
          onRefresh={loadUsage}
          formatUsd={formatUsd}
          formatToken={formatToken}
        />
      ) : null}

      {view === "wechat" ? (
        <WechatPage
          bridge={bridge}
          currentWorkDir={bridgeDraft.work_dir}
          busy={!!bridgeBusy}
          onBack={() => setView("providers")}
          selectCodexProject={selectCodexProject}
          permissionMode={bridgeDraft.permission_mode}
          changePermissionMode={changeBridgePermissionMode}
          refreshWechatQr={refreshWechatQr}
          reloginWechat={reloginWechat}
          startWechatConnection={startWechatConnection}
          stopWechatConnection={stopWechatConnection}
        />
      ) : null}

      {view === "webdav" ? (
        <WebdavPage webdav={webdav} busy={!!syncBusy} onBack={() => setView("providers")} onSave={saveWebdav} setWebdav={setWebdav} />
      ) : null}

      {view === "logs" ? (
        <LogsPage logs={logs} onBack={() => setView("providers")} />
      ) : null}

      <footer>
        <Archive size={14} />
        <span>{summary.codex_dir || "Codex 目录未检测"}</span>
        <button onClick={refreshAll} disabled={!!syncBusy}><RefreshCw size={13} />刷新</button>
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

function bridgeDraftFromProject(project: BridgeProjectStatus): BridgeProjectDraft {
  return {
    name: project.name,
    work_dir: project.work_dir,
    allow_from: project.allow_from || "*",
    admin_from: project.admin_from || "",
    model: project.model || "gpt-5.5",
    permission_mode: project.permission_mode || "plan",
  };
}

function formatToken(value: number) {
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(2)}M`;
  if (value >= 1_000) return `${(value / 1_000).toFixed(1)}K`;
  return String(value);
}

function formatUsd(value: number) {
  if (!Number.isFinite(value) || value <= 0) return "$0.00";
  if (value < 0.01) return `$${value.toFixed(4)}`;
  return `$${value.toFixed(2)}`;
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
