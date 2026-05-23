export type Summary = {
  provider: string;
  active_sessions: number;
  archived_sessions: number;
  codex_dir: string;
};

export type ProviderConfig = {
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

export type WebDavConfig = {
  base_url: string;
  username: string;
  password: string;
  verify_tls: boolean;
};

export type TokenUsage = {
  input_tokens: number;
  cached_input_tokens: number;
  output_tokens: number;
  reasoning_output_tokens: number;
  total_tokens: number;
};

export type DailyUsage = TokenUsage & {
  date: string;
  cost_usd: number;
  events: number;
};

export type ProviderUsage = TokenUsage & {
  provider: string;
  cost_usd: number;
  events: number;
};

export type UsageSummary = {
  codex_dir: string;
  days: DailyUsage[];
  providers: ProviderUsage[];
  total: TokenUsage;
  total_cost_usd: number;
  files_scanned: number;
  usage_events: number;
};

export type BridgeProjectStatus = {
  name: string;
  work_dir: string;
  agent_type: string;
  has_weixin: boolean;
  has_weixin_session: boolean;
  has_codex: boolean;
  allow_from: string;
  admin_from: string;
  model: string;
  permission_mode: string;
};

export type CodexProjectStatus = {
  name: string;
  work_dir: string;
};

export type BridgeStatus = {
  installed: boolean;
  cc_connect_path?: string;
  version?: string;
  config_path: string;
  config_exists: boolean;
  daemon_status: string;
  qr_image_path: string;
  qr_image_exists: boolean;
  qr_image_data_url?: string;
  service_running: boolean;
  login_running: boolean;
  suggested_project_name: string;
  suggested_snippet: string;
  weixin_setup_command: string;
  start_command: string;
  has_logged_in_wechat_session: boolean;
  communication_ready: boolean;
  communication_hint: string;
  projects: BridgeProjectStatus[];
  codex_projects: CodexProjectStatus[];
};

export type BridgeProjectDraft = {
  name: string;
  work_dir: string;
  allow_from: string;
  admin_from: string;
  model: string;
  permission_mode: string;
};

export type LogLine = {
  id: number;
  message: string;
};
