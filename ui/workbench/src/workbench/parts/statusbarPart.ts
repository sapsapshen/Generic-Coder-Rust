import { Disposable } from '../../base/common/lifecycle';
import type { ServicesAccessor } from '../../platform/instantiation/common/instantiationService';
import { LayoutService } from '../services/layoutService';
import { ILayoutService, IWorkbenchService } from '../services/serviceIds';
import { WorkbenchService } from '../services/workbenchService';

// DeepSeek model pricing per 1M tokens (USD)
const MODEL_PRICING: Record<string, { cacheHit: number; cacheMiss: number; output: number }> = {
  'deepseek-v4-pro': { cacheHit: 0.003625, cacheMiss: 0.435, output: 0.87 },
  'deepseek-v4-flash': { cacheHit: 0.0028, cacheMiss: 0.14, output: 0.28 },
  'deepseek-reasoner': { cacheHit: 0.003625, cacheMiss: 0.435, output: 0.87 },
  'deepseek-chat': { cacheHit: 0.0028, cacheMiss: 0.14, output: 0.28 },
};

function estimateCost(
  model: string,
  promptTokens: number,
  completionTokens: number,
  cachedTokens: number,
): string {
  const lm = model.toLowerCase();
  const pricing = Object.entries(MODEL_PRICING).find(([k]) => lm.includes(k))?.[1];
  if (!pricing) return '';
  const missTokens = promptTokens - cachedTokens;
  const cost =
    (cachedTokens / 1_000_000) * pricing.cacheHit +
    (missTokens / 1_000_000) * pricing.cacheMiss +
    (completionTokens / 1_000_000) * pricing.output;
  return cost < 0.001 ? `<$0.001` : `$${cost.toFixed(4)}`;
}

export class StatusbarPart extends Disposable {
  private readonly layoutService: LayoutService;
  private readonly workbenchService: WorkbenchService;

  constructor(accessor: ServicesAccessor) {
    super();
    this.layoutService = accessor.get(ILayoutService);
    this.workbenchService = accessor.get(IWorkbenchService);
    this._register(this.workbenchService.onDidChangeState(() => this.render()));
    this.render();
  }

  render(): void {
    const state = this.workbenchService.state;
    const effectiveModel = state.autoRoute?.model || state.modelLabel;
    this.layoutService.getElement<HTMLSpanElement>('status-model').textContent = state.autoModelEnabled
      ? state.autoRoute
        ? `Auto -> ${state.autoRoute.model}${state.autoRoute.reasoning_effort ? ` / ${state.autoRoute.reasoning_effort}` : ''}`
        : `Auto (${state.modelLabel})`
      : state.modelLabel;
    this.layoutService.getElement<HTMLSpanElement>('status-workspace').textContent = state.workspace.active?.path || 'No workspace';
    this.layoutService.getElement<HTMLSpanElement>('status-mode').textContent = state.currentMode.toUpperCase();
    this.layoutService.getElement<HTMLSpanElement>('status-plan').textContent =
      state.planRemaining >= 0 ? `Plan: ${state.planRemaining} remaining` : 'Plan: idle';
    this.layoutService.getElement<HTMLSpanElement>('status-run').textContent = state.isRunning ? 'Running' : 'Idle';

    const usageEl = this.layoutService.getElement<HTMLSpanElement>('status-usage');
    if (state.lastUsage) {
      const { prompt_tokens, completion_tokens, cached_tokens } = state.lastUsage;
      const cached = cached_tokens ?? 0;
      const costStr = estimateCost(effectiveModel, prompt_tokens, completion_tokens, cached);
      const tokStr = `↑${prompt_tokens} ↓${completion_tokens}${cached ? ` 💾${cached}` : ''}`;
      usageEl.textContent = costStr ? `${tokStr} ${costStr}` : tokStr;
      usageEl.title = `Prompt: ${prompt_tokens} | Completion: ${completion_tokens} | Cached: ${cached}`;
    } else {
      usageEl.textContent = '';
      usageEl.title = '';
    }
  }
}
