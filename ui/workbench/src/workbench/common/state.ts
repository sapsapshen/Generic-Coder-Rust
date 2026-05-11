import type {
  AutoRouteState,
  ChangeEntry,
  ChatMessage,
  ModelOption,
  ModelState,
  ProviderProfile,
  RemoteForm,
  RemoteState,
  SessionEntry,
  SessionCheckpointEntry,
  SkillEntry,
  UsageSummary,
  WorkspaceEntry,
  WorkspacePreview,
  WorkspaceState,
  WorkflowNode,
} from '../../types';

export type ViewId = 'explorer' | 'scm' | 'extensions' | 'chat' | 'settings' | 'workflow';
export type ModeId = 'ask' | 'plan' | 'build' | 'review';

export type EditorTab =
  | { id: 'chat'; title: string; kind: 'chat' }
  | { id: string; title: string; kind: 'preview'; path: string; preview: WorkspacePreview }
  | { id: string; title: string; kind: 'diff'; path: string; diff: string };

export const DEFAULT_LLM_FORM: ModelState = {
  entry_key: 'generic_coder_native_oai_config',
  session_type: 'native_oai',
  protocol_preset: 'custom',
  api_mode: 'chat_completions',
  provider: '',
  name: '',
  apibase: '',
  apikey: '',
  model: '',
};

export const DEFAULT_REMOTE_FORM: RemoteForm = {
  enabled: false,
  server_name: '',
  name: '',
  host: '',
  port: 22,
  username: 'root',
  password: '',
  key_path: '',
  cwd: '',
};

export const THEME_OPTIONS = ['graphite', 'obsidian', 'cobalt', 'daybreak', 'paperink', 'solarflare'] as const;

export interface WorkbenchState {
  activeView: ViewId;
  activeTabId: string;
  tabs: EditorTab[];
  messages: ChatMessage[];
  sessions: SessionEntry[];
  workspaceTree: WorkspaceEntry[];
  changes: ChangeEntry[];
  skills: SkillEntry[];
  llmForm: ModelState;
  providerProfiles: ProviderProfile[];
  workspace: WorkspaceState;
  remote: RemoteState;
  models: ModelOption[];
  currentModelIndex: number;
  theme: string;
  isRunning: boolean;
  pendingTaskId: string | null;
  taskPlaceholderIndex: number | null;
  modelLabel: string;
  workspacePickerToken: string;
  workspaceCollapsedPaths: string[];
  workspaceDraftName: string;
  workspaceDraftPath: string;
  workflowNodes: WorkflowNode[];
  workflowActive: boolean;
  workflowCurrentNode: number;
  workflowFollowEnabled: boolean;
  loopEnabled: boolean;
  loopAvailable: boolean;
  computerUseEnabled: boolean;
  computerUseAvailable: boolean;
  yoloEnabled: boolean;
  reasoningEffort: 'off' | 'high' | 'max' | null;
  autoModelEnabled: boolean;
  autoRoute: AutoRouteState | null;
  lastUsage: { prompt_tokens: number; completion_tokens: number; cached_tokens?: number } | null;
  sessionUsage: UsageSummary | null;
  currentSessionIndex: number | null;
  currentSessionActiveCheckpoint: number | null;
  currentSessionCheckpoints: SessionCheckpointEntry[];
  checkpointPanelSessionIndex: number | null;
  checkpointPanelEntries: SessionCheckpointEntry[];
  currentMode: ModeId;
  multiAgentEnabled: boolean;
  oneShotEnabled: boolean;
  showDetailedAgentLogs: boolean;
  planRemaining: number;
  quickOpenVisible: boolean;
  inputValue: string;
}

export function createInitialWorkbenchState(): WorkbenchState {
  return {
    activeView: 'chat',
    activeTabId: 'chat',
    tabs: [{ id: 'chat', title: 'Chat', kind: 'chat' }],
    messages: [],
    sessions: [],
    workspaceTree: [],
    changes: [],
    skills: [],
    llmForm: { ...DEFAULT_LLM_FORM },
    providerProfiles: [],
    workspace: { active: null, workspaces: [], recent_folders: [] },
    remote: { form: { ...DEFAULT_REMOTE_FORM }, configs: [], active_connections: [], connected: false },
    models: [],
    currentModelIndex: 0,
    theme: 'graphite',
    isRunning: false,
    pendingTaskId: null,
    taskPlaceholderIndex: null,
    modelLabel: 'Model offline',
    workspacePickerToken: '',
    workspaceCollapsedPaths: [],
    workspaceDraftName: '',
    workspaceDraftPath: '',
    workflowNodes: [],
    workflowActive: false,
    workflowCurrentNode: 0,
    workflowFollowEnabled: false,
    loopEnabled: false,
    loopAvailable: false,
    computerUseEnabled: false,
    computerUseAvailable: false,
    yoloEnabled: false,
    reasoningEffort: null,
    autoModelEnabled: false,
    autoRoute: null,
    lastUsage: null,
    sessionUsage: null,
    currentSessionIndex: null,
    currentSessionActiveCheckpoint: null,
    currentSessionCheckpoints: [],
    checkpointPanelSessionIndex: null,
    checkpointPanelEntries: [],
    currentMode: 'build',
    multiAgentEnabled: false,
    oneShotEnabled: false,
    showDetailedAgentLogs: true,
    planRemaining: -1,
    quickOpenVisible: false,
    inputValue: '',
  };
}
