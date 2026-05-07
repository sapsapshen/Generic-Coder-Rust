import type {
  BootstrapPayload,
  ChangeEntry,
  ModelsPayload,
  RemoteState,
  SessionEntry,
  SessionCheckpointEntry,
  SkillEntry,
  TaskStatus,
  WorkspaceEntry,
  WorkspacePreview,
  WorkspaceState,
} from './types';

type JsonValue = Record<string, unknown>;

async function readJson<T>(response: Response): Promise<T> {
  const payload = (await response.json()) as T & { error?: string };
  if (!response.ok) {
    throw new Error((payload as { error?: string }).error || `HTTP ${response.status}`);
  }
  return payload;
}

export class ApiClient {
  async bootstrap(): Promise<BootstrapPayload> {
    const response = await fetch('/api/bootstrap');
    return readJson<BootstrapPayload>(response);
  }

  async models(): Promise<ModelsPayload> {
    const response = await fetch('/api/models');
    return readJson<ModelsPayload>(response);
  }

  async settings(): Promise<BootstrapPayload> {
    const response = await fetch('/api/settings');
    return readJson<BootstrapPayload>(response);
  }

  async setTheme(theme: string): Promise<void> {
    const response = await fetch('/api/theme', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ theme }),
    });
    await readJson<JsonValue>(response);
  }

  async setModel(index: number): Promise<{ current_index: number; model: string }> {
    const response = await fetch('/api/model', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ index }),
    });
    return readJson<{ current_index: number; model: string }>(response);
  }

  async saveLlmConfig(payload: JsonValue): Promise<{ model: string }> {
    const response = await fetch('/api/llm-config', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
    });
    return readJson<{ model: string }>(response);
  }

  async saveWorkspace(payload: { name: string; path: string }): Promise<WorkspaceState> {
    const response = await fetch('/api/workspace', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
    });
    return readJson<WorkspaceState>(response);
  }

  async pickerToken(): Promise<string> {
    const response = await fetch('/api/workspace/picker-token');
    const payload = await readJson<{ token: string }>(response);
    return payload.token || '';
  }

  async pickWorkspace(token: string): Promise<{ path: string | null; cancelled?: boolean }> {
    const response = await fetch('/api/workspace/pick', {
      method: 'POST',
      headers: {
        'X-Generic-Coder-UI': '1',
        'X-Generic-Coder-Picker-Token': token,
      },
    });
    return readJson<{ path: string | null; cancelled?: boolean }>(response);
  }

  async connectRemote(payload: JsonValue): Promise<RemoteState & { message?: string }> {
    const response = await fetch('/api/remote/connect', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
    });
    return readJson<RemoteState & { message?: string }>(response);
  }

  async workspaceTree(): Promise<WorkspaceEntry[]> {
    const response = await fetch('/api/workspace/tree');
    const payload = await readJson<{ tree?: WorkspaceEntry[] }>(response);
    return payload.tree || [];
  }

  async workspaceFiles(query: string, limit = 20): Promise<Array<{ name: string; path: string; rel: string }>> {
    const response = await fetch(`/api/workspace/files?q=${encodeURIComponent(query)}&limit=${limit}`);
    const payload = await readJson<{ files?: Array<{ name: string; path: string; rel: string }> }>(response);
    return payload.files || [];
  }

  async workspacePreview(filePath: string): Promise<WorkspacePreview> {
    const response = await fetch(`/api/workspace/preview?path=${encodeURIComponent(filePath)}`);
    return readJson<WorkspacePreview>(response);
  }

  workspacePreviewContentUrl(filePath: string): string {
    return `/api/workspace/preview-content?path=${encodeURIComponent(filePath)}`;
  }

  async sessions(): Promise<SessionEntry[]> {
    const response = await fetch('/api/sessions');
    const payload = await readJson<{ sessions?: SessionEntry[] }>(response);
    return payload.sessions || [];
  }

  async sessionCheckpoints(index: number): Promise<SessionCheckpointEntry[]> {
    const response = await fetch(`/api/sessions/${index}/checkpoints`);
    const payload = await readJson<{ checkpoints?: SessionCheckpointEntry[] }>(response);
    return payload.checkpoints || [];
  }

  async restoreSession(index: number, checkpoint?: number): Promise<{ index: number; checkpoint?: number; messages: unknown[] }> {
    const response = await fetch('/api/sessions/restore', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ index, checkpoint }),
    });
    return readJson<{ index: number; checkpoint?: number; messages: unknown[] }>(response);
  }

  async forkSession(index: number, checkpoint?: number): Promise<{ index: number; messages: unknown[] }> {
    const response = await fetch('/api/sessions/fork', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ index, checkpoint }),
    });
    return readJson<{ index: number; messages: unknown[] }>(response);
  }

  async deleteSession(index: number): Promise<{ index: number; was_active: boolean }> {
    const response = await fetch(`/api/sessions/${index}/delete`, { method: 'POST' });
    return readJson<{ index: number; was_active: boolean }>(response);
  }

  async changes(): Promise<ChangeEntry[]> {
    const response = await fetch('/api/changes');
    const payload = await readJson<{ changes?: ChangeEntry[] }>(response);
    return payload.changes || [];
  }

  async diff(filePath: string): Promise<{ has_changes: boolean; diff: string }> {
    const response = await fetch('/api/diff', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ path: filePath }),
    });
    return readJson<{ has_changes: boolean; diff: string }>(response);
  }

  async revert(filePath: string): Promise<void> {
    const response = await fetch('/api/revert', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ path: filePath }),
    });
    await readJson<JsonValue>(response);
  }

  async skills(): Promise<SkillEntry[]> {
    const response = await fetch('/api/skills');
    const payload = await readJson<{ skills?: SkillEntry[] }>(response);
    return Array.isArray(payload.skills) ? payload.skills : [];
  }

  async installSkill(url: string): Promise<void> {
    const response = await fetch('/api/skills/install', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ url }),
    });
    await readJson<JsonValue>(response);
  }

  async toggleSkill(name: string): Promise<void> {
    const response = await fetch(`/api/skills/${encodeURIComponent(name)}/toggle`, { method: 'POST' });
    await readJson<JsonValue>(response);
  }

  async deleteSkill(name: string): Promise<void> {
    const response = await fetch(`/api/skills/${encodeURIComponent(name)}/delete`, { method: 'POST' });
    await readJson<JsonValue>(response);
  }

  async upgradeSkill(name: string): Promise<void> {
    const response = await fetch(`/api/skills/${encodeURIComponent(name)}/upgrade`, { method: 'POST' });
    await readJson<JsonValue>(response);
  }

  async previewSkill(name: string): Promise<{ file?: string; content?: string }> {
    const response = await fetch(`/api/skills/${encodeURIComponent(name)}/preview`);
    return readJson<{ file?: string; content?: string }>(response);
  }

  async workflow(): Promise<{ nodes?: Array<{ mode: 'work' | 'plan' | 'review'; label?: string; completed?: boolean }>; active?: boolean; current_node?: number }> {
    const response = await fetch('/api/workflow');
    return readJson<{ nodes?: Array<{ mode: 'work' | 'plan' | 'review'; label?: string; completed?: boolean }>; active?: boolean; current_node?: number }>(response);
  }

  async saveWorkflow(nodes: Array<{ mode: 'work' | 'plan' | 'review'; label?: string }>): Promise<void> {
    const response = await fetch('/api/workflow', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ nodes }),
    });
    await readJson<JsonValue>(response);
  }

  async resetWorkflow(): Promise<void> {
    const response = await fetch('/api/workflow/reset', { method: 'POST' });
    await readJson<JsonValue>(response);
  }

  async getWorkflowFollow(): Promise<{ enabled: boolean }> {
    const response = await fetch('/api/workflow/follow');
    return readJson<{ enabled: boolean }>(response);
  }

  async setWorkflowFollow(enabled: boolean): Promise<{ ok: boolean; reason?: string }> {
    const response = await fetch('/api/workflow/follow', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ enabled }),
    });
    return readJson<{ ok: boolean; reason?: string }>(response);
  }

  async checkLoopSuitable(prompt: string): Promise<{ suitable: boolean; reason?: string }> {
    const response = await fetch('/api/loop/suitable', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ prompt }),
    });
    return readJson<{ suitable: boolean; reason?: string }>(response);
  }

  async setLoop(enabled: boolean): Promise<void> {
    const response = await fetch('/api/loop', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ enabled }),
    });
    await readJson<JsonValue>(response);
  }

  async mode(): Promise<'work' | 'plan' | 'review'> {
    const response = await fetch('/api/mode');
    const payload = await readJson<{ mode: 'work' | 'plan' | 'review' }>(response);
    return payload.mode;
  }

  async setMode(mode: 'work' | 'plan' | 'review'): Promise<void> {
    const response = await fetch('/api/mode', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ mode }),
    });
    await readJson<JsonValue>(response);
  }

  async setMultiAgent(enabled: boolean): Promise<void> {
    const response = await fetch('/api/multi-agent', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ enabled }),
    });
    await readJson<JsonValue>(response);
  }

  async checkMultiAgentSuitable(prompt: string): Promise<{ suitable: boolean; reason?: string }> {
    const response = await fetch('/api/multi-agent/suitable', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ prompt }),
    });
    return readJson<{ suitable: boolean; reason?: string }>(response);
  }

  async setOneShot(enabled: boolean): Promise<void> {
    const response = await fetch('/api/one-shot', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ enabled }),
    });
    await readJson<JsonValue>(response);
  }

  async setComputerUse(enabled: boolean): Promise<{ enabled: boolean }> {
    const response = await fetch('/api/computer-use', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ enabled }),
    });
    return readJson<{ enabled: boolean }>(response);
  }

  async setYolo(enabled: boolean): Promise<{ ok: boolean; enabled: boolean }> {
    const response = await fetch('/api/yolo', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ enabled }),
    });
    return readJson<{ ok: boolean; enabled: boolean }>(response);
  }

  async setAutoModel(enabled: boolean): Promise<{ ok: boolean; enabled: boolean }> {
    const response = await fetch('/api/auto-model', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ enabled }),
    });
    return readJson<{ ok: boolean; enabled: boolean }>(response);
  }

  async setReasoningEffort(effort: string | null): Promise<{ ok: boolean; effort: string | null }> {
    const response = await fetch('/api/reasoning-effort', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ effort }),
    });
    return readJson<{ ok: boolean; effort: string | null }>(response);
  }

  async chat(prompt: string): Promise<{ handled?: boolean; messages?: unknown[]; notice?: string; task_id?: string; error?: string }> {
    const response = await fetch('/api/chat', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ prompt }),
    });
    return readJson<{ handled?: boolean; messages?: unknown[]; notice?: string; task_id?: string; error?: string }>(response);
  }

  async task(taskId: string): Promise<TaskStatus> {
    const response = await fetch(`/api/tasks/${taskId}`);
    return readJson<TaskStatus>(response);
  }

  async stop(): Promise<void> {
    const response = await fetch('/api/stop', { method: 'POST' });
    await readJson<JsonValue>(response);
  }

  async planStatus(): Promise<{ in_plan: boolean; plan_path: string; remaining: number }> {
    const response = await fetch('/api/plan/status');
    return readJson<{ in_plan: boolean; plan_path: string; remaining: number }>(response);
  }
}
