export interface ChatMessage {
  role: string;
  content: string;
  streaming?: boolean;
}

export interface WorkspaceEntry {
  path: string;
  name: string;
  type: 'dir' | 'file';
  depth: number;
}

export interface SessionEntry {
  index: number;
  rounds: number;
  relative_time: string;
  preview: string;
  checkpoint_count?: number;
  current?: boolean;
  usage_totals?: UsageSummary;
}

export interface SessionCheckpointEntry {
  index: number;
  relative_time: string;
  preview: string;
  rounds: number;
  usage_totals?: UsageSummary;
}

export interface ChangeEntry {
  path: string;
  basename: string;
  backup_time: string;
}

export interface WorkspacePreview {
  name: string;
  path: string;
  rel: string;
  kind: 'text' | 'image' | 'binary';
  mime: string;
  size: number;
  truncated?: boolean;
  content?: string;
  message?: string;
}

export interface SkillEntry {
  name: string;
  display_name?: string;
  description?: string;
  enabled: boolean;
  source_url?: string;
  version?: string;
  installed_at?: string;
  file_count?: number;
  files?: string[];
}

export interface WorkflowNode {
  mode: 'work' | 'plan' | 'review';
  label?: string;
  completed?: boolean;
}

export interface WorkspaceState {
  active: { name?: string; path: string } | null;
  workspaces: Array<{ name?: string; path: string }>;
  recent_folders: string[];
}

export interface RemoteForm {
  enabled: boolean;
  server_name: string;
  name: string;
  host: string;
  port: number;
  username: string;
  password: string;
  key_path: string;
  cwd: string;
}

export interface RemoteState {
  form: Partial<RemoteForm>;
  configs: Array<{ name: string; host: string; port: number; username: string }>;
  active_connections: string[];
  connected: boolean;
}

export interface ModelState {
  name?: string;
  model?: string;
  provider?: string;
  apibase?: string;
  apikey?: string;
  session_type?: string;
  entry_key?: string;
  protocol_preset?: string;
  api_mode?: string;
}

export interface ModelOption {
  index: number;
  label: string;
  name?: string;
  model: string;
}

export interface ProviderProfile {
  id: string;
  label: string;
  provider: string;
  description: string;
  session_type: string;
  api_mode: string;
  apibase: string;
  model: string;
  reasoning_effort?: string | null;
}

export interface UsageSummary {
  prompt_tokens: number;
  completion_tokens: number;
  cached_tokens: number;
  turns?: number;
}

export interface AutoRouteState {
  model: string;
  display_name?: string;
  reasoning_effort?: string | null;
  reason?: string;
}

export interface ModelsPayload {
  models: ModelOption[];
  current_index: number;
}

export interface BootstrapPayload {
  app_name?: string;
  subtitle?: string;
  theme?: string;
  messages?: ChatMessage[];
  is_running?: boolean;
  pending_task?: {
    task_id: string;
    preview?: string;
  } | null;
  model?: string;
  model_index?: number;
  workspace?: WorkspaceState;
  remote?: RemoteState;
  llm_form?: ModelState;
  models?: ModelsPayload;
  provider_profiles?: ProviderProfile[];
  mode?: 'work' | 'plan' | 'review';
  workflow?: { nodes?: WorkflowNode[]; active?: boolean; current_node?: number };
  picker_token?: string;
  multi_agent_enabled?: boolean;
  one_shot_enabled?: boolean;
  loop_enabled?: boolean;
  workflow_follow_enabled?: boolean;
  computer_use_enabled?: boolean;
  computer_use_available?: boolean;
  yolo_enabled?: boolean;
  reasoning_effort?: string | null;
  auto_model_enabled?: boolean;
  auto_route?: AutoRouteState | null;
  current_session?: {
    index: number;
    active_checkpoint?: number | null;
    checkpoint_count: number;
    origin_session_index?: number | null;
    origin_checkpoint_index?: number | null;
    usage_totals?: UsageSummary;
    last_usage?: UsageSummary | null;
    checkpoints?: SessionCheckpointEntry[];
  } | null;
}

export interface TaskStatus {
  done: boolean;
  preview: string;
  final: string;
  usage?: {
    prompt_tokens?: number;
    completion_tokens?: number;
    prompt_cache_hit_tokens?: number;
    prompt_cache_miss_tokens?: number;
  } | null;
}
