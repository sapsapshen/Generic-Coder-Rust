// ============================================================
// Generic Coder — Electron UI  |  Application
// ============================================================

(function () {
  'use strict';

  // ── Constants ──────────────────────────────────────────────────
  const THEME_OPTIONS = ['solarflare', 'graphite', 'daybreak', 'paperink', 'obsidian', 'slate', 'oxide', 'noir', 'cobalt', 'borealis'];
  const SESSION_TYPE_OPTIONS = ['native_oai', 'oai', 'native_claude', 'claude'];
  const PROTOCOL_PRESET_OPTIONS = [
    'custom', 'deepseek', 'qwen_dashscope', 'openai_chat', 'openai_responses',
    'anthropic_messages', 'openrouter', 'moonshot_oai', 'minimax_oai',
    'doubao_ark', 'hunyuan_oai', 'baidu_qianfan', 'zhipu_anthropic',
  ];
  const MODEL_PRESET_OPTIONS = [
    'custom', 'deepseek_v4_pro', 'deepseek_v4_flash', 'qwen_max_latest',
    'qwen_plus_latest', 'qwen3_coder_plus', 'qwen3_coder_flash',
    'kimi_k26', 'kimi_k25', 'minimax_m27', 'minimax_m1', 'glm_5_1',
    'doubao_15_pro_32k', 'hunyuan_turbos_latest', 'ernie_45_turbo_128k',
    'mimo_v25', 'gpt_5_4', 'gpt_4_1', 'claude_sonnet_4', 'claude_opus_4_7',
    'openrouter_claude',
  ];

  const MAX_INPUT_HISTORY = 50;

  const PROTOCOL_PRESETS = {
    custom: { sessionType: 'native_oai', api_mode: 'chat_completions', provider: '', apibase: '' },
    deepseek: { sessionType: 'native_oai', api_mode: 'chat_completions', provider: 'DeepSeek', apibase: 'https://api.deepseek.com/v1' },
    qwen_dashscope: { sessionType: 'native_oai', api_mode: 'chat_completions', provider: 'Qwen', apibase: 'https://dashscope.aliyuncs.com/compatible-mode/v1' },
    openai_chat: { sessionType: 'native_oai', api_mode: 'chat_completions', provider: 'OpenAI', apibase: 'https://api.openai.com/v1' },
    openai_responses: { sessionType: 'native_oai', api_mode: 'responses', provider: 'OpenAI', apibase: 'https://api.openai.com/v1' },
    anthropic_messages: { sessionType: 'native_claude', api_mode: 'chat_completions', provider: 'Anthropic', apibase: 'https://api.anthropic.com' },
    openrouter: { sessionType: 'oai', api_mode: 'chat_completions', provider: 'OpenRouter', apibase: 'https://openrouter.ai/api/v1' },
    moonshot_oai: { sessionType: 'oai', api_mode: 'chat_completions', provider: 'Kimi', apibase: 'https://api.moonshot.ai/v1' },
    minimax_oai: { sessionType: 'oai', api_mode: 'chat_completions', provider: 'MiniMax', apibase: 'https://api.minimaxi.com/v1' },
    doubao_ark: { sessionType: 'oai', api_mode: 'chat_completions', provider: 'Doubao', apibase: 'https://ark.cn-beijing.volces.com/api/v3' },
    hunyuan_oai: { sessionType: 'oai', api_mode: 'chat_completions', provider: 'Hunyuan', apibase: 'https://api.hunyuan.cloud.tencent.com/v1' },
    baidu_qianfan: { sessionType: 'oai', api_mode: 'chat_completions', provider: 'ERNIE', apibase: 'https://qianfan.baidubce.com/v2' },
    zhipu_anthropic: { sessionType: 'native_claude', api_mode: 'chat_completions', provider: 'Zhipu', apibase: 'https://open.bigmodel.cn/api/anthropic' },
  };

  const MODEL_PRESETS = {
    custom: {},
    deepseek_v4_pro: { provider: 'DeepSeek', model: 'deepseek-v4-pro', displayName: 'deepseek-v4-pro', protocolPreset: 'deepseek' },
    deepseek_v4_flash: { provider: 'DeepSeek', model: 'deepseek-v4-flash', displayName: 'deepseek-v4-flash', protocolPreset: 'deepseek' },
    qwen_max_latest: { provider: 'Qwen', model: 'qwen-max-latest', displayName: 'qwen-max-latest', protocolPreset: 'qwen_dashscope' },
    qwen_plus_latest: { provider: 'Qwen', model: 'qwen-plus-latest', displayName: 'qwen-plus-latest', protocolPreset: 'qwen_dashscope' },
    qwen3_coder_plus: { provider: 'Qwen', model: 'qwen3-coder-plus', displayName: 'qwen3-coder-plus', protocolPreset: 'qwen_dashscope' },
    qwen3_coder_flash: { provider: 'Qwen', model: 'qwen3-coder-flash', displayName: 'qwen3-coder-flash', protocolPreset: 'qwen_dashscope' },
    kimi_k26: { provider: 'Kimi', model: 'kimi-k2.6', displayName: 'kimi-k2.6', protocolPreset: 'moonshot_oai' },
    kimi_k25: { provider: 'Kimi', model: 'kimi-k2.5', displayName: 'kimi-k2.5', protocolPreset: 'moonshot_oai' },
    gpt_5_4: { provider: 'OpenAI', model: 'gpt-5.4', displayName: 'gpt-5.4', protocolPreset: 'openai_responses' },
    gpt_4_1: { provider: 'OpenAI', model: 'gpt-4.1', displayName: 'gpt-4.1', protocolPreset: 'openai_chat' },
    claude_sonnet_4: { provider: 'Anthropic', model: 'claude-sonnet-4-20250514', displayName: 'claude-sonnet-4', protocolPreset: 'anthropic_messages' },
    claude_opus_4_7: { provider: 'Anthropic', model: 'claude-opus-4-7', displayName: 'claude-opus-4-7', protocolPreset: 'anthropic_messages' },
    minimax_m27: { provider: 'MiniMax', model: 'MiniMax-M2.7', displayName: 'MiniMax-M2.7', protocolPreset: 'minimax_oai' },
    minimax_m1: { provider: 'MiniMax', model: 'MiniMax-M1', displayName: 'MiniMax-M1', protocolPreset: 'minimax_oai' },
    glm_5_1: { provider: 'Zhipu', model: 'glm-5.1', displayName: 'glm-5.1', protocolPreset: 'zhipu_anthropic' },
    doubao_15_pro_32k: { provider: 'Doubao', model: 'doubao-1.5-pro-32k', displayName: 'doubao-1.5-pro-32k', protocolPreset: 'doubao_ark' },
    hunyuan_turbos_latest: { provider: 'Hunyuan', model: 'hunyuan-turbos-latest', displayName: 'hunyuan-turbos-latest', protocolPreset: 'hunyuan_oai' },
    ernie_45_turbo_128k: { provider: 'ERNIE', model: 'ernie-4.5-turbo-128k', displayName: 'ernie-4.5-turbo-128k', protocolPreset: 'baidu_qianfan' },
    mimo_v25: { provider: 'Xiaomi', model: 'mimo-v2.5', displayName: 'mimo-v2.5' },
    openrouter_claude: { provider: 'OpenRouter', model: 'anthropic/claude-opus-4-7', displayName: 'openrouter-claude', protocolPreset: 'openrouter' },
  };

  const PROTOCOL_PRESET_LABELS = {
    custom: 'Manual entry',
    deepseek: 'DeepSeek OAI',
    qwen_dashscope: 'Qwen / DashScope',
    openai_chat: 'OpenAI Chat Completions',
    openai_responses: 'OpenAI Responses',
    anthropic_messages: 'Anthropic Messages',
    openrouter: 'OpenRouter OAI',
    moonshot_oai: 'Kimi / Moonshot API',
    minimax_oai: 'MiniMax OAI',
    doubao_ark: 'Doubao / Ark API',
    hunyuan_oai: 'Tencent Hunyuan OAI',
    baidu_qianfan: 'Baidu Qianfan OAI',
    zhipu_anthropic: 'Zhipu Anthropic Compatible',
  };

  const MODEL_PRESET_LABELS = {
    custom: 'Manual entry',
    deepseek_v4_pro: 'DeepSeek V4 Pro',
    deepseek_v4_flash: 'DeepSeek V4 Flash',
    qwen_max_latest: 'Qwen Max Latest',
    qwen_plus_latest: 'Qwen Plus Latest',
    qwen3_coder_plus: 'Qwen3 Coder Plus',
    qwen3_coder_flash: 'Qwen3 Coder Flash',
    kimi_k26: 'Kimi K2.6',
    kimi_k25: 'Kimi K2.5',
    minimax_m27: 'MiniMax M2.7',
    minimax_m1: 'MiniMax M1',
    glm_5_1: 'GLM 5.1',
    doubao_15_pro_32k: 'Doubao 1.5 Pro 32K',
    hunyuan_turbos_latest: 'Hunyuan Turbo S Latest',
    ernie_45_turbo_128k: 'ERNIE 4.5 Turbo 128K',
    mimo_v25: 'MiMo v2.5',
    gpt_5_4: 'GPT-5.4',
    gpt_4_1: 'GPT-4.1',
    claude_sonnet_4: 'Claude Sonnet 4',
    claude_opus_4_7: 'Claude Opus 4.7',
    openrouter_claude: 'OpenRouter Claude Opus',
  };

  const THEME_LABELS = {
    solarflare: 'Solar Flare', graphite: 'Graphite', daybreak: 'Daybreak',
    paperink: 'Paper Ink', obsidian: 'Obsidian', slate: 'Slate',
    oxide: 'Oxide', noir: 'Noir', cobalt: 'Cobalt', borealis: 'Borealis',
  };

  const SESSION_TYPE_LABELS = {
    native_oai: 'Native OAI', oai: 'OAI', native_claude: 'Native Claude', claude: 'Claude',
  };

  // ── State ────────────────────────────────────────────────────
  const state = {
    serverUrl: 'http://127.0.0.1:8765',
    wsUrl: 'ws://127.0.0.1:8765/ws',
    model: '',
    temperature: 0.7,
    maxTokens: 16000,
    fontSize: 16,
    theme: 'solarflare',
    sessions: [],
    activeSessionId: null,
    messages: [],
    ws: null,
    isStreaming: false,
    streamingMessageEl: null,
    streamingThinkingEl: null,
    streamingToolEls: [],
    backendConnected: false,
    multiAgentEnabled: false,
    oneShotEnabled: false,
    workflowMode: 'work',
    computerUseEnabled: true,
    sidebarVisible: true,
    workspaceFiles: [],
    workspacePreviewPath: null,
    gitChanges: [],
    skills: [],
    pendingTaskId: null,
    attachFileName: null,
    attachFileContent: null,
    inputHistory: [],
    inputHistoryIndex: -1,
    pendingDraft: null,
    previewMessageSeq: 0,
    cmdPaletteIdx: -1,
    // LLM form state (for settings)
    llmForm: {
      entry_key: 'generic_coder_electron',
      session_type: 'native_oai',
      protocol_preset: 'custom',
      api_mode: 'chat_completions',
      provider: '',
      name: '',
      apikey: '',
      apibase: '',
      model: '',
    },
  };

  // ── DOM Refs ─────────────────────────────────────────────────
  const $ = (s) => document.querySelector(s);
  const $$ = (s) => document.querySelectorAll(s);

  const dom = {
    messages: $('#messages'),
    chatInput: $('#chat-input'),
    btnSend: $('#btn-send'),
    btnStop: $('#btn-stop'),
    btnAttach: $('#btn-attach'),
    attachFileInput: $('#attach-file-input'),
    attachPreview: $('#attach-preview'),
    attachPreviewName: $('#attach-preview-name'),
    attachPreviewRemove: $('#attach-preview-remove'),
    sessionList: $('#session-list'),
    btnNewSession: $('#btn-new-session'),
    tbSession: $('#tb-session'),
    statusModel: $('#status-model'),
    statusTokens: $('#status-tokens'),
    welcome: $('#chat-welcome'),
    chatContainer: $('#chat-container'),
    toolbarMode: $('#toolbar-mode'),
    toolbarTurn: $('#toolbar-turn'),
    // Overlays
    settingsOverlay: $('#settings-overlay'),
    btnSettings: $('#btn-settings'),
    btnCloseSettings: $('#btn-close-settings'),
    cmdPaletteOverlay: $('#cmd-palette-overlay'),
    cmdPaletteInput: $('#cmd-palette-input'),
    cmdPaletteResults: $('#cmd-palette-results'),
    btnCommandPalette: $('#btn-command-palette'),
    // Settings tabs
    settingsTabs: $('#settings-tabs'),
    // Settings — Model
    modelPresetSelect: $('#model-preset-select'),
    protocolPresetSelect: $('#protocol-preset-select'),
    modelSelect: $('#model-select'),
    sessionTypeSelect: $('#session-type-select'),
    providerInput: $('#provider-input'),
    displayNameInput: $('#display-name-input'),
    modelNameInput: $('#model-name-input'),
    baseUrlInput: $('#base-url-input'),
    apiKeyInput: $('#api-key-input'),
    apiKeyToggle: $('#api-key-toggle'),
    saveLlmButton: $('#save-llm-settings'),
    // Settings — Appearance
    themeSelect: $('#theme-select'),
    cfgFontSize: $('#cfg-font-size'),
    // Settings — Workspace
    cfgProjectDir: $('#cfg-project-dir'),
    btnPickFolder: $('#btn-pick-folder'),
    btnApplyWorkspace: $('#btn-apply-workspace'),
    // Settings — Skills
    skillsList: $('#skills-list'),
    skillsStatus: $('#skills-status'),
    skillUrlInput: $('#skill-url-input'),
    installSkillButton: $('#install-skill-button'),
    // Sidebar
    btnToggleSidebar: $('#btn-toggle-sidebar'),
    sidebar: $('#sidebar'),
    workspaceTree: $('#workspace-tree'),
    changesSection: $('#changes-section'),
    changesList: $('#changes-list'),
    changesCount: $('#changes-count'),
    btnRefreshWorkspace: $('#btn-refresh-workspace'),
    // Toggles
    toggleMultiAgent: $('#toggle-multi-agent'),
    toggleOneShot: $('#toggle-one-shot'),
    toggleComputerUse: $('#toggle-computer-use'),
    // Workflow
    workflowPipeline: $('#workflow-pipeline'),
    // Toast
    toastContainer: $('#toast-container'),
    inputContext: $('#input-context'),
    statusHint: $('#status-hint'),
  };

  function normalizeServerUrl() {
    try {
      const u = new URL(state.serverUrl);
      if (u.hostname === 'localhost') u.hostname = '127.0.0.1';
      state.serverUrl = u.toString().replace(/\/$/, '');
    } catch (e) { /* keep as-is */ }
  }

  // ── Init ─────────────────────────────────────────────────────
  async function init() {
    loadSettings();
    normalizeServerUrl();
    if (window.electronAPI && window.electronAPI.getBackendUrl) {
      try {
        const url = await window.electronAPI.getBackendUrl();
        if (url && typeof url === 'string') state.serverUrl = url.replace(/\/$/, '');
      } catch (e) { /* ignore */ }
    }
    applySettings();
    renderAllSelects();
    bindEvents();
    await connectBackend();
    ensureActiveSession();
    bindHintChips();
  }

  // ── Settings Persistence ─────────────────────────────────────
  function loadSettings() {
    try {
      const saved = JSON.parse(localStorage.getItem('gc-ui-settings-v2') || '{}');
      if (saved.serverUrl) state.serverUrl = saved.serverUrl;
      if (saved.model) state.model = saved.model;
      if (saved.temperature !== undefined) state.temperature = saved.temperature;
      if (saved.maxTokens) state.maxTokens = saved.maxTokens;
      if (saved.fontSize) state.fontSize = saved.fontSize;
      if (saved.theme) state.theme = saved.theme;
      if (saved.sessions) state.sessions = saved.sessions;
      if (saved.activeSessionId) state.activeSessionId = saved.activeSessionId;
      if (saved.multiAgentEnabled !== undefined) state.multiAgentEnabled = saved.multiAgentEnabled;
      if (saved.oneShotEnabled !== undefined) state.oneShotEnabled = saved.oneShotEnabled;
      if (saved.computerUseEnabled !== undefined) state.computerUseEnabled = saved.computerUseEnabled;
      if (saved.workflowMode) state.workflowMode = saved.workflowMode;
      if (saved.sidebarVisible !== undefined) state.sidebarVisible = saved.sidebarVisible;
      // Restore LLM form (the full model configuration)
      if (saved.llmForm) {
        state.llmForm = { ...state.llmForm, ...saved.llmForm };
      }
      normalizeStoredSessions();
    } catch (e) { /* ignore */ }
  }

  function saveSettings() {
    localStorage.setItem('gc-ui-settings-v2', JSON.stringify({
      serverUrl: state.serverUrl,
      model: state.model,
      temperature: state.temperature,
      maxTokens: state.maxTokens,
      fontSize: state.fontSize,
      theme: state.theme,
      sessions: state.sessions,
      activeSessionId: state.activeSessionId,
      multiAgentEnabled: state.multiAgentEnabled,
      oneShotEnabled: state.oneShotEnabled,
      computerUseEnabled: state.computerUseEnabled,
      workflowMode: state.workflowMode,
      sidebarVisible: state.sidebarVisible,
      llmForm: state.llmForm,
    }));
  }

  function applySettings() {
    setTheme(state.theme, false);
    document.documentElement.style.setProperty('font-size', state.fontSize + 'px');

    if (dom.cfgFontSize) dom.cfgFontSize.value = state.fontSize;
    dom.statusModel.textContent = state.model || 'default';

    dom.toggleMultiAgent.checked = state.multiAgentEnabled;
    dom.toggleOneShot.checked = state.oneShotEnabled;
    if (dom.toggleComputerUse) dom.toggleComputerUse.checked = state.computerUseEnabled;
    dom.toolbarMode.textContent = state.workflowMode.charAt(0).toUpperCase() + state.workflowMode.slice(1);

    updateSidebarVisibility();

    $$('.workflow-node').forEach(n => {
      n.classList.toggle('active', n.dataset.mode === state.workflowMode);
    });

    state.wsUrl = state.serverUrl.replace(/^http/, 'ws') + '/ws';

    // Hydrate LLM form fields in settings
    hydrateLlmFormFields();
  }

  function updateSidebarVisibility() {
    dom.sidebar.classList.toggle('collapsed', !state.sidebarVisible);
    dom.btnToggleSidebar.textContent = state.sidebarVisible ? '\u2630' : '\u2630';
  }

  // ── Events ───────────────────────────────────────────────────
  function bindEvents() {
    // Chat
    dom.btnSend.addEventListener('click', handleSend);
    dom.btnStop.addEventListener('click', handleStop);
    dom.chatInput.addEventListener('keydown', handleInputKey);
    dom.chatInput.addEventListener('input', autoResizeInput);
    dom.btnAttach.addEventListener('click', () => dom.attachFileInput.click());
    dom.attachFileInput.addEventListener('change', handleAttachFile);
    dom.attachPreviewRemove.addEventListener('click', clearAttachment);

    // Session
    dom.btnNewSession.addEventListener('click', createNewSession);

    // Settings
    dom.btnSettings.addEventListener('click', () => openSettings());
    dom.btnCloseSettings.addEventListener('click', () => closeSettings());
    dom.settingsOverlay.addEventListener('click', (e) => {
      if (e.target === dom.settingsOverlay) closeSettings();
    });

    // Settings tabs
    dom.settingsTabs.querySelectorAll('.settings-tab').forEach(tab => {
      tab.addEventListener('click', () => switchSettingsTab(tab.dataset.tab));
    });

    // Settings — Model
    if (dom.modelPresetSelect) {
      dom.modelPresetSelect.addEventListener('change', () => applyModelPreset(dom.modelPresetSelect.value));
    }
    if (dom.protocolPresetSelect) {
      dom.protocolPresetSelect.addEventListener('change', () => applyProtocolPreset(dom.protocolPresetSelect.value));
    }
    if (dom.providerInput) dom.providerInput.addEventListener('input', () => { state.llmForm.provider = dom.providerInput.value; });
    if (dom.displayNameInput) dom.displayNameInput.addEventListener('input', () => { state.llmForm.name = dom.displayNameInput.value; });
    if (dom.modelNameInput) dom.modelNameInput.addEventListener('input', () => { state.llmForm.model = dom.modelNameInput.value; });
    if (dom.baseUrlInput) dom.baseUrlInput.addEventListener('input', () => { state.llmForm.apibase = dom.baseUrlInput.value; });
    if (dom.apiKeyInput) dom.apiKeyInput.addEventListener('input', () => { state.llmForm.apikey = dom.apiKeyInput.value; });
    if (dom.apiKeyToggle) dom.apiKeyToggle.addEventListener('click', toggleApiKey);
    if (dom.saveLlmButton) dom.saveLlmButton.addEventListener('click', saveLlmConfig);

    // Settings — Appearance
    if (dom.themeSelect) {
      dom.themeSelect.addEventListener('change', () => {
        setTheme(dom.themeSelect.value);
        saveSettings();
      });
    }
    if (dom.cfgFontSize) {
      dom.cfgFontSize.addEventListener('change', () => {
        state.fontSize = parseInt(dom.cfgFontSize.value);
        document.documentElement.style.setProperty('font-size', state.fontSize + 'px');
        saveSettings();
      });
    }

    // Settings — Workspace
    if (dom.btnPickFolder) dom.btnPickFolder.addEventListener('click', pickFolder);
    if (dom.btnApplyWorkspace) dom.btnApplyWorkspace.addEventListener('click', applyWorkspace);

    // Settings — Skills
    if (dom.installSkillButton) dom.installSkillButton.addEventListener('click', installSkill);

    // Toggles
    dom.toggleMultiAgent.addEventListener('change', () => toggleMultiAgent());
    dom.toggleOneShot.addEventListener('change', () => toggleOneShot());
    if (dom.toggleComputerUse) dom.toggleComputerUse.addEventListener('change', () => toggleComputerUse());

    // Workflow
    $$('.workflow-node').forEach(n => {
      n.addEventListener('click', () => setWorkflowMode(n.dataset.mode));
    });

    // Sidebar
    dom.btnToggleSidebar.addEventListener('click', toggleSidebar);
    dom.btnRefreshWorkspace.addEventListener('click', refreshWorkspace);
    dom.workspaceTree.addEventListener('click', handleWorkspaceTreeClick);
    dom.messages.addEventListener('click', handleMessageActionsClick);

    // Command Palette
    dom.btnCommandPalette.addEventListener('click', openCommandPalette);
    dom.cmdPaletteOverlay.addEventListener('click', (e) => {
      if (e.target === dom.cmdPaletteOverlay) closeCommandPalette();
    });
    dom.cmdPaletteInput.addEventListener('keydown', handleCmdPaletteKey);
    dom.cmdPaletteInput.addEventListener('input', filterCmdPalette);

    // Keyboard shortcuts
    document.addEventListener('keydown', handleGlobalKey);
  }

  // ── Render All Selects ───────────────────────────────────────
  function renderAllSelects() {
    renderThemeOptions();
    renderModelPresetOptions();
    renderProtocolPresetOptions();
    renderSessionTypeOptions();
  }

  function renderThemeOptions() {
    if (!dom.themeSelect) return;
    dom.themeSelect.innerHTML = THEME_OPTIONS.map(t =>
      `<option value="${t}">${THEME_LABELS[t] || t}</option>`
    ).join('');
    dom.themeSelect.value = state.theme;
  }

  function renderModelPresetOptions() {
    if (!dom.modelPresetSelect) return;
    dom.modelPresetSelect.innerHTML = MODEL_PRESET_OPTIONS.map(v =>
      `<option value="${v}">${MODEL_PRESET_LABELS[v] || v}</option>`
    ).join('');
    dom.modelPresetSelect.value = state.llmForm.model_preset || 'custom';
  }

  function renderProtocolPresetOptions() {
    if (!dom.protocolPresetSelect) return;
    dom.protocolPresetSelect.innerHTML = PROTOCOL_PRESET_OPTIONS.map(v =>
      `<option value="${v}">${PROTOCOL_PRESET_LABELS[v] || v}</option>`
    ).join('');
    dom.protocolPresetSelect.value = state.llmForm.protocol_preset || 'custom';
  }

  function renderSessionTypeOptions() {
    if (!dom.sessionTypeSelect) return;
    dom.sessionTypeSelect.innerHTML = SESSION_TYPE_OPTIONS.map(v =>
      `<option value="${v}">${SESSION_TYPE_LABELS[v] || v}</option>`
    ).join('');
    dom.sessionTypeSelect.value = state.llmForm.session_type || 'native_oai';
  }

  function hydrateLlmFormFields() {
    if (dom.providerInput) dom.providerInput.value = state.llmForm.provider || '';
    if (dom.displayNameInput) dom.displayNameInput.value = state.llmForm.name || '';
    if (dom.modelNameInput) dom.modelNameInput.value = state.llmForm.model || '';
    if (dom.baseUrlInput) dom.baseUrlInput.value = state.llmForm.apibase || '';
    if (dom.apiKeyInput) dom.apiKeyInput.value = state.llmForm.apikey || '';
    if (dom.sessionTypeSelect) dom.sessionTypeSelect.value = state.llmForm.session_type || 'native_oai';
    if (dom.protocolPresetSelect) dom.protocolPresetSelect.value = state.llmForm.protocol_preset || 'custom';
    if (dom.apiKeyToggle) dom.apiKeyToggle.textContent = 'Show';
    if (dom.apiKeyInput) dom.apiKeyInput.type = 'password';
  }

  // ── Model Preset Logic ───────────────────────────────────────
  function applyProtocolPreset(presetKey) {
    const preset = PROTOCOL_PRESETS[presetKey] || PROTOCOL_PRESETS.custom;
    state.llmForm.protocol_preset = presetKey;
    state.llmForm.api_mode = preset.api_mode || 'chat_completions';
    if (presetKey === 'custom') {
      renderProtocolPresetOptions();
      return;
    }
    if (dom.sessionTypeSelect) dom.sessionTypeSelect.value = preset.sessionType;
    state.llmForm.session_type = preset.sessionType;
    if (preset.provider && dom.providerInput) dom.providerInput.value = preset.provider;
    if (preset.apibase && dom.baseUrlInput) dom.baseUrlInput.value = preset.apibase;
    renderProtocolPresetOptions();
  }

  function applyModelPreset(presetKey) {
    const preset = MODEL_PRESETS[presetKey] || MODEL_PRESETS.custom;
    state.llmForm.model_preset = presetKey;
    if (presetKey === 'custom') {
      renderModelPresetOptions();
      return;
    }
    if (preset.protocolPreset) {
      applyProtocolPreset(preset.protocolPreset);
    }
    if (preset.provider && dom.providerInput) dom.providerInput.value = preset.provider;
    if (preset.model && dom.modelNameInput) dom.modelNameInput.value = preset.model;
    if (preset.displayName && dom.displayNameInput) dom.displayNameInput.value = preset.displayName;
    renderModelPresetOptions();
  }

  function toggleApiKey() {
    const input = dom.apiKeyInput;
    const btn = dom.apiKeyToggle;
    if (input.type === 'password') {
      input.type = 'text';
      btn.textContent = 'Hide';
    } else {
      input.type = 'password';
      btn.textContent = 'Show';
    }
  }

  async function saveLlmConfig() {
    const payload = {
      entry_key: state.llmForm.entry_key || 'generic_coder_electron',
      session_type: dom.sessionTypeSelect ? dom.sessionTypeSelect.value : state.llmForm.session_type,
      protocol_preset: dom.protocolPresetSelect ? dom.protocolPresetSelect.value : state.llmForm.protocol_preset,
      api_mode: state.llmForm.api_mode || 'chat_completions',
      provider: dom.providerInput ? dom.providerInput.value.trim() : '',
      name: dom.displayNameInput ? dom.displayNameInput.value.trim() : '',
      model: dom.modelNameInput ? dom.modelNameInput.value.trim() : '',
      apibase: dom.baseUrlInput ? dom.baseUrlInput.value.trim() : '',
      apikey: dom.apiKeyInput ? dom.apiKeyInput.value.trim() : '',
    };

    try {
      const res = await fetch(state.serverUrl + '/api/llm-config', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
      });
      const data = await res.json();
      if (!res.ok) {
        toast(data.error || 'Failed to save model config', 'error');
        return;
      }
      state.llmForm = { ...state.llmForm, ...payload };
      state.model = data.model || payload.model;
      dom.statusModel.textContent = state.model || 'default';
      saveSettings();
      toast('Model configuration saved', 'info');
    } catch (e) {
      toast('Save failed: ' + e.message, 'error');
    }
  }

  // ── Keyboard ─────────────────────────────────────────────────
  function handleInputKey(e) {
    if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      handleSend();
      return;
    }

    if (e.key === 'ArrowUp' && shouldRecallHistory()) {
      e.preventDefault();
      navigateInputHistory(-1);
      return;
    }

    if (e.key === 'ArrowDown' && state.inputHistoryIndex !== -1) {
      e.preventDefault();
      navigateInputHistory(1);
    }
  }

  function shouldRecallHistory() {
    if (!dom.chatInput || !state.inputHistory.length) return false;
    if (dom.chatInput.selectionStart !== dom.chatInput.selectionEnd) return false;
    const caret = dom.chatInput.selectionStart || 0;
    const beforeCaret = dom.chatInput.value.slice(0, caret);
    return !beforeCaret.includes('\n');
  }

  function snapshotCurrentDraft() {
    return {
      text: dom.chatInput.value,
      attachmentName: state.attachFileName,
      attachmentContent: state.attachFileContent,
    };
  }

  function pushInputHistory(entry) {
    if (!entry || !entry.text || !entry.text.trim()) return;

    const previous = state.inputHistory[state.inputHistory.length - 1];
    if (
      previous
      && previous.text === entry.text
      && previous.attachmentName === entry.attachmentName
      && previous.attachmentContent === entry.attachmentContent
    ) {
      state.inputHistoryIndex = -1;
      state.pendingDraft = null;
      return;
    }

    state.inputHistory.push({
      text: entry.text,
      attachmentName: entry.attachmentName || null,
      attachmentContent: entry.attachmentContent || null,
    });

    if (state.inputHistory.length > MAX_INPUT_HISTORY) {
      state.inputHistory.shift();
    }

    state.inputHistoryIndex = -1;
    state.pendingDraft = null;
  }

  function navigateInputHistory(direction) {
    if (!state.inputHistory.length) return;

    if (direction < 0) {
      if (state.inputHistoryIndex === -1) {
        state.pendingDraft = snapshotCurrentDraft();
        state.inputHistoryIndex = state.inputHistory.length - 1;
      } else {
        state.inputHistoryIndex = Math.max(0, state.inputHistoryIndex - 1);
      }
      applyDraftEntry(state.inputHistory[state.inputHistoryIndex]);
      return;
    }

    if (state.inputHistoryIndex === -1) return;

    const nextIndex = state.inputHistoryIndex + 1;
    if (nextIndex >= state.inputHistory.length) {
      state.inputHistoryIndex = -1;
      applyDraftEntry(state.pendingDraft);
      state.pendingDraft = null;
      return;
    }

    state.inputHistoryIndex = nextIndex;
    applyDraftEntry(state.inputHistory[state.inputHistoryIndex]);
  }

  function applyDraftEntry(entry) {
    dom.chatInput.value = entry?.text || '';
    autoResizeInput();

    if (entry?.attachmentName && entry?.attachmentContent) {
      setAttachmentPreview(entry.attachmentName, entry.attachmentContent);
    } else {
      clearAttachment();
    }

    dom.chatInput.focus();
    const end = dom.chatInput.value.length;
    dom.chatInput.setSelectionRange(end, end);
  }

  function handleGlobalKey(e) {
    const mod = e.metaKey || e.ctrlKey;
    if (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA') {
      if (e.key === 'Escape') {
        dom.chatInput.blur();
        dom.cmdPaletteOverlay.classList.remove('visible');
      }
      return;
    }
    if (e.key === 'Escape') {
      e.preventDefault();
      handleStop();
      dom.cmdPaletteOverlay.classList.remove('visible');
      closeSettings();
    }
    if ((mod && e.key === 'k') || (mod && e.key === 'p')) {
      e.preventDefault();
      openCommandPalette();
    }
    if (mod && e.key === 'b') { e.preventDefault(); toggleSidebar(); }
    if (mod && e.key === ',') { e.preventDefault(); openSettings(); }
    if (mod && e.key === 'n') { e.preventDefault(); createNewSession(); }
    if (mod && e.key === '1') { e.preventDefault(); setWorkflowMode('work'); }
    if (mod && e.key === '2') { e.preventDefault(); setWorkflowMode('plan'); }
    if (mod && e.key === '3') { e.preventDefault(); setWorkflowMode('review'); }
    if (mod && e.key === 'm') { e.preventDefault(); dom.toggleMultiAgent.click(); }
    if (mod && e.key === 't') { e.preventDefault(); cycleTheme(); }
    if (mod && e.key === 'l') { e.preventDefault(); clearChat(); }
  }

  function autoResizeInput() {
    dom.chatInput.style.height = 'auto';
    dom.chatInput.style.height = Math.min(dom.chatInput.scrollHeight, 200) + 'px';
  }

  // ── Backend Connection (HTTP — matches Rust /api/chat + task polling) ──
  async function connectBackend() {
    const maxRetries = 30;
    for (let i = 0; i < maxRetries; i++) {
      try {
        const resp = await fetch(state.serverUrl + '/health', { signal: AbortSignal.timeout(2000) });
        if (resp.ok) {
          state.backendConnected = true;
          await loadBackendState();
          return;
        }
      } catch (e) {
        /* retry */
      }
      await new Promise(r => setTimeout(r, 500 * Math.min(i + 1, 6)));
    }
    state.backendConnected = false;
  }

  function messagesFromServerPayload(messages) {
    if (!Array.isArray(messages)) return [];
    return messages.map((m) => {
      let content = m.content;
      if (typeof content !== 'string') {
        try {
          content = JSON.stringify(content);
        } catch (e) {
          content = String(content);
        }
      }
      return {
        role: m.role || 'assistant',
        content,
        timestamp: new Date().toISOString(),
      };
    });
  }

  async function pollTaskHttp(taskId) {
    try {
      while (state.pendingTaskId === taskId) {
        const res = await fetch(state.serverUrl + '/api/tasks/' + encodeURIComponent(taskId));
        const data = await res.json().catch(() => ({}));

        if (state.pendingTaskId !== taskId) return;

        const preview = data.preview || '';
        const lastIdx = state.messages.length - 1;
        if (lastIdx >= 0 && state.messages[lastIdx].role === 'assistant') {
          state.messages[lastIdx].content = preview || '...';
          if (state.streamingMessageEl) {
            const bodyEl = state.streamingMessageEl.querySelector('.message-body');
            if (bodyEl) bodyEl.innerHTML = formatContent(state.messages[lastIdx].content);
          }
        }

        scrollToBottom();

        if (data.done) {
          const finalText = data.final || data.preview || '';
          if (lastIdx >= 0 && state.messages[lastIdx].role === 'assistant') {
            state.messages[lastIdx].content = finalText;
            if (state.streamingMessageEl) {
              const bodyEl = state.streamingMessageEl.querySelector('.message-body');
              if (bodyEl) bodyEl.innerHTML = formatContent(finalText);
            }
          }
          state.pendingTaskId = null;
          finishStreaming();
          scrollToBottom();
          refreshWorkspace();
          return;
        }

        await new Promise(r => setTimeout(r, 700));
      }
    } catch (e) {
      addSystemMessage('任务轮询失败: ' + (e.message || String(e)));
      finishStreaming();
    }
  }

  async function loadBackendState() {
    try {
      // Load skills
      if (state.backendConnected) {
        await loadSkills();
      }

      // Load workspace
      if (state.backendConnected) {
        await refreshWorkspace();
      }
    } catch (e) { /* ignore */ }
  }

  // ── WebSocket Handler ────────────────────────────────────────
  function handleWsMessage(event) {
    let data;
    try { data = JSON.parse(event.data); } catch (e) { return; }

    switch (data.type) {
      case 'token':
        handleStreamToken(data);
        break;
      case 'thinking':
        handleThinkingDelta(data);
        break;
      case 'tool_start':
        handleToolStart(data);
        break;
      case 'tool_end':
        handleToolEnd(data);
        break;
      case 'done':
        finishStreaming();
        break;
      case 'error':
        handleStreamError(data);
        break;
      case 'session_created':
        handleSessionCreated(data);
        break;
      case 'usage':
        handleUsage(data);
        break;
      case 'acp_event':
        handleAcpEvent(data);
        break;
      case 'oneshot_event':
        handleOneShotEvent(data);
        break;
      case 'status':
        handleStatusEvent(data);
        break;
      default:
        if (data.content || data.text) {
          handleStreamToken(data);
        }
    }
  }

  function handleStatusEvent(data) {
    if (data.turn !== undefined) {
      dom.toolbarTurn.textContent = 'Turn ' + data.turn;
    }
  }

  // ── Session Management ───────────────────────────────────────
  function createNewSession() {
    const session = {
      id: 'sess_' + Date.now(),
      name: 'Session ' + (state.sessions.length + 1),
      messages: [],
      createdAt: new Date().toISOString(),
    };
    state.sessions.unshift(session);
    state.activeSessionId = session.id;
    state.messages = [];
    saveSettings();
    renderSessions();
    resetChat();
    dom.tbSession.textContent = '\u2014 ' + session.name + ' \u2014';
  }

  function renderSessions() {
    dom.sessionList.innerHTML = '';

    if (state.sessions.length === 0) {
      dom.sessionList.innerHTML = '<div class="empty-state">No sessions yet.<br>Click + to start.</div>';
      return;
    }

    state.sessions.forEach((s) => {
      const el = document.createElement('div');
      el.className = 'session-item' + (s.id === state.activeSessionId ? ' active' : '');
      el.innerHTML = `
        <span class="session-item-name">${escHtml(s.name)}</span>
        <span class="session-item-count">${s.messages ? s.messages.length : 0}</span>
      `;
      el.addEventListener('click', () => switchSession(s.id));
      dom.sessionList.appendChild(el);
    });
  }

  function switchSession(id) {
    if (state.isStreaming) return;
    const session = state.sessions.find((s) => s.id === id);
    if (!session) return;
    state.activeSessionId = id;
    state.messages = Array.isArray(session.messages)
      ? session.messages.map((msg) => ensurePreviewMessageId(msg))
      : [];
    session.messages = state.messages;
    saveSettings();
    renderSessions();
    renderAllMessages();
    dom.tbSession.textContent = '\u2014 ' + session.name + ' \u2014';
  }

  function ensureActiveSession() {
    if (!state.activeSessionId) {
      createNewSession();
    } else {
      const session = state.sessions.find((s) => s.id === state.activeSessionId);
      if (session) {
        state.messages = Array.isArray(session.messages)
          ? session.messages.map((msg) => ensurePreviewMessageId(msg))
          : [];
        session.messages = state.messages;
        dom.tbSession.textContent = '\u2014 ' + session.name + ' \u2014';
      } else {
        state.activeSessionId = null;
        createNewSession();
      }
    }
    renderSessions();
    renderAllMessages();
  }

  // ── Messaging ────────────────────────────────────────────────
  async function handleSend() {
    const draftEntry = snapshotCurrentDraft();
    const text = draftEntry.text.trim();
    if (!text || state.isStreaming) return;

    if (text.startsWith('/')) {
      handleSlashCommand(text);
      dom.chatInput.value = '';
      dom.chatInput.style.height = 'auto';
      return;
    }

    if (!state.activeSessionId) createNewSession();

    dom.btnSend.disabled = true;
    dom.btnStop.disabled = false;
    pushInputHistory(draftEntry);
    dom.chatInput.value = '';
    dom.chatInput.style.height = 'auto';

    // Build prompt with optional file context
    let promptText = text;
    const uploadedFile = state.attachFileName;
    const uploadedContent = state.attachFileContent;
    clearAttachment();

    if (uploadedFile && uploadedContent) {
      // Truncate file content to avoid blowing up context
      const maxFileLen = 30000;
      const truncated = uploadedContent.length > maxFileLen
        ? uploadedContent.slice(0, maxFileLen) + '\n... (file truncated)'
        : uploadedContent;
      promptText = '# Attached file: ' + uploadedFile + '\n\n```\n' + truncated + '\n```\n\n---\n' + text;
      // Show truncated preview in user message
      const previewLen = Math.min(uploadedContent.length, 1200);
      addUserMessage(text + '\n\n[Attached: ' + uploadedFile + ' (' + (uploadedContent.length > 1024 ? (uploadedContent.length / 1024).toFixed(1) + ' KB)' : uploadedContent.length + ' bytes)') + ']');
    } else {
      addUserMessage(text);
    }
    dom.welcome.style.display = 'none';

    if (!state.backendConnected) {
      addSystemMessage('Connecting to backend...');
      for (let i = 0; i < 20; i++) {
        await new Promise(r => setTimeout(r, 500));
        try {
          const resp = await fetch(state.serverUrl + '/health', { signal: AbortSignal.timeout(2000) });
          if (resp.ok) {
            state.backendConnected = true;
            await loadBackendState();
            break;
          }
        } catch (e) { /* retry */ }
      }
      const lastMsg = state.messages[state.messages.length - 1];
      if (lastMsg && lastMsg.role === 'system' && lastMsg.content === 'Connecting to backend...') {
        state.messages.pop();
        renderAllMessages();
      }
    }

    if (!state.backendConnected) {
      addSystemMessage('Cannot connect to backend. The server may still be starting — please wait a moment and try again.');
      dom.btnSend.disabled = false;
      dom.btnStop.disabled = true;
      return;
    }

    state.isStreaming = true;
    state.streamingThinkingEl = null;
    state.streamingToolEls = [];

    try {
      const res = await fetch(state.serverUrl + '/api/chat', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ prompt: promptText }),
      });
      let data = {};
      try {
        data = await res.json();
      } catch (e) {
        data = {};
      }

      if (!res.ok) {
        const errMsg = (data && (data.error || data.message)) ? (data.error || data.message) : ('HTTP ' + res.status);
        addSystemMessage(String(errMsg));
        finishStreaming();
        return;
      }

      if (data.handled) {
        state.messages = messagesFromServerPayload(data.messages);
        const session = state.sessions.find((s) => s.id === state.activeSessionId);
        if (session) session.messages = state.messages;
        saveSettings();
        renderAllMessages();
        if (data.notice) addSystemMessage(String(data.notice));
        finishStreaming();
        return;
      }

      if (!data.task_id) {
        addSystemMessage(String((data && data.error) ? data.error : 'No task id from server'));
        finishStreaming();
        return;
      }

      state.pendingTaskId = data.task_id;
      createStreamingMessage();
      await pollTaskHttp(data.task_id);
    } catch (e) {
      addSystemMessage('Request failed: ' + (e.message || String(e)));
      finishStreaming();
    }
  }

  async function handleStop() {
    if (!state.isStreaming) return;

    try {
      await fetch(state.serverUrl + '/api/stop', { method: 'POST' });
    } catch (e) { /* ignore */ }

    state.pendingTaskId = null;
    finishStreaming();

    try {
      await fetch(state.serverUrl + '/api/chat', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ prompt: '/new' }),
      });
    } catch (e) { /* ignore */ }

    dom.chatInput.value = '';
    dom.chatInput.focus();
    toast('Generation stopped', 'info');
  }

  function handleSlashCommand(text) {
    const cmd = text.toLowerCase();
    if (cmd === '/new' || cmd === '/clear') {
      clearChat();
    } else if (cmd === '/help') {
      addSystemMessage('Commands: /new (clear), /sessions (list), /theme (cycle), /mode work|plan|review, /workspace (refresh), /quit (exit)');
    } else if (cmd === '/sessions') {
      const list = state.sessions.map((s, i) => `${i + 1}. ${s.name} (${s.messages.length} msgs)`).join('\n');
      addSystemMessage('Sessions:\n' + (list || 'None'));
    } else if (cmd === '/theme') {
      cycleTheme();
      addSystemMessage('Theme: ' + state.theme);
    } else if (cmd.startsWith('/mode ')) {
      const mode = cmd.split(' ')[1];
      if (['work', 'plan', 'review'].includes(mode)) {
        setWorkflowMode(mode);
        addSystemMessage('Mode: ' + mode);
      }
    } else if (cmd === '/workspace') {
      refreshWorkspace();
      addSystemMessage('Workspace refreshed');
    } else if (cmd === '/quit') {
      addSystemMessage('Use Cmd+Q or close the window to quit.');
    } else {
      addSystemMessage('Unknown command: ' + text + '\nType /help for available commands.');
    }
  }

  function addUserMessage(text) {
    const msg = { role: 'user', content: text, timestamp: new Date().toISOString() };
    state.messages.push(msg);
    const session = state.sessions.find((s) => s.id === state.activeSessionId);
    if (session) session.messages = state.messages;
    renderMessage(msg);
    saveSettings();
    renderSessions();
    scrollToBottom();
  }

  function addSystemMessage(text) {
    const msg = { role: 'system', content: text, timestamp: new Date().toISOString() };
    state.messages.push(msg);
    renderMessage(msg);
    scrollToBottom();
  }

  function nextPreviewMessageId() {
    state.previewMessageSeq += 1;
    return 'preview-' + Date.now() + '-' + state.previewMessageSeq;
  }

  function ensurePreviewMessageId(msg) {
    if (!msg || !msg.preview || msg.id) return msg;
    return { ...msg, id: nextPreviewMessageId() };
  }

  function normalizeStoredSessions() {
    if (!Array.isArray(state.sessions)) {
      state.sessions = [];
      return;
    }

    state.sessions = state.sessions.map((session) => ({
      ...session,
      messages: Array.isArray(session.messages)
        ? session.messages.map((msg) => ensurePreviewMessageId(msg))
        : [],
    }));
  }

  function addWorkspacePreviewMessage(preview) {
    const msg = {
      id: nextPreviewMessageId(),
      role: 'system',
      preview,
      timestamp: new Date().toISOString(),
    };
    state.messages.push(msg);
    const session = state.sessions.find((s) => s.id === state.activeSessionId);
    if (session) session.messages = state.messages;
    dom.welcome.style.display = 'none';
    renderMessage(msg);
    saveSettings();
    renderSessions();
    scrollToBottom();
  }

  function createStreamingMessage() {
    const msgObj = { role: 'assistant', content: '', timestamp: new Date().toISOString() };
    state.messages.push(msgObj);
    state.streamingMessageEl = renderMessage(msgObj, true);
  }

  function renderMessage(msg, isStreaming) {
    const el = document.createElement('div');
    el.className = 'message';
    if (msg.id) el.dataset.messageId = msg.id;

    let header = '';
    if (msg.role === 'user') {
      header = `<div class="message-header"><span class="message-role user">YOU</span><span class="message-time">${formatTime(msg.timestamp)}</span></div>`;
    } else if (msg.role === 'assistant') {
      header = `<div class="message-header"><span class="message-role assistant">CODER</span><span class="message-time">${formatTime(msg.timestamp)}</span></div>`;
    } else if (msg.role === 'system') {
      header = '<div class="message-header"><span class="message-role system">SYS</span></div>';
    } else {
      header = `<div class="message-header"><span class="message-role system">${escHtml(msg.role || 'MSG')}</span></div>`;
    }

    const bodyEl = document.createElement('div');
    bodyEl.className = 'message-body';
    if (msg.preview) {
      bodyEl.classList.add('message-body--preview');
      renderWorkspacePreview(bodyEl, msg);
    } else {
      bodyEl.innerHTML = isStreaming ? '' : formatContent(msg.content);
    }

    el.innerHTML = header;
    el.appendChild(bodyEl);

    dom.messages.appendChild(el);
    scrollToBottom();
    return el;
  }

  function renderWorkspacePreview(bodyEl, msg) {
    const preview = msg.preview || {};
    const card = document.createElement('div');
    card.className = 'workspace-preview';

    const header = document.createElement('div');
    header.className = 'workspace-preview__header';

    const headerMain = document.createElement('div');
    headerMain.className = 'workspace-preview__header-main';

    const title = document.createElement('div');
    title.className = 'workspace-preview__title';
    title.textContent = preview.rel || preview.name || 'Workspace preview';

    const meta = document.createElement('div');
    meta.className = 'workspace-preview__meta';
    const metaBits = [];
    if (preview.kind) metaBits.push(String(preview.kind).toUpperCase());
    if (typeof preview.size === 'number') metaBits.push(formatBytes(preview.size));
    if (preview.truncated) metaBits.push('TRUNCATED');
    meta.textContent = metaBits.join(' · ');

    headerMain.appendChild(title);
    headerMain.appendChild(meta);
    header.appendChild(headerMain);

    const closeBtn = document.createElement('button');
    closeBtn.className = 'workspace-preview__close';
    closeBtn.type = 'button';
    closeBtn.setAttribute('aria-label', 'Close preview');
    closeBtn.dataset.previewClose = msg.id || '';
    closeBtn.textContent = '×';
    header.appendChild(closeBtn);

    card.appendChild(header);

    if (preview.kind === 'image' && preview.data_url) {
      const image = document.createElement('img');
      image.className = 'workspace-preview__image';
      image.src = preview.data_url;
      image.alt = preview.name || preview.rel || 'Workspace image preview';
      card.appendChild(image);
    } else if (preview.kind === 'text') {
      const text = document.createElement('pre');
      text.className = 'workspace-preview__text';
      text.textContent = preview.content || '';
      card.appendChild(text);
    } else {
      const notice = document.createElement('div');
      notice.className = 'workspace-preview__notice';
      notice.textContent = preview.message || 'This file type cannot be previewed in the session pane.';
      card.appendChild(notice);
    }

    bodyEl.appendChild(card);
  }

  function renderAllMessages() {
    dom.messages.innerHTML = '';
    if (state.messages.length === 0) {
      dom.welcome.style.display = '';
    } else {
      dom.welcome.style.display = 'none';
      state.messages.forEach((msg) => renderMessage(msg));
    }
  }

  function resetChat() {
    dom.messages.innerHTML = '';
    dom.welcome.style.display = '';
  }

  function clearChat() {
    state.messages = [];
    const session = state.sessions.find((s) => s.id === state.activeSessionId);
    if (session) session.messages = [];
    saveSettings();
    renderSessions();
    resetChat();
    dom.tbSession.textContent = '\u2014 ' + (session ? session.name : 'new session') + ' \u2014';
  }

  // ── Streaming Handlers ───────────────────────────────────────
  function handleStreamToken(data) {
    if (!state.streamingMessageEl) return;
    const bodyEl = state.streamingMessageEl.querySelector('.message-body');
    if (!bodyEl) return;

    const content = data.content || data.token || '';
    state.messages[state.messages.length - 1].content += content;
    bodyEl.innerHTML = formatContent(state.messages[state.messages.length - 1].content);
    scrollToBottom();
  }

  function handleThinkingDelta(data) {
    if (!state.streamingMessageEl) return;
    const bodyEl = state.streamingMessageEl.querySelector('.message-body');
    if (!bodyEl) return;

    if (!state.streamingThinkingEl) {
      state.streamingThinkingEl = createThinkingBlock(bodyEl);
    }

    const body = state.streamingThinkingEl.querySelector('.thinking-body');
    body.textContent += data.content || data.thinking || '';
    scrollToBottom();
  }

  function handleToolStart(data) {
    if (!state.streamingMessageEl) return;
    const bodyEl = state.streamingMessageEl.querySelector('.message-body');
    if (!bodyEl) return;
    const block = createToolBlock(bodyEl, data.name || data.tool_name || 'tool', data.input || '');
    state.streamingToolEls.push(block);
  }

  function handleToolEnd(data) {
    if (!state.streamingMessageEl) return;
    const bodyEl = state.streamingMessageEl.querySelector('.message-body');
    if (!bodyEl) return;
    const blocks = bodyEl.querySelectorAll('.tool-block');
    const last = blocks[blocks.length - 1];
    if (last) {
      const output = last.querySelector('.tool-output');
      if (output) output.textContent += '\n' + (data.output || data.result || '');
    }
  }

  function handleStreamError(data) {
    if (!state.streamingMessageEl) return;
    const bodyEl = state.streamingMessageEl.querySelector('.message-body');
    if (!bodyEl) return;
    const errDiv = document.createElement('div');
    errDiv.style.cssText = 'color: var(--err); margin-top: 8px; font-family: var(--font-mono); font-size: 12px;';
    errDiv.textContent = 'Error: ' + (data.message || data.error || 'Unknown error');
    bodyEl.appendChild(errDiv);
    finishStreaming();
  }

  function handleUsage(data) {
    dom.statusTokens.textContent = formatTokenUsage(data);
  }

  function handleSessionCreated(data) {
    if (data.session_id) {
      state.activeSessionId = data.session_id;
      saveSettings();
    }
  }

  function finishStreaming() {
    state.isStreaming = false;
    state.streamingMessageEl = null;
    state.streamingThinkingEl = null;
    state.streamingToolEls = [];
    dom.btnSend.disabled = false;
    dom.btnStop.disabled = true;
    dom.toolbarTurn.textContent = '';

    const session = state.sessions.find((s) => s.id === state.activeSessionId);
    if (session) session.messages = state.messages;
    saveSettings();
    renderSessions();
  }

  // ── ACP (Multi-Agent) Events ─────────────────────────────────
  function handleAcpEvent(data) {
    if (!state.streamingMessageEl) return;
    const bodyEl = state.streamingMessageEl.querySelector('.message-body');
    if (!bodyEl) return;

    const acp = data.acp_event || data;
    switch (acp.event) {
      case 'plan_start': {
        const card = document.createElement('div');
        card.className = 'acp-plan-card';
        card.innerHTML = '<div class="acp-plan-header"><span class="acp-plan-icon">\u25C6</span>ACP Multi-Agent Plan</div>';
        bodyEl.appendChild(card);
        state._acpCard = card;
        break;
      }
      case 'step_assigned': {
        if (state._acpCard) {
          const step = document.createElement('div');
          step.className = 'acp-step';
          step.innerHTML = `
            <span class="acp-step__role ${escHtml(acp.role || 'coder')}">${escHtml(acp.role || 'coder')}</span>
            <div class="acp-step__desc">${escHtml(acp.description || acp.step || '')}</div>
          `;
          state._acpCard.appendChild(step);
        }
        break;
      }
      case 'step_complete': {
        if (state._acpCard) {
          const steps = state._acpCard.querySelectorAll('.acp-step');
          const idx = (acp.step_index || 0);
          if (steps[idx]) steps[idx].classList.add('status-done');
        }
        break;
      }
      case 'step_error': {
        if (state._acpCard) {
          const steps = state._acpCard.querySelectorAll('.acp-step');
          const idx = (acp.step_index || 0);
          if (steps[idx]) steps[idx].classList.add('status-error');
        }
        break;
      }
      case 'specialist_start': {
        if (state._acpCard) {
          const banner = document.createElement('div');
          banner.className = 'acp-specialist-banner';
          banner.textContent = 'Deploying ' + (acp.role || 'specialist') + ' agent...';
          state._acpCard.appendChild(banner);
        }
        break;
      }
    }
    scrollToBottom();
  }

  // ── One Shot Events ──────────────────────────────────────────
  function handleOneShotEvent(data) {
    if (!state.streamingMessageEl) return;
    const bodyEl = state.streamingMessageEl.querySelector('.message-body');
    if (!bodyEl) return;

    const ev = data.oneshot_event || data;
    switch (ev.event) {
      case 'brainstorming_start': {
        const badge = document.createElement('div');
        badge.className = 'oneshot-cycle-badge';
        badge.textContent = '\u21BB Brainstorm Cycle ' + (ev.cycle || '?');
        bodyEl.appendChild(badge);
        state._oneshotSection = badge;
        break;
      }
      case 'options_generated': {
        const list = document.createElement('div');
        list.className = 'oneshot-option-list';
        let html = '<div class="oneshot-option-list-header">Options</div>';
        const options = ev.options || [];
        options.forEach((opt) => {
          const isSelected = opt.direction === ev.selected;
          html += '<div class="oneshot-option' + (isSelected ? ' selected' : '') + '">'
            + '<div class="oneshot-option__direction">' + escHtml(opt.direction) + '</div>';
          if (opt.rationale) html += '<div class="oneshot-option__rationale">' + escHtml(opt.rationale) + '</div>';
          if (opt.pros && opt.pros.length) html += '<div class="oneshot-option__pros">+ ' + opt.pros.join(', ') + '</div>';
          if (isSelected) html += '<div style="color:var(--accent-1);font-size:10px;margin-top:2px;">\u25B6 Selected</div>';
          html += '</div>';
        });
        list.innerHTML = html;
        bodyEl.appendChild(list);
        break;
      }
      case 'brainstorming_exhausted': {
        const banner = document.createElement('div');
        banner.className = 'oneshot-exhausted-banner';
        banner.textContent = 'Exhausted: ' + (ev.reason || 'No new options available');
        bodyEl.appendChild(banner);
        break;
      }
      case 'roadblock_detected': {
        const block = document.createElement('div');
        block.className = 'oneshot-cycle-badge';
        block.style.borderColor = 'var(--warn)';
        block.style.color = 'var(--warn)';
        block.textContent = '\u26A0 Roadblock Cycle ' + (ev.cycle || '') + ': ' + (ev.reason || '');
        bodyEl.appendChild(block);
        break;
      }
      case 'done': {
        const done = document.createElement('div');
        done.className = 'oneshot-cycle-badge';
        done.style.borderColor = 'var(--ok)';
        done.style.color = 'var(--ok)';
        done.textContent = '\u2713 One Shot Complete';
        bodyEl.appendChild(done);
        break;
      }
    }
    scrollToBottom();
  }

  // ── Content Formatting ───────────────────────────────────────
  function formatContent(text) {
    if (!text) return '';

    let html = escHtml(text);

    // Code blocks with language
    html = html.replace(/```(\w*)\n([\s\S]*?)```/g, (_, lang, code) => {
      return createCodeBlockHtml(lang, code.trim());
    });

    // Inline code
    html = html.replace(/`([^`]+)`/g, '<code style="background:var(--bg-2);padding:1px 5px;border-radius:3px;font-family:\'SF Mono\',\'Fira Code\',monospace;font-size:0.92em;color:var(--accent-1);">$1</code>');

    // Bold
    html = html.replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>');

    return html;
  }

  function createCodeBlockHtml(lang, code) {
    const langLabel = lang || 'code';
    return '<div class="code-block">'
      + '<div class="code-block-header">'
      + '<span class="code-block-lang">' + escHtml(langLabel) + '</span>'
      + '<button class="code-block-copy" data-code="' + escAttr(code) + '">Copy</button>'
      + '</div>'
      + '<pre>' + escHtml(code) + '</pre></div>';
  }

  function createThinkingBlock(container) {
    const block = document.createElement('div');
    block.className = 'thinking-block expanded';
    block.innerHTML = '<button class="thinking-toggle">'
      + '<span class="thinking-indicator"></span>'
      + '<span>THINKING</span>'
      + '<span class="thinking-chevron">\u25B8</span>'
      + '</button>'
      + '<div class="thinking-body"></div>';
    const btn = block.querySelector('.thinking-toggle');
    btn.addEventListener('click', () => block.classList.toggle('expanded'));
    container.appendChild(block);
    return block;
  }

  function createToolBlock(container, toolName, toolInput) {
    const block = document.createElement('div');
    block.className = 'tool-block';
    block.innerHTML = '<div class="tool-header">\u26A1 ' + escHtml(toolName) + '</div>'
      + '<div class="tool-output">' + escHtml(typeof toolInput === 'string' ? toolInput : JSON.stringify(toolInput, null, 2)) + '</div>';
    container.appendChild(block);
    return block;
  }

  // Delegated event for copy buttons
  document.addEventListener('click', (e) => {
    if (e.target.classList.contains('code-block-copy')) {
      const code = e.target.dataset.code;
      navigator.clipboard.writeText(code).then(() => {
        e.target.textContent = 'Copied!';
        e.target.classList.add('copied');
        setTimeout(() => {
          e.target.textContent = 'Copy';
          e.target.classList.remove('copied');
        }, 2000);
      }).catch(() => {
        e.target.textContent = 'Failed';
      });
    }
  });

  // ── Workflow ─────────────────────────────────────────────────
  function setWorkflowMode(mode) {
    state.workflowMode = mode;
    dom.toolbarMode.textContent = mode.charAt(0).toUpperCase() + mode.slice(1);
    $$('.workflow-node').forEach(n => {
      n.classList.toggle('active', n.dataset.mode === mode);
    });
    saveSettings();
    sendConfig({ mode });
  }

  // ── Toggles ──────────────────────────────────────────────────
  async function toggleMultiAgent() {
    const wantsEnabled = dom.toggleMultiAgent.checked;
    if (wantsEnabled && state.oneShotEnabled) {
      state.oneShotEnabled = false;
      dom.toggleOneShot.checked = false;
      sendConfig({ one_shot_enabled: false });
    }
    state.multiAgentEnabled = wantsEnabled;
    sendConfig({ multi_agent_enabled: wantsEnabled });
    saveSettings();
  }

  async function toggleOneShot() {
    const wantsEnabled = dom.toggleOneShot.checked;
    if (wantsEnabled && state.multiAgentEnabled) {
      state.multiAgentEnabled = false;
      dom.toggleMultiAgent.checked = false;
      sendConfig({ multi_agent_enabled: false });
    }
    state.oneShotEnabled = wantsEnabled;
    sendConfig({ one_shot_enabled: wantsEnabled });
    saveSettings();
  }

  async function toggleComputerUse() {
    state.computerUseEnabled = dom.toggleComputerUse.checked;
    sendConfig({ computer_use_enabled: state.computerUseEnabled });
    saveSettings();
  }

  function sendConfig(data) {
    if (!state.backendConnected) return;
    const base = state.serverUrl;
    const opts = { headers: { 'Content-Type': 'application/json' } };
    try {
      if (data.mode !== undefined) {
        fetch(base + '/api/mode', { method: 'POST', ...opts, body: JSON.stringify({ mode: data.mode }) }).catch(() => {});
      }
      if (data.multi_agent_enabled !== undefined) {
        fetch(base + '/api/multi-agent', { method: 'POST', ...opts, body: JSON.stringify({ enabled: data.multi_agent_enabled }) }).catch(() => {});
      }
      if (data.one_shot_enabled !== undefined) {
        fetch(base + '/api/one-shot', { method: 'POST', ...opts, body: JSON.stringify({ enabled: data.one_shot_enabled }) }).catch(() => {});
      }
      if (data.computer_use_enabled !== undefined) {
        fetch(base + '/api/computer-use', { method: 'POST', ...opts, body: JSON.stringify({ enabled: data.computer_use_enabled }) }).catch(() => {});
      }
    } catch (e) { /* ignore */ }
  }

  // ── Sidebar ──────────────────────────────────────────────────
  function toggleSidebar() {
    state.sidebarVisible = !state.sidebarVisible;
    updateSidebarVisibility();
    saveSettings();
  }

  async function refreshWorkspace() {
    if (!state.backendConnected) return;
    try {
      const treeResp = await fetch(state.serverUrl + '/api/workspace/tree');
      if (treeResp.ok) {
        const treeData = await treeResp.json();
        state.workspaceFiles = treeData.tree || [];
      }
      const changesResp = await fetch(state.serverUrl + '/api/changes');
      if (changesResp.ok) {
        const changesData = await changesResp.json();
        const raw = changesData.changes || [];
        state.gitChanges = raw.map((c) => ({
          path: c.path || c.basename,
          name: c.basename || c.path,
          status: 'modified',
        }));
      }
      renderWorkspaceTree();
      renderGitChanges();
    } catch (e) { /* ignore */ }
  }

  async function handleWorkspaceTreeClick(event) {
    const entry = event.target.closest('.ws-entry');
    if (!entry || !dom.workspaceTree.contains(entry)) return;
    if (entry.dataset.type !== 'file') return;

    const filePath = entry.dataset.path;
    if (!filePath) return;

    state.workspacePreviewPath = filePath;
    renderWorkspaceTree();
    await previewWorkspaceFile(filePath);
  }

  function handleMessageActionsClick(event) {
    const closeBtn = event.target.closest('[data-preview-close]');
    if (!closeBtn || !dom.messages.contains(closeBtn)) return;

    const messageId = closeBtn.dataset.previewClose;
    if (!messageId) return;
    removeMessageById(messageId);
  }

  function removeMessageById(messageId) {
    const nextMessages = state.messages.filter((msg) => msg.id !== messageId);
    if (nextMessages.length === state.messages.length) return;

    state.messages = nextMessages;
    const session = state.sessions.find((s) => s.id === state.activeSessionId);
    if (session) session.messages = state.messages;
    saveSettings();
    renderSessions();
    renderAllMessages();
  }

  async function previewWorkspaceFile(filePath) {
    try {
      const response = await fetch(state.serverUrl + '/api/workspace/preview?path=' + encodeURIComponent(filePath));
      const payload = await response.json().catch(() => ({}));
      if (!response.ok) {
        throw new Error(payload.error || 'Failed to preview file');
      }
      addWorkspacePreviewMessage(payload);
    } catch (error) {
      toast(error.message || 'Failed to preview workspace file', 'error');
    }
  }

  function renderWorkspaceTree() {
    if (state.workspaceFiles.length === 0) {
      dom.workspaceTree.innerHTML = '<div class="empty-state">No workspace loaded</div>';
      return;
    }
    dom.workspaceTree.innerHTML = state.workspaceFiles.map(f => {
      const isDir = f.type === 'dir';
      let iconClass = 'file';
      if (isDir) iconClass = 'dir';
      else if (f.name && (f.name.endsWith('.rs') || f.name.endsWith('.go') || f.name.endsWith('.ts'))) iconClass += ' rs';
      else if (f.name && f.name.endsWith('.md')) iconClass += ' md';
      const icon = isDir ? '\u25B8' : '\u2014';
      const indent = (f.depth || 0) * 12;
      const isActive = !isDir && state.workspacePreviewPath === (f.path || '');
      return '<div class="ws-entry' + (isActive ? ' is-active' : '') + '" style="padding-left:' + (14 + indent) + 'px" data-path="' + escAttr(f.path || '') + '" data-type="' + escAttr(f.type || '') + '">'
        + '<span class="ws-entry__icon ' + iconClass + '">' + icon + '</span>'
        + '<span class="ws-entry__name">' + escHtml(f.name || '') + '</span>'
        + '</div>';
    }).join('');
  }

  function renderGitChanges() {
    if (!state.gitChanges || state.gitChanges.length === 0) {
      dom.changesSection.style.display = 'none';
      return;
    }
    dom.changesSection.style.display = '';
    dom.changesCount.textContent = state.gitChanges.length;
    dom.changesList.innerHTML = state.gitChanges.map(c => '<div class="change-entry ' + escHtml(c.status || 'modified') + '">'
      + '<span class="change-entry__icon">' + (c.status === 'added' ? 'A' : c.status === 'deleted' ? 'D' : 'M') + '</span>'
      + '<span class="change-entry__name">' + escHtml(c.path || c.name || '') + '</span>'
      + '</div>').join('');
  }

  // ── Skills Management ────────────────────────────────────────
  async function loadSkills() {
    try {
      const res = await fetch(state.serverUrl + '/api/skills');
      if (!res.ok) return;
      const data = await res.json();
      state.skills = data.skills || [];
      renderSkills();
    } catch (e) {
      if (dom.skillsStatus) dom.skillsStatus.textContent = 'Failed to load skills';
    }
  }

  function renderSkills() {
    if (!dom.skillsList) return;

    if (!state.skills || !state.skills.length) {
      dom.skillsList.innerHTML = '<div class="empty-state">No skills installed. Paste a GitHub or Clawhub URL above to install one.</div>';
      if (dom.skillsStatus) dom.skillsStatus.textContent = '0 skills installed';
      return;
    }

    if (dom.skillsStatus) dom.skillsStatus.textContent = state.skills.length + ' skill(s) installed';

    dom.skillsList.innerHTML = state.skills.map((s) => {
      const statusLabel = s.enabled ? 'Enabled' : 'Disabled';
      const statusClass = s.enabled ? 'skill-badge--enabled' : 'skill-badge--disabled';
      const sourceDisplay = s.source_url ? s.source_url.replace(/^https?:\/\//, '').slice(0, 50) + (s.source_url.length > 50 ? '...' : '') : '';
      const descDisplay = s.description || 'No description';
      const filesLabel = (s.file_count || (s.files ? s.files.length : 0)) + ' file(s)';

      return '<div class="skill-card" data-skill-name="' + escHtml(s.name) + '">'
        + '<div class="skill-card__header">'
        + '<span class="skill-card__name">' + escHtml(s.display_name || s.name) + '</span>'
        + '<div class="skill-card__actions">'
        + '<span class="skill-badge ' + statusClass + '">' + statusLabel + '</span>'
        + '<span class="skill-badge skill-badge--count">' + filesLabel + '</span>'
        + '</div>'
        + '</div>'
        + '<div class="skill-card__desc">' + escHtml(descDisplay) + '</div>'
        + '<div class="skill-card__meta">'
        + (sourceDisplay ? '<span>Source: <a href="' + escHtml(s.source_url) + '" target="_blank" rel="noopener">' + escHtml(sourceDisplay) + '</a></span>' : '')
        + (s.version ? '<span>v' + escHtml(s.version) + '</span>' : '')
        + (s.installed_at ? '<span>Installed: ' + escHtml(s.installed_at.slice(0, 10)) + '</span>' : '')
        + '</div>'
        + '<div class="skill-card__actions">'
        + '<button class="skill-btn skill-btn--primary skill-preview-btn" data-skill="' + escHtml(s.name) + '">Preview</button>'
        + '<button class="skill-btn skill-toggle-btn" data-skill="' + escHtml(s.name) + '">' + (s.enabled ? 'Disable' : 'Enable') + '</button>'
        + '<button class="skill-btn skill-upgrade-btn" data-skill="' + escHtml(s.name) + '">Upgrade</button>'
        + '<button class="skill-btn skill-btn--danger skill-delete-btn" data-skill="' + escHtml(s.name) + '">Delete</button>'
        + '</div>'
        + '</div>';
    }).join('');

    // Wire up buttons
    dom.skillsList.querySelectorAll('.skill-toggle-btn').forEach((btn) => {
      btn.addEventListener('click', () => toggleSkill(btn.dataset.skill));
    });
    dom.skillsList.querySelectorAll('.skill-delete-btn').forEach((btn) => {
      btn.addEventListener('click', () => deleteSkill(btn.dataset.skill));
    });
    dom.skillsList.querySelectorAll('.skill-upgrade-btn').forEach((btn) => {
      btn.addEventListener('click', () => upgradeSkill(btn.dataset.skill));
    });
    dom.skillsList.querySelectorAll('.skill-preview-btn').forEach((btn) => {
      btn.addEventListener('click', () => previewSkill(btn.dataset.skill));
    });
  }

  async function installSkill() {
    if (!dom.skillUrlInput) return;
    const url = dom.skillUrlInput.value.trim();
    if (!url) {
      toast('Please enter a skill URL', 'warn');
      return;
    }

    dom.installSkillButton.disabled = true;
    dom.installSkillButton.textContent = 'Installing...';

    try {
      const res = await fetch(state.serverUrl + '/api/skills/install', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ url }),
      });
      const data = await res.json();
      if (!res.ok) {
        toast(data.error || 'Failed to install skill', 'error');
        return;
      }
      dom.skillUrlInput.value = '';
      toast('Skill "' + (data.display_name || data.name) + '" installed successfully', 'info');
      await loadSkills();
    } catch (e) {
      toast('Install failed: ' + e.message, 'error');
    } finally {
      dom.installSkillButton.disabled = false;
      dom.installSkillButton.textContent = 'Install';
    }
  }

  async function toggleSkill(name) {
    try {
      const res = await fetch(state.serverUrl + '/api/skills/' + encodeURIComponent(name) + '/toggle', {
        method: 'POST',
      });
      const data = await res.json();
      if (!res.ok) {
        toast(data.error || 'Failed to toggle skill', 'error');
        return;
      }
      toast('Skill "' + (data.display_name || data.name) + '" ' + (data.enabled ? 'enabled' : 'disabled'), 'info');
      await loadSkills();
    } catch (e) {
      toast('Toggle failed: ' + e.message, 'error');
    }
  }

  async function deleteSkill(name) {
    if (!confirm('Delete skill "' + name + '"? This removes all its files.')) return;

    try {
      const res = await fetch(state.serverUrl + '/api/skills/' + encodeURIComponent(name) + '/delete', {
        method: 'POST',
      });
      const data = await res.json();
      if (!res.ok) {
        toast(data.error || 'Failed to delete skill', 'error');
        return;
      }
      toast('Skill "' + name + '" deleted', 'info');
      await loadSkills();
    } catch (e) {
      toast('Delete failed: ' + e.message, 'error');
    }
  }

  async function upgradeSkill(name) {
    const btn = dom.skillsList ? dom.skillsList.querySelector('.skill-upgrade-btn[data-skill="' + CSS.escape(name) + '"]') : null;
    if (btn) {
      btn.disabled = true;
      btn.textContent = 'Upgrading...';
    }

    try {
      const res = await fetch(state.serverUrl + '/api/skills/' + encodeURIComponent(name) + '/upgrade', {
        method: 'POST',
      });
      const data = await res.json();
      if (!res.ok) {
        toast(data.error || 'Failed to upgrade skill', 'error');
        return;
      }
      toast('Skill "' + (data.display_name || data.name) + '" upgraded', 'info');
      await loadSkills();
    } catch (e) {
      toast('Upgrade failed: ' + e.message, 'error');
    } finally {
      if (btn) {
        btn.disabled = false;
        btn.textContent = 'Upgrade';
      }
    }
  }

  async function previewSkill(name) {
    try {
      const res = await fetch(state.serverUrl + '/api/skills/' + encodeURIComponent(name) + '/preview');
      const data = await res.json();
      if (!res.ok) {
        toast(data.error || 'Failed to preview skill', 'error');
        return;
      }
      // Show preview as a system message in the chat
      const previewText = '## Skill Preview: ' + name + '\n\n**File:** `' + (data.file || 'README.md') + '`\n\n```markdown\n' + (data.content || '') + '\n```';
      addSystemMessage(previewText);
    } catch (e) {
      toast('Preview failed: ' + e.message, 'error');
    }
  }

  // ── Settings Dialog ──────────────────────────────────────────
  function openSettings() {
    dom.settingsOverlay.classList.add('visible');
    // Load skills fresh when opening
    loadSkills();
    switchSettingsTab('model');
  }

  function closeSettings() {
    dom.settingsOverlay.classList.remove('visible');
  }

  function switchSettingsTab(name) {
    dom.settingsTabs.querySelectorAll('.settings-tab').forEach(t => t.classList.toggle('active', t.dataset.tab === name));
    $$('.settings-page').forEach(p => p.classList.toggle('active', p.id === 'settings-page-' + name));
  }

  async function pickFolder() {
    // Use Electron native dialog via preload bridge
    if (window.electronAPI && window.electronAPI.pickFolder) {
      try {
        const path = await window.electronAPI.pickFolder();
        if (path && dom.cfgProjectDir) {
          dom.cfgProjectDir.value = path;
        }
      } catch (e) {
        toast('Folder picker failed: ' + e.message, 'error');
      }
    }
  }

  async function applyWorkspace() {
    const path = dom.cfgProjectDir?.value?.trim();
    if (!path) {
      toast('Please select or enter a project directory', 'error');
      return;
    }
    // Derive workspace name from folder name
    const name = path.split(/[\\/]/).filter(Boolean).pop() || 'workspace';
    try {
      const resp = await fetch(state.serverUrl + '/api/workspace', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ path, name }),
      });
      if (!resp.ok) {
        const err = await resp.json().catch(() => ({}));
        toast('Workspace error: ' + (err.error || resp.statusText), 'error');
        return;
      }
      const data = await resp.json();
      toast('Workspace opened: ' + (data.active?.name || name), 'success');
      state.backendConnected = true;
      await refreshWorkspace();
    } catch (e) {
      toast('Failed to open workspace: ' + e.message, 'error');
    }
  }

  // ── Attach File ──────────────────────────────────────────────
  async function handleAttachFile() {
    const file = dom.attachFileInput.files[0];
    if (!file) return;
    try {
      const content = await file.text();
      setAttachmentPreview(file.name, content);
    } catch (e) {
      toast('Failed to read file: ' + e.message, 'error');
    }
    dom.attachFileInput.value = '';
  }

  function setAttachmentPreview(name, content) {
    state.attachFileName = name;
    state.attachFileContent = content;
    dom.attachPreviewName.textContent = name;
    dom.attachPreview.style.display = 'flex';
  }

  function clearAttachment() {
    state.attachFileName = null;
    state.attachFileContent = null;
    dom.attachPreview.style.display = 'none';
    dom.attachFileInput.value = '';
  }

  // ── Theme ────────────────────────────────────────────────────
  function setTheme(name, persist) {
    if (!THEME_OPTIONS.includes(name)) return;
    state.theme = name;
    document.documentElement.setAttribute('data-theme', name);
    // Also set on body for CSS selectors
    document.body.setAttribute('data-theme', name);
    if (dom.themeSelect) dom.themeSelect.value = name;
    if (persist !== false) {
      try {
        window.localStorage.setItem('generic-coder-theme', name);
      } catch (e) { /* ignore */ }
    }
  }

  function cycleTheme() {
    const idx = THEME_OPTIONS.indexOf(state.theme);
    const next = THEME_OPTIONS[(idx + 1) % THEME_OPTIONS.length];
    setTheme(next);
    saveSettings();
    toast('Theme: ' + (THEME_LABELS[next] || next), 'info');
  }

  // ── Command Palette ──────────────────────────────────────────
  function openCommandPalette() {
    dom.cmdPaletteOverlay.classList.add('visible');
    dom.cmdPaletteInput.value = '';
    state.cmdPaletteIdx = -1;
    filterCmdPalette();
    setTimeout(() => dom.cmdPaletteInput.focus(), 50);
  }

  function closeCommandPalette() {
    dom.cmdPaletteOverlay.classList.remove('visible');
    state.cmdPaletteIdx = -1;
  }

  function filterCmdPalette() {
    const query = dom.cmdPaletteInput.value.toLowerCase();
    const items = $$('#cmd-palette-results .cmd-item');
    items.forEach(item => {
      const label = (item.querySelector('.cmd-item__label')?.textContent || '').toLowerCase();
      item.style.display = !query || label.includes(query) ? '' : 'none';
    });
    state.cmdPaletteIdx = -1;
    updateCmdPaletteSelection();
  }

  function handleCmdPaletteKey(e) {
    const items = [...$$('#cmd-palette-results .cmd-item')].filter(i => i.style.display !== 'none');
    if (e.key === 'Escape') {
      closeCommandPalette();
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      state.cmdPaletteIdx = Math.min(state.cmdPaletteIdx + 1, items.length - 1);
      updateCmdPaletteSelection();
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      state.cmdPaletteIdx = Math.max(state.cmdPaletteIdx - 1, -1);
      updateCmdPaletteSelection();
    } else if (e.key === 'Enter') {
      e.preventDefault();
      if (state.cmdPaletteIdx >= 0 && items[state.cmdPaletteIdx]) {
        executeCmdPaletteAction(items[state.cmdPaletteIdx].dataset.cmd);
      }
      closeCommandPalette();
    }
  }

  function updateCmdPaletteSelection() {
    const items = $$('#cmd-palette-results .cmd-item');
    items.forEach((item, i) => {
      item.classList.toggle('selected', i === state.cmdPaletteIdx);
    });
  }

  function executeCmdPaletteAction(cmd) {
    switch (cmd) {
      case 'new-session': createNewSession(); break;
      case 'toggle-sidebar': toggleSidebar(); break;
      case 'settings': openSettings(); break;
      case 'work-mode': setWorkflowMode('work'); break;
      case 'plan-mode': setWorkflowMode('plan'); break;
      case 'review-mode': setWorkflowMode('review'); break;
      case 'toggle-multi-agent': dom.toggleMultiAgent.click(); break;
      case 'toggle-theme': cycleTheme(); break;
      case 'stop': handleStop(); break;
      case 'clear': clearChat(); break;
    }
  }

  // ── Toast Notifications ──────────────────────────────────────
  function toast(message, type) {
    type = type || 'info';
    const el = document.createElement('div');
    el.className = 'toast ' + type;
    el.textContent = message;
    dom.toastContainer.appendChild(el);
    setTimeout(() => {
      el.style.opacity = '0';
      el.style.transform = 'translateX(20px)';
      el.style.transition = 'all 0.2s ease';
      setTimeout(() => el.remove(), 200);
    }, 3000);
  }

  // ── Helpers ──────────────────────────────────────────────────
  function escHtml(str) {
    if (!str) return '';
    const div = document.createElement('div');
    div.appendChild(document.createTextNode(str));
    return div.innerHTML;
  }

  function escAttr(str) {
    if (!str) return '';
    return str.replace(/'/g, '&#39;').replace(/"/g, '&quot;');
  }

  function formatTokenUsage(data) {
    const usage = data && typeof data.usage === 'object' ? data.usage : data;
    if (!usage || typeof usage !== 'object') return '\u2014';

    const inputTokens = firstNumericValue(
      usage.input_tokens,
      usage.prompt_tokens,
      usage.prompt_token_count,
      usage.inputTokenCount
    );
    const outputTokens = firstNumericValue(
      usage.output_tokens,
      usage.completion_tokens,
      usage.generated_tokens,
      usage.outputTokenCount
    );
    const cachedTokens = firstNumericValue(
      usage.cached_tokens,
      usage.cache_creation_input_tokens,
      usage.cache_read_input_tokens
    );
    const totalTokens = firstNumericValue(
      usage.total_tokens,
      usage.totalTokenCount,
      usage.usage,
      Number.isFinite(inputTokens) && Number.isFinite(outputTokens) ? inputTokens + outputTokens : null
    );

    if (!Number.isFinite(totalTokens) && !Number.isFinite(inputTokens) && !Number.isFinite(outputTokens)) {
      return '\u2014';
    }

    const detailBits = [];
    if (Number.isFinite(inputTokens)) detailBits.push(formatNumber(inputTokens) + ' in');
    if (Number.isFinite(outputTokens)) detailBits.push(formatNumber(outputTokens) + ' out');
    if (Number.isFinite(cachedTokens)) detailBits.push(formatNumber(cachedTokens) + ' cached');

    if (Number.isFinite(totalTokens)) {
      return detailBits.length
        ? formatNumber(totalTokens) + ' (' + detailBits.join(' / ') + ')'
        : formatNumber(totalTokens);
    }

    return detailBits.join(' / ');
  }

  function firstNumericValue() {
    for (let index = 0; index < arguments.length; index += 1) {
      const value = arguments[index];
      const numeric = typeof value === 'string' && value.trim() !== '' ? Number(value) : value;
      if (Number.isFinite(numeric)) return numeric;
    }
    return null;
  }

  function formatNumber(value) {
    return new Intl.NumberFormat().format(Math.round(value));
  }

  function formatBytes(value) {
    if (!Number.isFinite(value) || value < 1024) return (value || 0) + ' B';
    if (value < 1024 * 1024) return (value / 1024).toFixed(1) + ' KB';
    return (value / (1024 * 1024)).toFixed(1) + ' MB';
  }

  function formatTime(ts) {
    if (!ts) return '';
    const d = new Date(ts);
    return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  }

  function scrollToBottom() {
    requestAnimationFrame(() => {
      dom.chatContainer.scrollTop = dom.chatContainer.scrollHeight;
    });
  }

  // ── Hint Chips ───────────────────────────────────────────────
  function bindHintChips() {
    $$('.hint-chip').forEach(chip => {
      chip.addEventListener('click', () => {
        dom.chatInput.value = chip.textContent;
        handleSend();
      });
    });
  }


  // ── Boot ─────────────────────────────────────────────────────
  void init();

})();
