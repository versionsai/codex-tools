import type { LucideIcon } from "lucide-react";
import { BarChart3, MessageCircleMore, Settings2, TerminalSquare } from "lucide-react";

export type View = "providers" | "provider-form" | "usage" | "wechat" | "webdav" | "logs";

export type ToolView = Exclude<View, "providers" | "provider-form">;

export type ToolOverviewState = {
  label: string;
  detail: string;
};

export type ToolDefinition = {
  id: ToolView;
  title: string;
  tip: string;
  description: string;
  icon: LucideIcon;
  buildOverview: (snapshot: ToolRegistrySnapshot) => ToolOverviewState;
  detailTitle: string;
  detailDescription: string;
  capabilities: {
    showInWorkspace: boolean;
    hasDetailPage: boolean;
    hasConfig: boolean;
    category: "analytics" | "integration" | "sync" | "diagnostics";
  };
};

export type ToolRegistrySnapshot = {
  usage: {
    totalCostUsd: number;
    providerCount: number;
    usageEvents: number;
  };
  wechat: {
    installed: boolean;
    projectCount: number;
    daemonStatus: string;
  };
  webdav: {
    baseUrl: string;
  };
  logs: {
    count: number;
    latestMessage: string;
  };
  formatUsd: (value: number) => string;
};

export const toolRegistry: ToolDefinition[] = [
  {
    id: "usage",
    title: "用量统计",
    tip: "用量统计",
    description: "离线统计 Codex token 与预估 Cost，按日期和 Provider 汇总。",
    icon: BarChart3,
    detailTitle: "Codex 用量统计",
    detailDescription: "基于本地 JSONL 离线统计，Cost 统一按 OpenAI 官方模型价格估算",
    capabilities: {
      showInWorkspace: true,
      hasDetailPage: true,
      hasConfig: false,
      category: "analytics",
    },
    buildOverview: (snapshot) => ({
      label: snapshot.formatUsd(snapshot.usage.totalCostUsd),
      detail: `${snapshot.usage.providerCount} 个 Provider，${snapshot.usage.usageEvents} 条事件`,
    }),
  },
  {
    id: "wechat",
    title: "微信连接",
    tip: "微信连接",
    description: "管理微信登录、连接配置和服务状态。",
    icon: MessageCircleMore,
    detailTitle: "微信连接",
    detailDescription: "在应用内完成微信登录、连接配置和服务管理。",
    capabilities: {
      showInWorkspace: true,
      hasDetailPage: true,
      hasConfig: true,
      category: "integration",
    },
    buildOverview: (snapshot) => ({
      label: snapshot.wechat.installed ? `${snapshot.wechat.projectCount} 个项目` : "未安装",
      detail: snapshot.wechat.installed
        ? (snapshot.wechat.daemonStatus || "微信连接已就绪")
        : "等待安装微信连接",
    }),
  },
  {
    id: "webdav",
    title: "WebDAV 配置",
    tip: "WebDAV 配置",
    description: "维护线程云同步所需的 WebDAV 参数，供顶部同步动作直接调用。",
    icon: Settings2,
    detailTitle: "WebDAV 配置",
    detailDescription: "配置一次即可，之后只通过顶部同步按钮使用。",
    capabilities: {
      showInWorkspace: true,
      hasDetailPage: true,
      hasConfig: true,
      category: "sync",
    },
    buildOverview: (snapshot) => ({
      label: snapshot.webdav.baseUrl.trim() ? "已配置" : "未配置",
      detail: snapshot.webdav.baseUrl.trim() || "尚未填写 WebDAV 服务地址",
    }),
  },
  {
    id: "logs",
    title: "运行日志",
    tip: "运行日志",
    description: "查看最近的应用动作和错误信息，便于排查同步与 Provider 问题。",
    icon: TerminalSquare,
    detailTitle: "运行日志",
    detailDescription: "仅在排查同步或 Provider 问题时查看。",
    capabilities: {
      showInWorkspace: true,
      hasDetailPage: true,
      hasConfig: false,
      category: "diagnostics",
    },
    buildOverview: (snapshot) => ({
      label: `${snapshot.logs.count} 条日志`,
      detail: snapshot.logs.latestMessage || "最近动作和错误会显示在这里",
    }),
  },
];

export function buildToolOverviewStates(snapshot: ToolRegistrySnapshot): Record<ToolView, ToolOverviewState> {
  return Object.fromEntries(toolRegistry.map((tool) => [tool.id, tool.buildOverview(snapshot)])) as Record<
    ToolView,
    ToolOverviewState
  >;
}

export function getToolDefinition(view: ToolView): ToolDefinition {
  return toolRegistry.find((tool) => tool.id === view) as ToolDefinition;
}

export function getWorkspaceTools(): ToolDefinition[] {
  return toolRegistry.filter((tool) => tool.capabilities.showInWorkspace);
}
