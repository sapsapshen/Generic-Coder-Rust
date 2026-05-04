const DEFAULT_LLM_FORM = {
  entry_key: 'generic_coder_native_oai_config',
  session_type: 'native_oai',
  protocol_preset: 'custom',
  api_mode: 'chat_completions',
  provider: '',
  name: '',
  apikey: '',
  apibase: '',
  model: '',
};

const DEFAULT_REMOTE_FORM = {
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

const state = {
  messages: [],
  pendingTaskId: null,
  theme: 'solarflare',
  taskPlaceholderId: null,
  isRunning: false,
  locale: 'zh',
  workspaceNameDirty: false,
  workspaceNameAutoFilled: false,
  workspacePickerToken: '',
  models: [],
  currentModelIndex: 0,
  llmForm: { ...DEFAULT_LLM_FORM },
  workspace: { active: null, workspaces: [], recent_folders: [] },
  remote: { form: { ...DEFAULT_REMOTE_FORM }, configs: [], active_connections: [], connected: false },
  currentMode: 'work',
  workflowNodes: [],
  workflowActive: false,
  workflowCurrentNode: 0,
};

const THEME_OPTIONS = ['solarflare', 'graphite', 'daybreak', 'paperink', 'obsidian', 'slate', 'oxide', 'noir', 'cobalt', 'borealis'];
const LANGUAGE_OPTIONS = ['zh', 'en', 'es'];
const SESSION_TYPE_OPTIONS = ['native_oai', 'oai', 'native_claude', 'claude'];
const PROTOCOL_PRESET_OPTIONS = [
  'custom',
  'deepseek',
  'qwen_dashscope',
  'openai_chat',
  'openai_responses',
  'anthropic_messages',
  'openrouter',
  'moonshot_oai',
  'minimax_oai',
  'doubao_ark',
  'hunyuan_oai',
  'baidu_qianfan',
  'zhipu_anthropic',
];
const MODEL_PRESET_OPTIONS = [
  'custom',
  'deepseek_v4_pro',
  'deepseek_v4_flash',
  'qwen_max_latest',
  'qwen_plus_latest',
  'qwen3_coder_plus',
  'qwen3_coder_flash',
  'kimi_k26',
  'kimi_k25',
  'minimax_m27',
  'minimax_m1',
  'glm_5_1',
  'doubao_15_pro_32k',
  'hunyuan_turbos_latest',
  'ernie_45_turbo_128k',
  'mimo_v25',
  'gpt_5_4',
  'gpt_4_1',
  'claude_sonnet_4',
  'claude_opus_4_7',
  'openrouter_claude',
];

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

const I18N = {
  zh: {
    newChat: '新建会话',
    sessions: '恢复会话',
    stop: '停止生成',
    settings: '设置',
    close: '关闭',
    send: '发送',
    model: '模型',
    theme: '主题',
    language: '语言',
    sessionsTitle: '可恢复会话',
    composerPlaceholder: '输入消息',
    idle: '空闲',
    running: '运行中',
    modelOffline: '未配置模型',
    pollingError: '任务轮询失败',
    stopSent: '已发送停止信号',
    requestError: '请求未成功发出',
    modelSwitchError: '模型切换失败',
    modelSwitchOk: '模型已切换',
    modelSettings: '模型设置',
    commonModel: '常见模型',
    protocolPreset: '协议预设',
    sessionType: '接入协议',
    provider: '提供商',
    displayName: '显示名',
    modelName: '模型名称',
    baseUrl: 'Base URL',
    apiKey: 'API Key',
    showSecret: '显示',
    hideSecret: '隐藏',
    saveModelSettings: '保存模型配置',
    llmSaveOk: '模型配置已保存并启用',
    llmSaveError: '模型配置保存失败',
    workspaceSettings: '本地工作区',
    workspaceName: '工作区名称',
    workspacePath: '工作目录',
    browseFolder: '浏览',
    applyWorkspace: '应用工作区',
    workspaceSaveOk: '工作区已切换',
    workspaceSaveError: '工作区切换失败',
    workspacePickError: '图形选择目录失败',
    currentWorkspace: '当前工作区',
    noWorkspace: '未设置本地工作区',
    recentWorkspaces: '最近工作区',
    use: '使用',
    remoteSettings: '远程工作环境',
    remoteMode: '启用远程工作环境',
    remoteName: '连接名称',
    host: '主机',
    port: '端口',
    username: '用户名',
    password: '密码',
    keyPath: '密钥路径',
    remoteCwd: '远程工作目录',
    connectRemote: '连接远程环境',
    remoteConnectOk: '远程环境已连接',
    remoteConnectError: '远程环境连接失败',
    currentRemote: '当前远程环境',
    noRemote: '未连接远程环境',
    savedRemotes: '已保存连接',
    connected: '已连接',
    disconnected: '未连接',
    interfaceSettings: '界面',
    noSessions: '没有可恢复的历史会话。',
    userRole: '你',
    assistantRole: 'Generic Coder',
    languageName: '中文',
    rounds: (value) => `${value} 轮`,
  },
  en: {
    newChat: 'New chat',
    sessions: 'Sessions',
    stop: 'Stop',
    settings: 'Settings',
    close: 'Close',
    send: 'Send',
    model: 'Model',
    theme: 'Theme',
    language: 'Language',
    sessionsTitle: 'Sessions',
    composerPlaceholder: 'Type a message',
    idle: 'Idle',
    running: 'Running',
    modelOffline: 'Model offline',
    pollingError: 'Task polling failed',
    stopSent: 'Stop signal sent',
    requestError: 'Request did not start',
    modelSwitchError: 'Model switch failed',
    modelSwitchOk: 'Model switched',
    modelSettings: 'Model settings',
    commonModel: 'Common model',
    protocolPreset: 'Protocol preset',
    sessionType: 'Session type',
    provider: 'Provider',
    displayName: 'Display name',
    modelName: 'Model name',
    baseUrl: 'Base URL',
    apiKey: 'API Key',
    showSecret: 'Show',
    hideSecret: 'Hide',
    saveModelSettings: 'Save model settings',
    llmSaveOk: 'Model settings saved and activated',
    llmSaveError: 'Failed to save model settings',
    workspaceSettings: 'Local workspace',
    workspaceName: 'Workspace name',
    workspacePath: 'Workspace path',
    browseFolder: 'Browse',
    applyWorkspace: 'Apply workspace',
    workspaceSaveOk: 'Workspace switched',
    workspaceSaveError: 'Failed to switch workspace',
    workspacePickError: 'Failed to choose folder',
    currentWorkspace: 'Current workspace',
    noWorkspace: 'No local workspace selected',
    recentWorkspaces: 'Recent workspaces',
    use: 'Use',
    remoteSettings: 'Remote environment',
    remoteMode: 'Use remote environment',
    remoteName: 'Connection name',
    host: 'Host',
    port: 'Port',
    username: 'Username',
    password: 'Password',
    keyPath: 'Key path',
    remoteCwd: 'Remote working directory',
    connectRemote: 'Connect remote environment',
    remoteConnectOk: 'Remote environment connected',
    remoteConnectError: 'Failed to connect remote environment',
    currentRemote: 'Current remote environment',
    noRemote: 'No remote environment connected',
    savedRemotes: 'Saved connections',
    connected: 'Connected',
    disconnected: 'Disconnected',
    interfaceSettings: 'Interface',
    noSessions: 'No recoverable sessions.',
    userRole: 'You',
    assistantRole: 'Generic Coder',
    languageName: 'English',
    rounds: (value) => `${value} rounds`,
  },
  es: {
    newChat: 'Nuevo chat',
    sessions: 'Sesiones',
    stop: 'Detener',
    settings: 'Ajustes',
    close: 'Cerrar',
    send: 'Enviar',
    model: 'Modelo',
    theme: 'Tema',
    language: 'Idioma',
    sessionsTitle: 'Sesiones',
    composerPlaceholder: 'Escribe un mensaje',
    idle: 'En espera',
    running: 'En curso',
    modelOffline: 'Modelo inactivo',
    pollingError: 'Falló la lectura de la tarea',
    stopSent: 'Se envió la señal de parada',
    requestError: 'La solicitud no se inició',
    modelSwitchError: 'Falló el cambio de modelo',
    modelSwitchOk: 'Modelo cambiado',
    modelSettings: 'Ajustes del modelo',
    commonModel: 'Modelo común',
    protocolPreset: 'Preajuste de protocolo',
    sessionType: 'Tipo de sesión',
    provider: 'Proveedor',
    displayName: 'Nombre visible',
    modelName: 'Nombre del modelo',
    baseUrl: 'Base URL',
    apiKey: 'API Key',
    showSecret: 'Mostrar',
    hideSecret: 'Ocultar',
    saveModelSettings: 'Guardar modelo',
    llmSaveOk: 'La configuración del modelo se guardó y activó',
    llmSaveError: 'No se pudo guardar la configuración del modelo',
    workspaceSettings: 'Espacio de trabajo local',
    workspaceName: 'Nombre del espacio',
    workspacePath: 'Ruta del espacio',
    browseFolder: 'Examinar',
    applyWorkspace: 'Aplicar espacio',
    workspaceSaveOk: 'Espacio de trabajo cambiado',
    workspaceSaveError: 'No se pudo cambiar el espacio',
    workspacePickError: 'No se pudo elegir la carpeta',
    currentWorkspace: 'Espacio actual',
    noWorkspace: 'No hay espacio local seleccionado',
    recentWorkspaces: 'Espacios recientes',
    use: 'Usar',
    remoteSettings: 'Entorno remoto',
    remoteMode: 'Usar entorno remoto',
    remoteName: 'Nombre de conexión',
    host: 'Host',
    port: 'Puerto',
    username: 'Usuario',
    password: 'Contraseña',
    keyPath: 'Ruta de clave',
    remoteCwd: 'Directorio remoto',
    connectRemote: 'Conectar entorno remoto',
    remoteConnectOk: 'Entorno remoto conectado',
    remoteConnectError: 'No se pudo conectar el entorno remoto',
    currentRemote: 'Entorno remoto actual',
    noRemote: 'No hay entorno remoto conectado',
    savedRemotes: 'Conexiones guardadas',
    connected: 'Conectado',
    disconnected: 'Desconectado',
    interfaceSettings: 'Interfaz',
    noSessions: 'No hay sesiones recuperables.',
    userRole: 'Tú',
    assistantRole: 'Generic Coder',
    languageName: 'Español',
    rounds: (value) => `${value} rondas`,
  },
};

const THEME_LABELS = {
  zh: { solarflare: 'Solar Flare', graphite: 'Graphite', daybreak: 'Daybreak', paperink: 'Paper Ink', obsidian: 'Obsidian', slate: 'Slate', oxide: 'Oxide', noir: 'Noir', cobalt: 'Cobalt', borealis: 'Borealis' },
  en: { solarflare: 'Solar Flare', graphite: 'Graphite', daybreak: 'Daybreak', paperink: 'Paper Ink', obsidian: 'Obsidian', slate: 'Slate', oxide: 'Oxide', noir: 'Noir', cobalt: 'Cobalt', borealis: 'Borealis' },
  es: { solarflare: 'Llamarada Solar', graphite: 'Graphite', daybreak: 'Amanecer', paperink: 'Tinta Papel', obsidian: 'Obsidiana', slate: 'Pizarra', oxide: 'Oxido', noir: 'Noir', cobalt: 'Cobalto', borealis: 'Boreal' },
};

const LANGUAGE_LABELS = {
  zh: { zh: '中文', en: 'English', es: 'Español' },
  en: { zh: 'Chinese', en: 'English', es: 'Spanish' },
  es: { zh: 'Chino', en: 'Inglés', es: 'Español' },
};

const SESSION_TYPE_LABELS = {
  zh: { native_oai: 'Native OAI', oai: 'OAI', native_claude: 'Native Claude', claude: 'Claude' },
  en: { native_oai: 'Native OAI', oai: 'OAI', native_claude: 'Native Claude', claude: 'Claude' },
  es: { native_oai: 'Native OAI', oai: 'OAI', native_claude: 'Native Claude', claude: 'Claude' },
};

const PROTOCOL_PRESET_LABELS = {
  zh: {
    custom: '手动填写',
    deepseek: 'DeepSeek OAI',
    qwen_dashscope: 'Qwen / DashScope（直连 Key）',
    openai_chat: 'OpenAI Chat Completions',
    openai_responses: 'OpenAI Responses',
    anthropic_messages: 'Anthropic Messages',
    openrouter: 'OpenRouter OAI',
    moonshot_oai: 'Kimi / Moonshot API',
    minimax_oai: 'MiniMax OAI',
    doubao_ark: 'Doubao / Ark API',
    hunyuan_oai: 'Tencent Hunyuan OAI',
    baidu_qianfan: 'Baidu Qianfan OAI',
    zhipu_anthropic: '智谱 Anthropic 兼容',
  },
  en: {
    custom: 'Manual entry',
    deepseek: 'DeepSeek OAI',
    qwen_dashscope: 'Qwen / DashScope (Direct key)',
    openai_chat: 'OpenAI Chat Completions',
    openai_responses: 'OpenAI Responses',
    anthropic_messages: 'Anthropic Messages',
    openrouter: 'OpenRouter OAI',
    moonshot_oai: 'Kimi / Moonshot API',
    minimax_oai: 'MiniMax OAI',
    doubao_ark: 'Doubao / Ark API',
    hunyuan_oai: 'Tencent Hunyuan OAI',
    baidu_qianfan: 'Baidu Qianfan OAI',
    zhipu_anthropic: 'Zhipu Anthropic compatible',
  },
  es: {
    custom: 'Manual',
    deepseek: 'DeepSeek OAI',
    qwen_dashscope: 'Qwen / DashScope (clave directa)',
    openai_chat: 'OpenAI Chat Completions',
    openai_responses: 'OpenAI Responses',
    anthropic_messages: 'Anthropic Messages',
    openrouter: 'OpenRouter OAI',
    moonshot_oai: 'Kimi / Moonshot API',
    minimax_oai: 'MiniMax OAI',
    doubao_ark: 'Doubao / Ark API',
    hunyuan_oai: 'Tencent Hunyuan OAI',
    baidu_qianfan: 'Baidu Qianfan OAI',
    zhipu_anthropic: 'Zhipu Anthropic compatible',
  },
};

const MODEL_PRESET_LABELS = {
  zh: {
    custom: '手动填写',
    deepseek_v4_pro: 'DeepSeek V4 Pro',
    deepseek_v4_flash: 'DeepSeek V4 Flash',
    qwen_max_latest: 'Qwen Max Latest',
    qwen_plus_latest: 'Qwen Plus Latest',
    qwen3_coder_plus: 'Qwen3 Coder Plus',
    qwen3_coder_flash: 'Qwen3 Coder Flash',
    kimi_k26: 'Kimi K2.6',
    kimi_k25: 'Kimi K2.5',
    gpt_5_4: 'GPT-5.4',
    gpt_4_1: 'GPT-4.1',
    claude_sonnet_4: 'Claude Sonnet 4',
    claude_opus_4_7: 'Claude Opus 4.7',
    minimax_m27: 'MiniMax M2.7',
    minimax_m1: 'MiniMax M1',
    glm_5_1: 'GLM 5.1',
    doubao_15_pro_32k: 'Doubao 1.5 Pro 32K',
    hunyuan_turbos_latest: 'Hunyuan Turbo S Latest',
    ernie_45_turbo_128k: 'ERNIE 4.5 Turbo 128K',
    mimo_v25: 'MiMo v2.5',
    openrouter_claude: 'OpenRouter Claude Opus',
  },
  en: {
    custom: 'Manual entry',
    deepseek_v4_pro: 'DeepSeek V4 Pro',
    deepseek_v4_flash: 'DeepSeek V4 Flash',
    qwen_max_latest: 'Qwen Max Latest',
    qwen_plus_latest: 'Qwen Plus Latest',
    qwen3_coder_plus: 'Qwen3 Coder Plus',
    qwen3_coder_flash: 'Qwen3 Coder Flash',
    kimi_k26: 'Kimi K2.6',
    kimi_k25: 'Kimi K2.5',
    gpt_5_4: 'GPT-5.4',
    gpt_4_1: 'GPT-4.1',
    claude_sonnet_4: 'Claude Sonnet 4',
    claude_opus_4_7: 'Claude Opus 4.7',
    minimax_m27: 'MiniMax M2.7',
    minimax_m1: 'MiniMax M1',
    glm_5_1: 'GLM 5.1',
    doubao_15_pro_32k: 'Doubao 1.5 Pro 32K',
    hunyuan_turbos_latest: 'Hunyuan Turbo S Latest',
    ernie_45_turbo_128k: 'ERNIE 4.5 Turbo 128K',
    mimo_v25: 'MiMo v2.5',
    openrouter_claude: 'OpenRouter Claude Opus',
  },
  es: {
    custom: 'Manual',
    deepseek_v4_pro: 'DeepSeek V4 Pro',
    deepseek_v4_flash: 'DeepSeek V4 Flash',
    qwen_max_latest: 'Qwen Max Latest',
    qwen_plus_latest: 'Qwen Plus Latest',
    qwen3_coder_plus: 'Qwen3 Coder Plus',
    qwen3_coder_flash: 'Qwen3 Coder Flash',
    kimi_k26: 'Kimi K2.6',
    kimi_k25: 'Kimi K2.5',
    gpt_5_4: 'GPT-5.4',
    gpt_4_1: 'GPT-4.1',
    claude_sonnet_4: 'Claude Sonnet 4',
    claude_opus_4_7: 'Claude Opus 4.7',
    minimax_m27: 'MiniMax M2.7',
    minimax_m1: 'MiniMax M1',
    glm_5_1: 'GLM 5.1',
    doubao_15_pro_32k: 'Doubao 1.5 Pro 32K',
    hunyuan_turbos_latest: 'Hunyuan Turbo S Latest',
    ernie_45_turbo_128k: 'ERNIE 4.5 Turbo 128K',
    mimo_v25: 'MiMo v2.5',
    openrouter_claude: 'OpenRouter Claude Opus',
  },
};

const feedEl = document.getElementById('chat-feed');
const composerEl = document.getElementById('composer-input');
const composerForm = document.getElementById('composer-form');
const sendButton = document.getElementById('send-button');
const statusLabel = document.getElementById('status-label');
const modelLabel = document.getElementById('model-label');
const statusDot = document.getElementById('status-dot');
const modelSelect = document.getElementById('model-select');
const themeSelect = document.getElementById('theme-select');
const themePill = document.getElementById('theme-pill');
const languagePill = document.getElementById('language-pill');
const languageSelect = document.getElementById('language-select');
const messageCount = document.getElementById('message-count');
const toastEl = document.getElementById('toast');
const sessionsDialog = document.getElementById('sessions-dialog');
const settingsDialog = document.getElementById('settings-dialog');
const sessionsList = document.getElementById('sessions-list');
const lastReplyTimeEl = document.getElementById('last-reply-time');

const modelPresetSelect = document.getElementById('model-preset-select');
const protocolPresetSelect = document.getElementById('protocol-preset-select');
const sessionTypeSelect = document.getElementById('session-type-select');
const providerInput = document.getElementById('provider-input');
const displayNameInput = document.getElementById('display-name-input');
const modelNameInput = document.getElementById('model-name-input');
const baseUrlInput = document.getElementById('base-url-input');
const apiKeyInput = document.getElementById('api-key-input');
const apiKeyToggle = document.getElementById('api-key-toggle');
const saveLlmButton = document.getElementById('save-llm-settings');

const workspaceStatus = document.getElementById('workspace-status');
const workspaceNameInput = document.getElementById('workspace-name-input');
const workspacePathInput = document.getElementById('workspace-path-input');
const browseWorkspacePathButton = document.getElementById('browse-workspace-path');
const saveWorkspaceButton = document.getElementById('save-workspace-settings');
const workspaceList = document.getElementById('workspace-list');

const remoteStatus = document.getElementById('remote-status');
const remoteEnabledInput = document.getElementById('remote-enabled-input');
const remoteNameInput = document.getElementById('remote-name-input');
const remoteHostInput = document.getElementById('remote-host-input');
const remotePortInput = document.getElementById('remote-port-input');
const remoteUsernameInput = document.getElementById('remote-username-input');
const remotePasswordInput = document.getElementById('remote-password-input');
const remoteKeyPathInput = document.getElementById('remote-key-path-input');
const remoteCwdInput = document.getElementById('remote-cwd-input');
const connectRemoteButton = document.getElementById('connect-remote-button');
const remoteConfigList = document.getElementById('remote-config-list');
const skillsList = document.getElementById('skills-list');
const skillsStatus = document.getElementById('skills-status');
const skillUrlInput = document.getElementById('skill-url-input');
const installSkillButton = document.getElementById('install-skill-button');
const WORKSPACE_PICKER_TOKEN_KEY = 'generic-coder-workspace-picker-token';

function hydrateWorkspacePickerToken() {
  const hash = window.location.hash.startsWith('#') ? window.location.hash.slice(1) : '';
  const params = new URLSearchParams(hash);
  const tokenFromHash = params.get('picker_token');
  if (tokenFromHash) {
    window.sessionStorage.setItem(WORKSPACE_PICKER_TOKEN_KEY, tokenFromHash);
    params.delete('picker_token');
    const nextHash = params.toString();
    const nextUrl = `${window.location.pathname}${window.location.search}${nextHash ? `#${nextHash}` : ''}`;
    window.history.replaceState(null, '', nextUrl);
  }
  state.workspacePickerToken = window.sessionStorage.getItem(WORKSPACE_PICKER_TOKEN_KEY) || '';
}

function t(key) {
  return I18N[state.locale][key];
}

function preferredLocale() {
  const stored = window.localStorage.getItem('generic-coder-locale');
  if (stored && LANGUAGE_OPTIONS.includes(stored)) return stored;
  const raw = (navigator.language || 'en').toLowerCase();
  if (raw.startsWith('zh')) return 'zh';
  if (raw.startsWith('es')) return 'es';
  return 'en';
}

function syncDocumentLanguage() {
  document.documentElement.lang = state.locale === 'zh' ? 'zh-CN' : state.locale;
}

function renderThemeOptions() {
  themeSelect.innerHTML = THEME_OPTIONS.map((theme) => `<option value="${theme}">${THEME_LABELS[state.locale][theme] || theme}</option>`).join('');
  themeSelect.value = state.theme;
}

function renderLanguageOptions() {
  languageSelect.innerHTML = LANGUAGE_OPTIONS.map((locale) => `<option value="${locale}">${LANGUAGE_LABELS[state.locale][locale] || locale}</option>`).join('');
  languageSelect.value = state.locale;
}

function renderModelOptions() {
  if (!state.models.length) {
    modelSelect.innerHTML = `<option value="">${t('modelOffline')}</option>`;
    modelSelect.disabled = true;
    return;
  }
  modelSelect.disabled = false;
  modelSelect.innerHTML = state.models.map((item) => `<option value="${item.index}">${escapeHtml(item.label)}</option>`).join('');
  modelSelect.value = String(state.currentModelIndex);
}

function renderSessionTypeOptions() {
  sessionTypeSelect.innerHTML = SESSION_TYPE_OPTIONS.map((value) => `<option value="${value}">${SESSION_TYPE_LABELS[state.locale][value] || value}</option>`).join('');
  sessionTypeSelect.value = state.llmForm.session_type || 'native_oai';
}

function inferProtocolPreset(form) {
  const currentApiMode = String(form.api_mode || 'chat_completions').toLowerCase();
  const currentSessionType = String(form.session_type || 'native_oai').toLowerCase();
  const currentBaseUrl = canonicalizeBaseUrl(form.apibase);
  for (const key of PROTOCOL_PRESET_OPTIONS) {
    if (key === 'custom') continue;
    const preset = PROTOCOL_PRESETS[key];
    if (!preset) continue;
    if (preset.sessionType !== currentSessionType) continue;
    if ((preset.api_mode || 'chat_completions') !== currentApiMode) continue;
    if (canonicalizeBaseUrl(preset.apibase) !== currentBaseUrl) continue;
    return key;
  }
  return 'custom';
}

function inferModelPreset(form) {
  const currentModel = String(form.model || '').trim().toLowerCase();
  const currentProvider = String(form.provider || '').trim().toLowerCase();
  const currentSessionType = String(form.session_type || 'native_oai').trim().toLowerCase();
  const currentProtocolPreset = inferProtocolPreset(form);
  if (currentModel === 'deepseek-chat' || currentModel === 'deepseek-reasoner') {
    return 'custom';
  }
  let fallback = 'custom';
  for (const key of MODEL_PRESET_OPTIONS) {
    if (key === 'custom') continue;
    const preset = MODEL_PRESETS[key];
    if ((preset?.model || '').trim().toLowerCase() !== currentModel) {
      continue;
    }
    const presetSessionType = preset?.protocolPreset
      ? PROTOCOL_PRESETS[preset.protocolPreset]?.sessionType
      : '';
    if (presetSessionType && presetSessionType !== currentSessionType) {
      continue;
    }
    if (preset?.protocolPreset && preset.protocolPreset === currentProtocolPreset) {
      return key;
    }
    if (preset?.provider && preset.provider.trim().toLowerCase() === currentProvider) {
      return key;
    }
    if (fallback === 'custom') fallback = key;
  }
  return fallback;
}

function canonicalizeBaseUrl(url) {
  const normalized = String(url || '').trim().toLowerCase().replace(/\/+$/, '');
  if (normalized === 'https://api.moonshot.cn/v1') {
    return 'https://api.moonshot.ai/v1';
  }
  return normalized;
}

function hydrateLlmFormMetadata() {
  state.llmForm.protocol_preset = state.llmForm.protocol_preset || inferProtocolPreset(state.llmForm);
  state.llmForm.model_preset = state.llmForm.model_preset || inferModelPreset(state.llmForm);
  if (!state.llmForm.provider) {
    state.llmForm.provider = MODEL_PRESETS[state.llmForm.model_preset]?.provider
      || PROTOCOL_PRESETS[state.llmForm.protocol_preset]?.provider
      || '';
  }
  if (!state.llmForm.name) {
    state.llmForm.name = MODEL_PRESETS[state.llmForm.model_preset]?.displayName
      || state.llmForm.model
      || '';
  }
}

function renderProtocolPresetOptions() {
  protocolPresetSelect.innerHTML = PROTOCOL_PRESET_OPTIONS.map((value) => `<option value="${value}">${PROTOCOL_PRESET_LABELS[state.locale][value] || value}</option>`).join('');
  protocolPresetSelect.value = state.llmForm.protocol_preset || 'custom';
}

function renderModelPresetOptions() {
  modelPresetSelect.innerHTML = MODEL_PRESET_OPTIONS.map((value) => `<option value="${value}">${MODEL_PRESET_LABELS[state.locale][value] || value}</option>`).join('');
  modelPresetSelect.value = state.llmForm.model_preset || 'custom';
}

function renderApiKeyToggle() {
  const isVisible = apiKeyInput.type === 'text';
  apiKeyToggle.textContent = isVisible ? t('hideSecret') : t('showSecret');
  apiKeyToggle.setAttribute('aria-pressed', isVisible ? 'true' : 'false');
  apiKeyToggle.setAttribute('aria-label', isVisible ? t('hideSecret') : t('showSecret'));
}

function syncPresetSelectionsFromFields() {
  state.llmForm.protocol_preset = inferProtocolPreset({
    session_type: sessionTypeSelect.value,
    api_mode: state.llmForm.api_mode || 'chat_completions',
    apibase: baseUrlInput.value.trim(),
  });
  state.llmForm.model_preset = inferModelPreset({
    session_type: sessionTypeSelect.value,
    api_mode: state.llmForm.api_mode || 'chat_completions',
    provider: providerInput.value.trim(),
    apibase: baseUrlInput.value.trim(),
    model: modelNameInput.value.trim(),
  });
  renderProtocolPresetOptions();
  renderModelPresetOptions();
}

function markManualProtocolEntry() {
  state.llmForm.protocol_preset = 'custom';
  state.llmForm.api_mode = state.llmForm.api_mode || 'chat_completions';
  protocolPresetSelect.value = 'custom';
}

function markManualModelEntry() {
  state.llmForm.model_preset = 'custom';
  modelPresetSelect.value = 'custom';
}

function applyProtocolPreset(presetKey) {
  const preset = PROTOCOL_PRESETS[presetKey] || PROTOCOL_PRESETS.custom;
  state.llmForm.protocol_preset = presetKey;
  state.llmForm.api_mode = preset.api_mode || 'chat_completions';
  if (presetKey === 'custom') {
    renderProtocolPresetOptions();
    return;
  }
  sessionTypeSelect.value = preset.sessionType;
  if (preset.provider) providerInput.value = preset.provider;
  if (preset.apibase) baseUrlInput.value = preset.apibase;
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
  if (preset.provider) providerInput.value = preset.provider;
  if (preset.model) modelNameInput.value = preset.model;
  if (preset.displayName) displayNameInput.value = preset.displayName;
  renderModelPresetOptions();
}

function mergeRemoteState(remote) {
  const incoming = remote || {};
  return {
    form: { ...DEFAULT_REMOTE_FORM, ...(incoming.form || {}) },
    configs: incoming.configs || [],
    active_connections: incoming.active_connections || [],
    connected: Boolean(incoming.connected),
  };
}

function hydrateSettings(data) {
  state.llmForm = { ...DEFAULT_LLM_FORM, ...(data.llm_form || {}) };
  hydrateLlmFormMetadata();
  state.workspace = {
    active: data.workspace?.active || null,
    workspaces: data.workspace?.workspaces || [],
    recent_folders: data.workspace?.recent_folders || [],
  };
  state.remote = mergeRemoteState(data.remote);
  if (data.models?.models) {
    state.models = data.models.models;
  }
  if (Number.isInteger(data.models?.current_index)) {
    state.currentModelIndex = data.models.current_index;
  }
  renderSettingsState();
}

function renderWorkspaceChoices() {
  const entries = [];
  const seen = new Set();
  if (state.workspace.active?.path) {
    entries.push({ path: state.workspace.active.path, name: state.workspace.active.name || '', label: `${t('currentWorkspace')}: ${state.workspace.active.path}` });
    seen.add(state.workspace.active.path);
  }
  (state.workspace.workspaces || []).forEach((item) => {
    if (!item.path || seen.has(item.path)) return;
    entries.push({ path: item.path, name: item.name || '', label: item.path });
    seen.add(item.path);
  });
  (state.workspace.recent_folders || []).forEach((path) => {
    if (!path || seen.has(path)) return;
    entries.push({ path, name: '', label: path });
    seen.add(path);
  });
  if (!entries.length) {
    workspaceList.innerHTML = `<div class="settings-empty">${t('noWorkspace')}</div>`;
    return;
  }
  workspaceList.innerHTML = entries.map((item) => `<button type="button" class="settings-chip-button" data-workspace-path="${escapeHtml(item.path)}" data-workspace-name="${escapeHtml(item.name)}">${escapeHtml(item.label)}</button>`).join('');
}

function inferWorkspaceNameFromPath(path) {
  if (!path) return '';
  const normalized = String(path).replace(/[\\/]+$/, '');
  const parts = normalized.split(/[\\/]/).filter(Boolean);
  return parts.length ? parts[parts.length - 1] : normalized;
}

function shouldAutofillWorkspaceName(previousPath) {
  const currentName = workspaceNameInput.value.trim();
  return !currentName || state.workspaceNameAutoFilled;
}

function renderRemoteChoices() {
  const configs = state.remote.configs || [];
  if (!configs.length) {
    remoteConfigList.innerHTML = `<div class="settings-empty">${t('noRemote')}</div>`;
    return;
  }
  remoteConfigList.innerHTML = configs.map((item) => {
    const connected = (state.remote.active_connections || []).includes(item.name);
    const label = `${item.name} · ${item.username}@${item.host}:${item.port} · ${connected ? t('connected') : t('disconnected')}`;
    return `<button type="button" class="settings-chip-button" data-remote-name="${escapeHtml(item.name || '')}" data-remote-host="${escapeHtml(item.host || '')}" data-remote-port="${escapeHtml(String(item.port || 22))}" data-remote-username="${escapeHtml(item.username || 'root')}">${escapeHtml(label)}</button>`;
  }).join('');
}

function renderSettingsState() {
  renderThemeOptions();
  renderLanguageOptions();
  renderModelOptions();
  renderSessionTypeOptions();
  renderProtocolPresetOptions();
  renderModelPresetOptions();

  providerInput.value = state.llmForm.provider || '';
  displayNameInput.value = state.llmForm.name || '';
  modelNameInput.value = state.llmForm.model || '';
  baseUrlInput.value = state.llmForm.apibase || '';
  apiKeyInput.value = state.llmForm.apikey || '';
  renderApiKeyToggle();

  workspaceNameInput.value = state.workspace.active?.name || '';
  workspacePathInput.value = state.workspace.active?.path || '';
  state.workspaceNameDirty = false;
  state.workspaceNameAutoFilled = false;
  workspaceStatus.textContent = state.workspace.active?.path ? `${t('currentWorkspace')}: ${state.workspace.active.path}` : t('noWorkspace');
  renderWorkspaceChoices();

  const remoteForm = state.remote.form || DEFAULT_REMOTE_FORM;
  remoteEnabledInput.checked = Boolean(remoteForm.enabled);
  remoteNameInput.value = remoteForm.server_name || remoteForm.name || '';
  remoteHostInput.value = remoteForm.host || '';
  remotePortInput.value = String(remoteForm.port || 22);
  remoteUsernameInput.value = remoteForm.username || 'root';
  remotePasswordInput.value = remoteForm.password || '';
  remoteKeyPathInput.value = remoteForm.key_path || '';
  remoteCwdInput.value = remoteForm.cwd || '';
  remoteStatus.textContent = state.remote.connected && (remoteForm.server_name || remoteForm.name)
    ? `${t('currentRemote')}: ${remoteForm.server_name || remoteForm.name} · ${remoteForm.cwd || '~'}`
    : t('noRemote');
  renderRemoteChoices();
  loadSkills();
}

function applyLocale(locale, persist = true) {
  if (!LANGUAGE_OPTIONS.includes(locale)) return;
  state.locale = locale;
  if (persist) {
    window.localStorage.setItem('generic-coder-locale', locale);
  }
  syncDocumentLanguage();
  document.querySelectorAll('[data-i18n]').forEach((node) => {
    node.textContent = t(node.dataset.i18n);
  });
  document.querySelectorAll('[data-i18n-placeholder]').forEach((node) => {
    node.setAttribute('placeholder', t(node.dataset.i18nPlaceholder));
  });
  renderSettingsState();
  setTheme(state.theme, false);
  renderMessages();
}

function escapeHtml(text) {
  return String(text)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

function renderContent(text, streaming = false) {
  if (!text) return `<div class="message-card__body${streaming ? ' is-streaming' : ''}">...</div>`;

  const placeholders = [];

  // ── Extract blocks that need special handling ──────────
  let html = text;

  // Code blocks (protect from escaping)
  html = html.replace(/```(\w*)\n([\s\S]*?)```/g, (_, lang, code) => {
    const idx = placeholders.length;
    placeholders.push({ type: 'code', lang, code });
    return `\x00C${idx}\x00`;
  });

  // Thinking blocks
  html = html.replace(/<thinking>([\s\S]*?)<\/thinking>/g, (_, content) => {
    const idx = placeholders.length;
    placeholders.push({ type: 'thinking', content });
    return `\x00T${idx}\x00`;
  });

  // Summary badges
  html = html.replace(/<summary>(.*?)<\/summary>/g, (_, content) => {
    const idx = placeholders.length;
    placeholders.push({ type: 'summary', content });
    return `\x00S${idx}\x00`;
  });

  // ── Escape remaining text ──────────────────────────────
  html = escapeHtml(html);

  // ── Inline formatting ──────────────────────────────────
  html = html.replace(/`([^`]+)`/g, '<code class="inline-code">$1</code>');
  html = html.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
  html = html.replace(/\*([^*]+)\*/g, '<em>$1</em>');

  // ── Status tags ────────────────────────────────────────
  html = html.replace(/\[Action\]/g, '<span class="action-tag">Action</span>');
  html = html.replace(/\[Status\]/g, '<span class="status-tag">Status</span>');
  html = html.replace(/\[Error\]/g, '<span class="error-tag">Error</span>');
  html = html.replace(/\[Warn\]/g, '<span class="warn-tag">Warn</span>');
  html = html.replace(/\[Info\]/g, '<span class="info-tag">Info</span>');
  html = html.replace(/\[Backup\]/g, '<span class="backup-tag">Backup</span>');
  html = html.replace(/✅/g, '<span class="status-icon ok">✅</span>');
  html = html.replace(/❌/g, '<span class="status-icon err">❌</span>');
  html = html.replace(/⚠️/g, '<span class="status-icon warn">⚠️</span>');

  // ── Line breaks ────────────────────────────────────────
  html = html.replace(/\n/g, '<br />');

  // ── Restore placeholders ───────────────────────────────
  html = html.replace(/\x00C(\d+)\x00/g, (_, idx) => {
    const p = placeholders[idx];
    if (!p) return '';
    const langLabel = p.lang ? `<span class="code-lang">${escapeHtml(p.lang)}</span>` : '';
    return `<div class="code-block-wrapper">${langLabel}<pre><code>${highlightCode(escapeHtml(p.code), p.lang)}</code></pre></div>`;
  });

  html = html.replace(/\x00T(\d+)\x00/g, (_, idx) => {
    const p = placeholders[idx];
    if (!p) return '';
    const escaped = escapeHtml(p.content).replace(/\n/g, '<br />');
    const preview = p.content.replace(/<[^>]*>/g, '').trim().slice(0, 80);
    return `<details class="thinking-block"><summary class="thinking-summary">Thinking</summary><div class="thinking-content">${escaped}</div></details>`;
  });

  html = html.replace(/\x00S(\d+)\x00/g, (_, idx) => {
    const p = placeholders[idx];
    if (!p) return '';
    return `<span class="summary-badge">${escapeHtml(p.content)}</span>`;
  });

  return `<div class="message-card__body${streaming ? ' is-streaming' : ''}">${html}</div>`;
}

function highlightCode(code, lang) {
  // Simple keyword-based syntax highlighting
  const kwMap = {
    python: ['def', 'class', 'import', 'from', 'return', 'if', 'else', 'elif', 'for', 'while', 'try', 'except', 'finally', 'with', 'as', 'yield', 'lambda', 'pass', 'break', 'continue', 'and', 'or', 'not', 'in', 'is', 'None', 'True', 'False', 'self', 'raise', 'async', 'await'],
    javascript: ['function', 'const', 'let', 'var', 'return', 'if', 'else', 'for', 'while', 'try', 'catch', 'finally', 'class', 'import', 'export', 'default', 'from', 'async', 'await', 'new', 'this', 'null', 'undefined', 'true', 'false', 'typeof', 'instanceof'],
    bash: ['echo', 'export', 'source', 'cd', 'ls', 'rm', 'cp', 'mv', 'mkdir', 'grep', 'sed', 'awk', 'cat', 'chmod', 'sudo', 'git', 'pip', 'npm', 'python', 'node', 'docker', 'curl', 'wget'],
    typescript: ['function', 'const', 'let', 'var', 'return', 'if', 'else', 'class', 'interface', 'type', 'enum', 'import', 'export', 'async', 'await'],
    json: [],
    html: [],
    css: [],
    sql: ['SELECT', 'FROM', 'WHERE', 'INSERT', 'UPDATE', 'DELETE', 'CREATE', 'ALTER', 'DROP', 'TABLE', 'INTO', 'VALUES', 'SET', 'AND', 'OR', 'JOIN', 'LEFT', 'RIGHT', 'INNER', 'GROUP', 'BY', 'ORDER', 'LIMIT', 'HAVING', 'COUNT', 'SUM', 'AVG', 'AS', 'ON', 'NOT', 'NULL', 'PRIMARY', 'KEY', 'FOREIGN', 'INDEX'],
    diff: [],
    text: [],
  };

  const keywords = kwMap[lang] || kwMap.python;
  if (!keywords.length) return code;

  // Tokenize and highlight
  let result = '';
  const regex = new RegExp(
    `(^#.*$)|("[^"]*")|('[^']*')|(\\b\\d+\\.?\\d*\\b)|(\\b(?:${keywords.join('|')})\\b)|(^\\[.*\\]$)`,
    'gim'
  );

  let lastIdx = 0;
  let match;
  while ((match = regex.exec(code)) !== null) {
    result += code.slice(lastIdx, match.index);
    if (match[1]) result += `<span class="hl-comment">${match[1]}</span>`;       // comment
    else if (match[2]) result += `<span class="hl-string">${match[2]}</span>`;   // double-quoted
    else if (match[3]) result += `<span class="hl-string">${match[3]}</span>`;   // single-quoted
    else if (match[4]) result += `<span class="hl-number">${match[4]}</span>`;   // number
    else if (match[5]) result += `<span class="hl-keyword">${match[5]}</span>`;  // keyword
    else if (match[6]) result += `<span class="hl-tag">${match[6]}</span>`;      // bracket tag
    lastIdx = regex.lastIndex;
  }
  result += code.slice(lastIdx);
  return result;
}

function roleLabel(role) {
  return role === 'user' ? t('userRole') : t('assistantRole');
}

function renderMessages() {
  feedEl.innerHTML = state.messages.map((message) => {
    const roleClass = message.role === 'user' ? 'message-card--user' : 'message-card--assistant';
    const streaming = Boolean(message.streaming);
    const content = message.content || '';
    const isLong = !streaming && message.role === 'assistant' && (content.length > 3000 || content.split('\n').length > 50);
    return `
      <article class="message-card ${roleClass}" data-message-len="${content.length}">
        <div class="message-card__meta">
          <span>${roleLabel(message.role)}</span>
          ${isLong ? '<span class="message-card__length-badge">Long output</span>' : ''}
        </div>
        <div class="message-card__body-wrapper ${isLong ? 'message-card__body-wrapper--collapsed' : ''}">
          ${renderContent(content, streaming)}
        </div>
        ${isLong ? '<button class="expand-output-btn">Show full output ▼</button>' : ''}
      </article>
    `;
  }).join('');

  messageCount.textContent = String(state.messages.length);
  feedEl.scrollTop = feedEl.scrollHeight;
  const lastAssistant = [...state.messages].reverse().find((item) => item.role === 'assistant' && !item.streaming);
  lastReplyTimeEl.textContent = String(lastAssistant ? Date.now() : 0);

  // Wire up expand buttons
  feedEl.querySelectorAll('.expand-output-btn').forEach((btn) => {
    btn.addEventListener('click', () => {
      const wrapper = btn.parentElement.querySelector('.message-card__body-wrapper');
      const isCollapsed = wrapper.classList.contains('message-card__body-wrapper--collapsed');
      if (isCollapsed) {
        wrapper.classList.remove('message-card__body-wrapper--collapsed');
        btn.textContent = 'Show less ▲';
      } else {
        wrapper.classList.add('message-card__body-wrapper--collapsed');
        btn.textContent = 'Show full output ▼';
        // Scroll to the start of this message
        btn.parentElement.scrollIntoView({ behavior: 'smooth', block: 'start' });
      }
    });
  });
}

function setRunning(isRunning) {
  state.isRunning = isRunning;
  statusLabel.textContent = isRunning ? t('running') : t('idle');
  statusDot.classList.toggle('is-live', isRunning);
  sendButton.disabled = isRunning;
  document.getElementById('stop-button').disabled = !isRunning;
  document.getElementById('new-chat-button').disabled = isRunning;
  document.getElementById('turn-progress').hidden = !isRunning;
  if (!isRunning) {
    document.getElementById('turn-fill').style.width = '0%';
  }
}

// Override renderMessages to parse turn info from streaming content
const _origRenderMessages = renderMessages;
renderMessages = function() {
  _origRenderMessages();

  // Parse turn from last streaming message
  if (state.isRunning) {
    const lastMsg = [...state.messages].reverse().find((item) => item.streaming);
    if (lastMsg) {
      const match = lastMsg.content.match(/LLM Running \(Turn (\d+)\)/);
      if (match) {
        const turn = parseInt(match[1]);
        const maxTurns = 70;
        const pct = Math.round((turn / maxTurns) * 100);
        document.getElementById('turn-progress').hidden = false;
        document.getElementById('turn-label').textContent = `T${turn}/${maxTurns}`;
        document.getElementById('turn-fill').style.width = `${pct}%`;
      }
    }
  }
};

function showToast(message) {
  toastEl.textContent = message;
  toastEl.hidden = false;
  clearTimeout(showToast._timer);
  showToast._timer = setTimeout(() => {
    toastEl.hidden = true;
  }, 3200);
}

function setTheme(theme, persist = true) {
  state.theme = theme;
  document.documentElement.dataset.theme = theme;
  document.body.dataset.theme = theme;
  themePill.textContent = THEME_LABELS[state.locale][theme] || theme;
  languagePill.textContent = I18N[state.locale].languageName;
  if (persist) {
    window.localStorage.setItem('generic-coder-theme', theme);
  }
}

// ── Skills Management ────────────────────────────────────

async function loadSkills() {
  try {
    const res = await fetch('/api/skills');
    const data = await res.json();
    renderSkills(data);
  } catch (e) {
    skillsStatus.textContent = 'Failed to load skills';
  }
}

function renderSkills(skills) {
  if (!skills || !skills.length) {
    skillsList.innerHTML = '<div class="skills-empty">No skills installed. Paste a GitHub or Clawhub URL above to install one.</div>';
    skillsStatus.textContent = skills ? `${skills.length} skill(s) installed` : '0 skills installed';
    return;
  }

  skillsStatus.textContent = `${skills.length} skill(s) installed`;

  skillsList.innerHTML = skills.map((s) => {
    const statusLabel = s.enabled ? 'Enabled' : 'Disabled';
    const statusClass = s.enabled ? 'skill-badge--enabled' : 'skill-badge--disabled';
    const sourceDisplay = s.source_url ? s.source_url.replace(/^https?:\/\//, '').slice(0, 50) + (s.source_url.length > 50 ? '...' : '') : '';
    const descDisplay = s.description || 'No description';
    const filesLabel = `${s.file_count || s.files.length} file(s)`;

    return `
      <div class="skill-card" data-skill-name="${escapeHtml(s.name)}">
        <div class="skill-card__header">
          <span class="skill-card__name">${escapeHtml(s.display_name || s.name)}</span>
          <div class="skill-card__actions">
            <span class="skill-badge ${statusClass}">${statusLabel}</span>
            <span class="skill-badge skill-badge--count">${filesLabel}</span>
          </div>
        </div>
        <div class="skill-card__desc">${escapeHtml(descDisplay)}</div>
        <div class="skill-card__meta">
          ${sourceDisplay ? `<span>Source: <a href="${escapeHtml(s.source_url)}" target="_blank" rel="noopener">${escapeHtml(sourceDisplay)}</a></span>` : ''}
          ${s.version ? `<span>v${escapeHtml(s.version)}</span>` : ''}
          ${s.installed_at ? `<span>Installed: ${escapeHtml(s.installed_at.slice(0, 10))}</span>` : ''}
        </div>
        <div class="skill-card__actions">
          <button class="skill-btn skill-btn--primary skill-preview-btn" data-skill="${escapeHtml(s.name)}">Preview</button>
          <button class="skill-btn skill-toggle-btn" data-skill="${escapeHtml(s.name)}">${s.enabled ? 'Disable' : 'Enable'}</button>
          <button class="skill-btn skill-upgrade-btn" data-skill="${escapeHtml(s.name)}">Upgrade</button>
          <button class="skill-btn skill-btn--danger skill-delete-btn" data-skill="${escapeHtml(s.name)}">Delete</button>
        </div>
      </div>
    `;
  }).join('');

  // Wire up buttons
  skillsList.querySelectorAll('.skill-toggle-btn').forEach((btn) => {
    btn.addEventListener('click', () => toggleSkill(btn.dataset.skill));
  });
  skillsList.querySelectorAll('.skill-delete-btn').forEach((btn) => {
    btn.addEventListener('click', () => deleteSkill(btn.dataset.skill));
  });
  skillsList.querySelectorAll('.skill-upgrade-btn').forEach((btn) => {
    btn.addEventListener('click', () => upgradeSkill(btn.dataset.skill));
  });
  skillsList.querySelectorAll('.skill-preview-btn').forEach((btn) => {
    btn.addEventListener('click', () => previewSkill(btn.dataset.skill));
  });
}

async function installSkill() {
  const url = skillUrlInput.value.trim();
  if (!url) {
    showToast('Please enter a skill URL');
    return;
  }

  installSkillButton.disabled = true;
  installSkillButton.textContent = 'Installing...';

  try {
    const res = await fetch('/api/skills/install', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ url }),
    });
    const data = await res.json();
    if (!res.ok) {
      showToast(data.error || 'Failed to install skill');
      return;
    }
    skillUrlInput.value = '';
    showToast(`Skill "${data.display_name || data.name}" installed successfully`);
    await loadSkills();
  } catch (e) {
    showToast(`Install failed: ${e.message}`);
  } finally {
    installSkillButton.disabled = false;
    installSkillButton.textContent = 'Install';
  }
}

async function toggleSkill(name) {
  try {
    const res = await fetch(`/api/skills/${encodeURIComponent(name)}/toggle`, {
      method: 'POST',
    });
    const data = await res.json();
    if (!res.ok) {
      showToast(data.error || 'Failed to toggle skill');
      return;
    }
    showToast(`Skill "${data.display_name || data.name}" ${data.enabled ? 'enabled' : 'disabled'}`);
    await loadSkills();
  } catch (e) {
    showToast(`Toggle failed: ${e.message}`);
  }
}

async function deleteSkill(name) {
  if (!confirm(`Delete skill "${name}"? This removes all its files.`)) return;

  try {
    const res = await fetch(`/api/skills/${encodeURIComponent(name)}/delete`, {
      method: 'POST',
    });
    const data = await res.json();
    if (!res.ok) {
      showToast(data.error || 'Failed to delete skill');
      return;
    }
    showToast(`Skill "${name}" deleted`);
    await loadSkills();
  } catch (e) {
    showToast(`Delete failed: ${e.message}`);
  }
}

async function upgradeSkill(name) {
  const btn = skillsList.querySelector(`.skill-upgrade-btn[data-skill="${CSS.escape(name)}"]`);
  if (btn) {
    btn.disabled = true;
    btn.textContent = 'Upgrading...';
  }

  try {
    const res = await fetch(`/api/skills/${encodeURIComponent(name)}/upgrade`, {
      method: 'POST',
    });
    const data = await res.json();
    if (!res.ok) {
      showToast(data.error || 'Failed to upgrade skill');
      return;
    }
    showToast(`Skill "${data.display_name || data.name}" upgraded`);
    await loadSkills();
  } catch (e) {
    showToast(`Upgrade failed: ${e.message}`);
  } finally {
    if (btn) {
      btn.disabled = false;
      btn.textContent = 'Upgrade';
    }
  }
}

async function previewSkill(name) {
  try {
    const res = await fetch(`/api/skills/${encodeURIComponent(name)}/preview`);
    const data = await res.json();
    if (!res.ok) {
      showToast(data.error || 'Failed to preview skill');
      return;
    }
    // Show preview as a message in the chat feed
    const previewText = `## Skill Preview: ${escapeHtml(name)}\n\n**File:** \`${escapeHtml(data.file)}\`\n\n\`\`\`markdown\n${data.content}\n\`\`\``;
    state.messages.push({ role: 'assistant', content: previewText, streaming: false });
    renderMessages();
    feedEl.scrollTop = feedEl.scrollHeight;
  } catch (e) {
    showToast(`Preview failed: ${e.message}`);
  }
}

// ── Workflow Builder ─────────────────────────────────────

function renderWorkflow() {
  const pipeline = document.getElementById('workflow-pipeline');
  const saveBtn = document.getElementById('workflow-save-button');
  const resetBtn = document.getElementById('workflow-reset-button');
  const validation = document.getElementById('workflow-validation');
  const activeIndicator = document.getElementById('workflow-active');

  if (!state.workflowNodes.length) {
    pipeline.innerHTML = '<div class="workflow-empty">Add modes to build a pipeline</div>';
    saveBtn.hidden = true;
    resetBtn.hidden = true;
    validation.hidden = true;
    activeIndicator.hidden = true;
    pipeline.classList.remove('workflow-pipeline--has-items');
    return;
  }

  pipeline.classList.add('workflow-pipeline--has-items');
  saveBtn.hidden = false;
  resetBtn.hidden = false;

  // Render nodes
  pipeline.innerHTML = state.workflowNodes.map((node, i) => {
    const emoji = { work: '', plan: '', review: '' }[node.mode] || '';
    const label = { work: 'Work', plan: 'Plan', review: 'Review' }[node.mode] || node.mode;
    const activeClass = state.workflowActive && i === state.workflowCurrentNode ? ' workflow-node--active' : '';
    const completedClass = node.completed ? ' workflow-node--completed' : '';
    return `
      <div class="workflow-node workflow-node--${node.mode}${activeClass}${completedClass}"
           draggable="true"
           data-node-index="${i}">
        <span class="workflow-node__emoji">${emoji}</span>
        <span class="workflow-node__label">${label}</span>
        <button class="workflow-node__remove" data-remove="${i}" title="Remove">×</button>
      </div>
    `;
  }).join('');

  // Add arrow connectors between nodes
  // Re-insert with arrows by rewriting innerHTML
  const nodes = pipeline.querySelectorAll('.workflow-node');
  if (nodes.length > 1) {
    // Insert arrows via CSS pseudo-elements instead

  // Wire up drag events
  nodes.forEach((el) => {
    el.addEventListener('dragstart', (e) => {
      e.dataTransfer.setData('text/plain', el.dataset.nodeIndex);
      el.classList.add('workflow-node--dragging');
    });
    el.addEventListener('dragend', () => {
      el.classList.remove('workflow-node--dragging');
    });
  });
  }

  // Wire up remove buttons
  pipeline.querySelectorAll('.workflow-node__remove').forEach((btn) => {
    btn.addEventListener('click', (e) => {
      e.stopPropagation();
      const idx = parseInt(btn.dataset.remove);
      removeFromPipeline(idx);
    });
  });

  // Wire up drop zone
  pipeline.addEventListener('dragover', (e) => {
    e.preventDefault();
    e.dataTransfer.dropEffect = 'move';
  });

  pipeline.addEventListener('drop', (e) => {
    e.preventDefault();
    const fromIdx = parseInt(e.dataTransfer.getData('text/plain'));
    if (!isNaN(fromIdx)) {
      // Find the drop position
      const target = e.target.closest('.workflow-node');
      if (target) {
        const toIdx = parseInt(target.dataset.nodeIndex);
        moveNode(fromIdx, toIdx);
      }
    }
  });

  // Validate and show errors
  const v = validateWorkflow();
  if (!v.valid) {
    validation.hidden = false;
    validation.textContent = v.reason;
    saveBtn.disabled = true;
  } else {
    validation.hidden = true;
    saveBtn.disabled = false;
  }

  // Show active indicator if running
  if (state.workflowActive) {
    activeIndicator.hidden = false;
    const currentNode = state.workflowNodes[state.workflowCurrentNode];
    if (currentNode) {
      const label = { work: 'Work', plan: 'Plan', review: 'Review' }[currentNode.mode] || currentNode.mode;
      activeIndicator.textContent = `Running: ${label} (${state.workflowCurrentNode + 1}/${state.workflowNodes.length})`;
    }
  } else {
    activeIndicator.hidden = true;
  }
}

function addToPipeline(mode) {
  if (state.workflowNodes.length >= 3) {
    showToast('Maximum 3 nodes per workflow');
    return;
  }
  if (state.workflowNodes.length > 0 && state.workflowNodes[state.workflowNodes.length - 1].mode === mode) {
    showToast('Cannot add consecutive identical modes');
    return;
  }
  state.workflowNodes.push({ mode, label: '', completed: false });
  renderWorkflow();
}

function removeFromPipeline(index) {
  state.workflowNodes.splice(index, 1);
  state.workflowActive = false;
  state.workflowCurrentNode = 0;
  renderWorkflow();
}

function moveNode(from, to) {
  const node = state.workflowNodes.splice(from, 1)[0];
  state.workflowNodes.splice(to, 0, node);
  state.workflowActive = false;
  state.workflowCurrentNode = 0;
  renderWorkflow();
}

function validateWorkflow() {
  if (state.workflowNodes.length > 3) {
    return { valid: false, reason: 'Maximum 3 nodes per workflow' };
  }
  for (let i = 0; i < state.workflowNodes.length - 1; i++) {
    if (state.workflowNodes[i].mode === state.workflowNodes[i + 1].mode) {
      return { valid: false, reason: `Cannot have consecutive identical modes at position ${i + 1}` };
    }
  }
  return { valid: true, reason: null };
}

async function saveWorkflow() {
  const v = validateWorkflow();
  if (!v.valid) {
    showToast(v.reason || 'Invalid workflow');
    return;
  }

  if (state.isRunning) {
    showToast('Cannot apply workflow while agent is running');
    return;
  }

  const payload = {
    nodes: state.workflowNodes.map((n) => ({ mode: n.mode, label: n.label })),
  };

  try {
    const res = await fetch('/api/workflow', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
    });
    const data = await res.json();
    if (!res.ok) {
      showToast(data.error || 'Failed to save workflow');
      return;
    }
    state.workflowActive = true;
    state.workflowCurrentNode = 0;
    renderWorkflow();
    showToast('Workflow applied');
  } catch (e) {
    showToast(`Workflow save failed: ${e.message}`);
  }
}

async function resetWorkflow() {
  try {
    await fetch('/api/workflow/reset', { method: 'POST' });
    state.workflowActive = false;
    state.workflowCurrentNode = 0;
    state.workflowNodes = [];
    renderWorkflow();
    showToast('Workflow cleared');
  } catch (e) {
    // Clear locally even if API fails
    state.workflowActive = false;
    state.workflowCurrentNode = 0;
    state.workflowNodes = [];
    renderWorkflow();
  }
}

async function setMode(mode) {
  if (state.isRunning) {
    showToast('Cannot change mode while agent is running');
    return;
  }

  try {
    const res = await fetch('/api/mode', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ mode }),
    });
    const data = await res.json();
    if (!res.ok) {
      showToast(data.error || 'Failed to set mode');
      return;
    }
    state.currentMode = mode;
    showToast(`Mode set to: ${mode}`);
  } catch (e) {
    showToast(`Mode change failed: ${e.message}`);
  }
}

async function loadWorkflowState() {
  try {
    const res = await fetch('/api/workflow');
    const data = await res.json();
    if (data.nodes) {
      state.workflowNodes = data.nodes.map((n) => ({
        mode: n.mode,
        label: n.label || '',
        completed: n.completed || false,
      }));
      state.workflowActive = data.active || false;
      state.workflowCurrentNode = data.current_node || 0;
    }
    // Also load current mode
    const modeRes = await fetch('/api/mode');
    const modeData = await modeRes.json();
    if (modeData.mode) {
      state.currentMode = modeData.mode;
    }
    renderWorkflow();
  } catch (e) {
    // silently fail on startup
  }
}

async function loadBootstrap() {
  hydrateWorkspacePickerToken();
  const storedTheme = window.localStorage.getItem('generic-coder-theme');
  if (THEME_OPTIONS.includes(storedTheme)) {
    setTheme(storedTheme, false);
  }
  const res = await fetch('/api/bootstrap');
  const data = await res.json();
  state.messages = data.messages || [];
  modelLabel.textContent = data.model || t('modelOffline');
  state.currentModelIndex = Number.isInteger(data.model_index) ? data.model_index : 0;
  if (data.llm_form || data.workspace || data.remote) {
    hydrateSettings(data);
  }
  // Load mode and workflow from bootstrap
  if (data.mode) {
    state.currentMode = data.mode;
  }
  if (data.workflow && data.workflow.nodes) {
    state.workflowNodes = (data.workflow.nodes || []).map((n) => ({
      mode: n.mode,
      label: n.label || '',
      completed: n.completed || false,
    }));
    state.workflowActive = data.workflow.active || false;
    state.workflowCurrentNode = data.workflow.current_node || 0;
    renderWorkflow();
  }
  const resolvedTheme = THEME_OPTIONS.includes(storedTheme) ? storedTheme : (data.theme || 'solarflare');
  setTheme(resolvedTheme, false);
  setRunning(Boolean(data.is_running));
  renderMessages();
  loadWorkspaceTree();
  refreshSessionTabs();
  updateContextPreview();
  loadWorkflowState();
  await loadModels();
  if (resolvedTheme !== data.theme) {
    await fetch('/api/theme', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ theme: resolvedTheme }),
    });
  }
}

async function loadModels() {
  const res = await fetch('/api/models');
  const data = await res.json();
  state.models = data.models || [];
  state.currentModelIndex = Number.isInteger(data.current_index) ? data.current_index : state.currentModelIndex;
  renderModelOptions();
}

async function loadSettings() {
  const res = await fetch('/api/settings');
  const data = await res.json();
  hydrateSettings(data);
}

async function updateTheme(event) {
  const theme = event.target.value;
  await fetch('/api/theme', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ theme }),
  });
  setTheme(theme);
  renderSettingsState();
}

async function updateModel(event) {
  const index = Number(event.target.value);
  const res = await fetch('/api/model', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ index }),
  });
  const data = await res.json();
  if (!res.ok) {
    showToast(data.error || t('modelSwitchError'));
    await loadModels();
    return;
  }
  state.currentModelIndex = data.current_index;
  modelLabel.textContent = data.model || t('modelOffline');
  renderModelOptions();
  showToast(`${t('modelSwitchOk')}: ${data.model}`);
}

function updateLanguage(event) {
  applyLocale(event.target.value);
}

async function saveLlmConfig() {
  const protocolPreset = protocolPresetSelect.value || 'custom';
  const preset = PROTOCOL_PRESETS[protocolPreset] || PROTOCOL_PRESETS.custom;
  const apiMode = protocolPreset === 'custom'
    ? (state.llmForm.api_mode || 'chat_completions')
    : (preset.api_mode || 'chat_completions');
  const payload = {
    entry_key: state.llmForm.entry_key || '',
    session_type: sessionTypeSelect.value,
    protocol_preset: protocolPreset,
    api_mode: apiMode,
    provider: providerInput.value.trim(),
    name: displayNameInput.value.trim(),
    model: modelNameInput.value.trim(),
    apibase: baseUrlInput.value.trim(),
    apikey: apiKeyInput.value.trim(),
  };
  const res = await fetch('/api/llm-config', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(payload),
  });
  const data = await res.json();
  if (!res.ok) {
    showToast(data.error || t('llmSaveError'));
    return;
  }
  modelLabel.textContent = data.model || t('modelOffline');
  await loadSettings();
  showToast(t('llmSaveOk'));
}

async function saveWorkspace() {
  const payload = {
    name: workspaceNameInput.value.trim(),
    path: workspacePathInput.value.trim(),
  };
  const res = await fetch('/api/workspace', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(payload),
  });
  const data = await res.json();
  if (!res.ok) {
    showToast(data.error || t('workspaceSaveError'));
    return;
  }
  state.workspace = {
    active: data.active || null,
    workspaces: data.workspaces || [],
    recent_folders: data.recent_folders || [],
  };
  renderSettingsState();
  loadWorkspaceTree();
  showToast(t('workspaceSaveOk'));
}

async function browseWorkspacePath() {
  try {
    if (!state.workspacePickerToken) {
      showToast(t('workspacePickError'));
      return;
    }
    const previousPath = workspacePathInput.value.trim();
    const res = await fetch('/api/workspace/pick', {
      method: 'POST',
      headers: {
        'X-Generic-Coder-UI': '1',
        'X-Generic-Coder-Picker-Token': state.workspacePickerToken,
      },
    });
    const data = await res.json();
    if (!res.ok) {
      showToast(data.error || t('workspacePickError'));
      return;
    }
    if (!data.path) {
      return;
    }
    workspacePathInput.value = data.path;
    if (shouldAutofillWorkspaceName(previousPath)) {
      workspaceNameInput.value = inferWorkspaceNameFromPath(data.path);
      state.workspaceNameDirty = false;
      state.workspaceNameAutoFilled = true;
    }
  } catch (error) {
    showToast(`${t('workspacePickError')}: ${error.message}`);
  }
}

async function connectRemote() {
  const payload = {
    enabled: remoteEnabledInput.checked,
    server_name: remoteNameInput.value.trim(),
    host: remoteHostInput.value.trim(),
    port: Number(remotePortInput.value || 22),
    username: remoteUsernameInput.value.trim() || 'root',
    password: remotePasswordInput.value,
    key_path: remoteKeyPathInput.value.trim(),
    cwd: remoteCwdInput.value.trim(),
  };
  const res = await fetch('/api/remote/connect', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(payload),
  });
  const data = await res.json();
  if (!res.ok) {
    showToast(data.error || t('remoteConnectError'));
    return;
  }
  state.remote = mergeRemoteState(data);
  renderSettingsState();
  showToast(data.message || t('remoteConnectOk'));
}

function addStreamingPlaceholder() {
  const placeholder = { role: 'assistant', content: '...', streaming: true };
  state.messages.push(placeholder);
  state.taskPlaceholderId = state.messages.length - 1;
  renderMessages();
}

async function pollTask(taskId) {
  try {
    while (true) {
      const res = await fetch(`/api/tasks/${taskId}`);
      const data = await res.json();
      if (typeof state.taskPlaceholderId === 'number') {
        state.messages[state.taskPlaceholderId] = {
          role: 'assistant',
          content: data.preview || '...',
          streaming: !data.done,
        };
        renderMessages();
      }
      if (data.done) {
        if (typeof state.taskPlaceholderId === 'number') {
          state.messages[state.taskPlaceholderId] = {
            role: 'assistant',
            content: data.final || data.preview || '...',
            streaming: false,
          };
        }
        state.pendingTaskId = null;
        state.taskPlaceholderId = null;
        setRunning(false);
        renderMessages();
        // Check for file changes, plan status, and tabs after task completes
        setTimeout(loadChanges, 300);
        setTimeout(pollPlanStatus, 500);
        setTimeout(refreshSessionTabs, 700);
        setTimeout(updateContextPreview, 900);
        return;
      }
      await new Promise((resolve) => window.setTimeout(resolve, 700));
    }
  } catch (error) {
    setRunning(false);
    showToast(`${t('pollingError')}: ${error.message}`);
  }
}

async function sendPrompt(prompt) {
  const trimmed = prompt.trim();
  if (!trimmed || state.isRunning) return;

  state.messages.push({ role: 'user', content: trimmed, streaming: false });
  renderMessages();
  composerEl.value = '';
  setRunning(true);

  const res = await fetch('/api/chat', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ prompt: trimmed }),
  });
  const data = await res.json();

  if (data.handled) {
    state.messages = data.messages || state.messages;
    setRunning(false);
    renderMessages();
    if (data.notice) showToast(data.notice);
    return;
  }

  if (!data.task_id) {
    setRunning(false);
    showToast(data.error || t('requestError'));
    return;
  }

  state.pendingTaskId = data.task_id;
  addStreamingPlaceholder();
  pollTask(data.task_id);
}

async function loadSessions() {
  const res = await fetch('/api/sessions');
  const data = await res.json();
  const items = data.sessions || [];
  if (!items.length) {
    sessionsList.innerHTML = `<div class="session-item"><div class="session-item__preview">${t('noSessions')}</div></div>`;
    return;
  }

  sessionsList.innerHTML = items.map((item) => `
    <article class="session-item">
      <div>
        <div class="session-item__meta">
          <span class="session-chip">#${item.index}</span>
          <span class="session-chip">${t('rounds')(item.rounds)}</span>
          <span class="session-chip">${escapeHtml(item.relative_time)}</span>
        </div>
        <div class="session-item__preview">${escapeHtml(item.preview || '无法预览')}</div>
      </div>
      <button class="session-item__action" data-session-index="${item.index}">${t('sessions')}</button>
    </article>
  `).join('');
}

async function stopTask() {
  await fetch('/api/stop', { method: 'POST' });
  showToast(t('stopSent'));
}

async function loadChanges() {
  try {
    const res = await fetch('/api/changes');
    const data = await res.json();
    const changes = data.changes || [];
    const panel = document.getElementById('changes-panel');
    const list = document.getElementById('changes-list');

    if (!changes.length) {
      panel.hidden = true;
      return;
    }

    panel.hidden = false;
    list.innerHTML = changes.map((c) => `
      <div class="change-item">
        <span class="change-item__path" title="${escapeHtml(c.path)}">${escapeHtml(c.basename)}</span>
        <span class="change-item__time">${escapeHtml(c.backup_time)}</span>
        <button class="change-item__btn" data-diff-path="${escapeHtml(c.path)}">Diff</button>
        <button class="change-item__btn change-item__btn--revert" data-revert-path="${escapeHtml(c.path)}">Revert</button>
      </div>
    `).join('');

    // Wire up diff buttons
    list.querySelectorAll('[data-diff-path]').forEach((btn) => {
      btn.addEventListener('click', () => showDiff(btn.dataset.diffPath));
    });

    // Wire up revert buttons
    list.querySelectorAll('[data-revert-path]').forEach((btn) => {
      btn.addEventListener('click', () => revertFile(btn.dataset.revertPath));
    });
  } catch (error) {
    // silently fail for changes polling
  }
}

async function showDiff(filePath) {
  const diffView = document.getElementById('diff-view');
  diffView.hidden = false;
  diffView.innerHTML = '<div class="diff-line diff-line--info">Loading diff...</div>';

  try {
    const res = await fetch('/api/diff', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ path: filePath }),
    });
    const data = await res.json();
    if (data.error) {
      diffView.innerHTML = `<div class="diff-line diff-line--remove">Error: ${escapeHtml(data.error)}</div>`;
      return;
    }
    if (!data.has_changes) {
      diffView.innerHTML = `<div class="diff-line diff-line--info">No changes (backup and current file are identical).</div>`;
      return;
    }

    const lines = (data.diff || '').split('\n');
    diffView.innerHTML = `
      <div class="diff-view__header">
        <span style="color:var(--text-0);font-weight:700;font-size:12px">${escapeHtml(filePath)}</span>
        <button class="changes-panel__btn" id="close-diff-button">Close</button>
      </div>
    ` + lines.map((line) => {
      let cls = 'diff-line';
      if (line.startsWith('+++') || line.startsWith('---')) cls += ' diff-line--header';
      else if (line.startsWith('+')) cls += ' diff-line--add';
      else if (line.startsWith('-')) cls += ' diff-line--remove';
      else if (line.startsWith('@@')) cls += ' diff-line--info';
      return `<div class="${cls}">${escapeHtml(line)}</div>`;
    }).join('');

    document.getElementById('close-diff-button').addEventListener('click', () => {
      diffView.hidden = true;
    });

    diffView.scrollTop = 0;
  } catch (error) {
    diffView.innerHTML = `<div class="diff-line diff-line--remove">Failed to load diff: ${escapeHtml(error.message)}</div>`;
  }
}

async function revertFile(filePath) {
  if (!confirm(`Revert ${filePath} to its previous version?`)) return;

  try {
    const res = await fetch('/api/revert', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ path: filePath }),
    });
    const data = await res.json();
    if (res.ok) {
      showToast(`Reverted: ${filePath}`);
      // Hide diff view and refresh
      document.getElementById('diff-view').hidden = true;
      loadChanges();
    } else {
      showToast(`Revert failed: ${data.error || data.msg}`);
    }
  } catch (error) {
    showToast(`Revert error: ${error.message}`);
  }
}

// ── Command palette ──────────────────────────────────────
const COMMANDS = [
  { id: 'new', label: 'New Chat', desc: 'Start fresh conversation', shortcut: '⌘⇧N', action: () => sendPrompt('/new') },
  { id: 'sessions', label: 'Recover Session', desc: 'Continue previous conversation', shortcut: '', action: async () => { await loadSessions(); sessionsDialog.showModal(); } },
  { id: 'plan', label: 'Plan Mode', desc: 'Enter plan-driven development', shortcut: '', action: () => { composerEl.value = '/plan '; composerEl.focus(); } },
  { id: 'stop', label: 'Stop Generation', desc: 'Abort current task', shortcut: '⌘⇧S', action: stopTask },
  { id: 'settings', label: 'Settings', desc: 'Model, workspace, remote config', shortcut: '', action: async () => { await loadSettings(); settingsDialog.showModal(); } },
  { id: 'export', label: 'Export Conversation', desc: 'Download as markdown', shortcut: '', action: exportConversation },
  { id: 'changes', label: 'Show File Changes', desc: 'View diff of agent edits', shortcut: '', action: () => { document.getElementById('changes-panel').hidden = false; loadChanges(); } },
  { id: 'tree', label: 'Refresh Workspace Tree', desc: 'Reload file tree', shortcut: '', action: loadWorkspaceTree },
  { id: 'composer', label: 'Focus Composer', desc: 'Jump to input', shortcut: '⌘K', action: () => composerEl.focus() },
];

function openCommandPalette() {
  const palette = document.getElementById('command-palette');
  const search = document.getElementById('command-search');
  const list = document.getElementById('command-list');
  let selected = 0;

  function render(filter = '') {
    const q = filter.toLowerCase();
    const items = COMMANDS.filter((c) =>
      !q || c.label.toLowerCase().includes(q) || c.desc.toLowerCase().includes(q) || c.id.includes(q)
    );
    selected = Math.min(selected, items.length - 1);
    list.innerHTML = items.length
      ? items.map((c, i) => `
        <div class="command-item${i === selected ? ' is-selected' : ''}" data-id="${c.id}">
          ${c.shortcut ? `<span class="command-item__shortcut">${c.shortcut}</span>` : '<span class="command-item__shortcut"></span>'}
          <span class="command-item__label">${c.label}</span>
          <span class="command-item__desc">${c.desc}</span>
        </div>
      `).join('')
      : '<div class="command-empty">No matching commands</div>';

    list.querySelectorAll('.command-item').forEach((el, i) => {
      el.addEventListener('click', () => {
        const cmd = COMMANDS.find((c) => c.id === el.dataset.id);
        if (cmd) { palette.close(); cmd.action(); }
      });
    });
  }

  render();
  search.value = '';
  search.focus();
  palette.showModal();

  const closeHandler = () => {
    palette.removeEventListener('close', closeHandler);
    search.removeEventListener('input', inputHandler);
  };

  const inputHandler = () => {
    selected = 0;
    render(search.value);
  };

  search.addEventListener('input', inputHandler);

  search.addEventListener('keydown', (e) => {
    const items = list.querySelectorAll('.command-item');
    if (e.key === 'Escape') { palette.close(); return; }
    if (e.key === 'ArrowDown') { e.preventDefault(); selected = Math.min(selected + 1, items.length - 1); render(search.value); return; }
    if (e.key === 'ArrowUp') { e.preventDefault(); selected = Math.max(selected - 1, 0); render(search.value); return; }
    if (e.key === 'Enter') {
      e.preventDefault();
      const cmd = COMMANDS.find((c) => c.id === items[selected]?.dataset?.id);
      if (cmd) { palette.close(); cmd.action(); }
    }
  });

  palette.addEventListener('close', closeHandler);
}

// ── Keyboard shortcuts ──────────────────────────────────
document.addEventListener('keydown', (event) => {
  const mod = event.metaKey || event.ctrlKey;
  const key = event.key.toLowerCase();

  // Cmd/Ctrl+Enter → send
  if (mod && key === 'enter' && document.activeElement === composerEl) {
    event.preventDefault();
    sendPrompt(composerEl.value);
    return;
  }

  // Cmd/Ctrl+Shift+N → new chat
  if (mod && event.shiftKey && key === 'n') {
    event.preventDefault();
    sendPrompt('/new');
    return;
  }

  // Cmd/Ctrl+Shift+S → stop
  if (mod && event.shiftKey && key === 's') {
    event.preventDefault();
    stopTask();
    return;
  }

  // Escape → close dialogs
  if (key === 'escape') {
    if (sessionsDialog.open) { sessionsDialog.close(); return; }
    if (settingsDialog.open) { settingsDialog.close(); return; }
    // Close diff view if open
    const diffView = document.getElementById('diff-view');
    if (diffView && !diffView.hidden) { diffView.hidden = true; return; }
  }

  // Cmd/Ctrl+K → command palette (or focus composer if already in it)
  if (mod && key === 'k') {
    if (document.activeElement === composerEl) {
      return; // let default behavior (or use for @mention)
    }
    event.preventDefault();
    openCommandPalette();
    return;
  }
});

// ── Agent context preview ────────────────────────────────
function updateContextPreview() {
  const modeEl = document.getElementById('ctx-mode');
  const targetEl = document.getElementById('ctx-target');
  const modelEl = document.getElementById('ctx-model');
  const turnsEl = document.getElementById('ctx-turns');

  const remote = state.remote;
  const ws = state.workspace;

  if (remote.connected && remote.form?.server_name) {
    modeEl.textContent = 'Remote SSH';
    targetEl.textContent = remote.form.server_name;
  } else if (ws.active?.path) {
    modeEl.textContent = 'Local Workspace';
    targetEl.textContent = ws.active.name || ws.active.path.split('/').pop();
  } else {
    modeEl.textContent = 'Local (temp)';
    targetEl.textContent = 'None';
  }

  // Show current agent mode and workflow status
  const modeLabels = { work: 'Work', plan: 'Plan', review: 'Review' };

  modelEl.textContent = modelLabel.textContent || 'Not configured';
  if (state.workflowActive && state.workflowNodes.length > 0) {
    turnsEl.textContent = `Pipeline: ${state.workflowNodes.map((n) => modeLabels[n.mode] || n.mode).join(' → ')}`;
  } else {
    const mLabel = modeLabels[state.currentMode] || 'Work';
    turnsEl.textContent = state.isRunning ? 'Running...' : `${mLabel} · 70 max`;
  }
}

// Update context after bootstrap and settings changes
const _origHydrate = hydrateSettings;
hydrateSettings = function(data) {
  _origHydrate(data);
  setTimeout(updateContextPreview, 200);
};

// ── Workspace file tree ──────────────────────────────────
async function loadWorkspaceTree() {
  const container = document.getElementById('workspace-tree');
  try {
    const res = await fetch('/api/workspace/tree');
    const data = await res.json();
    if (data.error || !data.tree?.length) {
      container.innerHTML = `<div class="tree-empty">${escapeHtml(data.error || 'No workspace open')}</div>`;
      return;
    }

    const entries = data.tree;
    // Deduplicate: show only unique paths
    const seen = new Set();
    const rows = entries.filter((e) => {
      if (seen.has(e.path)) return false;
      seen.add(e.path);
      return true;
    });

    container.innerHTML = rows.map((e) => {
      const indent = '&nbsp;&nbsp;'.repeat(Math.max(0, e.depth));
      const icon = e.type === 'dir' ? '▸' : '';
      const cls = e.type === 'dir' ? 'tree-entry tree-entry--dir' : 'tree-entry tree-entry--file';
      return `<div class="${cls}" data-path="${escapeHtml(e.path)}" data-type="${e.type}" title="${escapeHtml(e.path)}">
        <span class="tree-icon">${icon}</span>
        <span class="tree-name">${indent}${escapeHtml(e.name)}</span>
      </div>`;
    }).join('');

    // Click handler: clicking a file copies path to composer
    container.querySelectorAll('.tree-entry--file').forEach((el) => {
      el.addEventListener('click', () => {
        const path = el.dataset.path;
        if (path) {
          composerEl.value = `Read and analyze: ${path}`;
          composerEl.focus();
        }
      });
    });
  } catch (error) {
    container.innerHTML = '<div class="tree-empty">Failed to load tree</div>';
  }
}

// Load tree on bootstrap and after workspace changes
document.getElementById('refresh-tree-button').addEventListener('click', loadWorkspaceTree);

// ── Plan mode ────────────────────────────────────────────
async function pollPlanStatus() {
  try {
    const res = await fetch('/api/plan/status');
    const data = await res.json();
    const progress = document.getElementById('plan-progress');
    if (data.in_plan) {
      progress.hidden = false;
      const remaining = Math.max(0, data.remaining);
      const pct = remaining >= 0 ? Math.max(5, Math.round((1 - remaining / Math.max(remaining + 3, 10)) * 100)) : 50;
      document.getElementById('plan-fill').style.width = `${pct}%`;
      document.getElementById('plan-text').textContent = remaining === 0 ? 'Plan: Complete' : `Plan: ${remaining} tasks left`;
    } else {
      progress.hidden = true;
    }
  } catch (e) {
    // silently fail
  }
}

document.getElementById('plan-button').addEventListener('click', () => {
  composerEl.value = '/plan ';
  composerEl.focus();
});

// ── Image paste / drop ───────────────────────────────────
async function uploadImage(dataUrl) {
  try {
    const res = await fetch('/api/upload', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ data: dataUrl }),
    });
    const data = await res.json();
    if (data.path) {
      return data.path;
    }
    throw new Error(data.error || 'Upload failed');
  } catch (e) {
    showToast(`Image upload failed: ${e.message}`);
    return null;
  }
}

async function handleImagePaste(file) {
  return new Promise((resolve) => {
    const reader = new FileReader();
    reader.onload = () => resolve(reader.result);
    reader.onerror = () => resolve(null);
    reader.readAsDataURL(file);
  });
}

composerEl.addEventListener('paste', async (event) => {
  const items = event.clipboardData?.items;
  if (!items) return;

  for (const item of items) {
    if (item.type.startsWith('image/')) {
      event.preventDefault();
      const file = item.getAsFile();
      if (!file) continue;

      // Show upload indicator
      const origPlaceholder = composerEl.placeholder;
      composerEl.placeholder = 'Uploading image...';
      composerEl.style.opacity = '0.6';

      const dataUrl = await handleImagePaste(file);
      if (dataUrl) {
        const path = await uploadImage(dataUrl);
        if (path) {
          const before = composerEl.value;
          composerEl.value = before + (before ? '\n' : '') + `@image:${path}`;
        }
      }

      composerEl.placeholder = origPlaceholder;
      composerEl.style.opacity = '';
      composerEl.focus();
    }
  }
});

composerEl.addEventListener('dragover', (event) => {
  event.preventDefault();
  event.dataTransfer.dropEffect = 'copy';
});

composerEl.addEventListener('drop', async (event) => {
  const files = event.dataTransfer?.files;
  if (!files) return;
  event.preventDefault();

  for (const file of files) {
    if (file.type.startsWith('image/')) {
      const dataUrl = await handleImagePaste(file);
      if (dataUrl) {
        const path = await uploadImage(dataUrl);
        if (path) {
          const before = composerEl.value;
          composerEl.value = before + (before ? '\n' : '') + `@image:${path}`;
        }
      }
    }
  }
});

// ── Session tabs ─────────────────────────────────────────
async function refreshSessionTabs() {
  const tabsEl = document.getElementById('session-tabs');
  try {
    const res = await fetch('/api/sessions');
    const data = await res.json();
    const sessions = data.sessions || [];

    tabsEl.innerHTML = `
      <span class="session-tab is-active" data-session="current">Chat</span>
    ` + sessions.slice(0, 8).map((s, i) => `
      <span class="session-tab" data-session="${s.index}" title="${escapeHtml(s.preview || '')}">
        #${s.index}
        <span class="session-tab__close" data-close="${s.index}">×</span>
      </span>
    `).join('');

    // Click handler for switching
    tabsEl.querySelectorAll('.session-tab').forEach((tab) => {
      tab.addEventListener('click', async (e) => {
        if (e.target.dataset.close) return; // handled by close handler
        const sessionIdx = tab.dataset.session;
        if (sessionIdx === 'current') return;
        if (state.isRunning) {
          showToast('Stop the current task first');
          return;
        }
        await sendPrompt(`/continue ${sessionIdx}`);
      });
    });

    // Close button handler
    tabsEl.querySelectorAll('.session-tab__close').forEach((btn) => {
      btn.addEventListener('click', (e) => {
        e.stopPropagation();
        btn.parentElement.remove();
      });
    });
  } catch (e) {
    // silently fail
  }
}

// Refresh tabs on load and after task completion
// (Added to pollTask completion via setTimeout)

// ── Conversation export ──────────────────────────────────
function exportConversation() {
  const lines = [];
  lines.push(`# Generic Coder Conversation`);
  lines.push(`\nExported: ${new Date().toISOString()}\n`);
  state.messages.forEach((msg) => {
    const role = msg.role === 'user' ? 'You' : 'Generic Coder';
    lines.push(`### ${role}\n`);
    // Strip thinking blocks for cleaner export
    const content = (msg.content || '').replace(/<thinking>[\s\S]*?<\/thinking>/g, '[thinking omitted]');
    lines.push(content + '\n');
  });
  const blob = new Blob([lines.join('\n')], { type: 'text/markdown' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = `generic-coder-export-${new Date().toISOString().slice(0, 10)}.md`;
  a.click();
  URL.revokeObjectURL(url);
  showToast('Conversation exported as markdown');
}

document.getElementById('session-button').addEventListener('click', async (event) => {
  // Shift+click on sessions button = export
  if (event.shiftKey) {
    event.preventDefault();
    exportConversation();
    return;
  }
  await loadSessions();
  sessionsDialog.showModal();
});

// ── Changes panel toggle ────────────────────────────────
document.getElementById('refresh-changes-button').addEventListener('click', loadChanges);
document.getElementById('toggle-changes-button').addEventListener('click', () => {
  const panel = document.getElementById('changes-panel');
  const list = document.getElementById('changes-list');
  const diffView = document.getElementById('diff-view');
  const toggleBtn = document.getElementById('toggle-changes-button');
  const isHidden = list.style.display === 'none';
  list.style.display = isHidden ? '' : 'none';
  diffView.hidden = isHidden ? diffView.hidden : true;
  toggleBtn.textContent = isHidden ? 'Hide' : 'Show';
});

composerForm.addEventListener('submit', async (event) => {
  event.preventDefault();
  await sendPrompt(composerEl.value);
});

// ── @‑mention file auto-complete ────────────────────────
let mentionState = { active: false, query: '', items: [], selected: -1, startPos: 0 };
const mentionDropdown = document.createElement('div');
mentionDropdown.className = 'mention-dropdown';
composerEl.parentNode.appendChild(mentionDropdown);

async function fetchMentionSuggestions(query) {
  try {
    const res = await fetch(`/api/workspace/files?q=${encodeURIComponent(query)}&limit=10`);
    const data = await res.json();
    return data.files || [];
  } catch (e) {
    return [];
  }
}

function renderMentionDropdown() {
  if (!mentionState.active || !mentionState.items.length) {
    mentionDropdown.classList.remove('is-active');
    return;
  }
  mentionDropdown.classList.add('is-active');
  mentionDropdown.innerHTML = mentionState.items.map((f, idx) => {
    const icon = '📄';
    const selClass = idx === mentionState.selected ? ' is-selected' : '';
    return `<div class="mention-item${selClass}" data-idx="${idx}">
      <span class="mention-item__icon">${icon}</span>
      <span class="mention-item__name">${escapeHtml(f.name)}</span>
      <span class="mention-item__path">${escapeHtml(f.rel)}</span>
    </div>`;
  });

  // Click handlers
  mentionDropdown.querySelectorAll('.mention-item').forEach((el) => {
    el.addEventListener('mousedown', (e) => {
      e.preventDefault();
      const idx = parseInt(el.dataset.idx);
      insertMention(mentionState.items[idx]);
    });
  });
}

function insertMention(file) {
  const before = composerEl.value.slice(0, mentionState.startPos);
  const after = composerEl.value.slice(composerEl.selectionStart);
  const mention = file.rel || file.name;
  composerEl.value = before + mention + ' ' + after;
  composerEl.focus();
  const cursorPos = before.length + mention.length + 1;
  composerEl.setSelectionRange(cursorPos, cursorPos);
  mentionState.active = false;
  mentionDropdown.classList.remove('is-active');
}

function detectMentionTrigger() {
  const pos = composerEl.selectionStart;
  const text = composerEl.value.slice(0, pos);
  const atIdx = text.lastIndexOf('@');
  if (atIdx === -1 || (atIdx > 0 && text[atIdx - 1] !== ' ' && text[atIdx - 1] !== '\n')) {
    mentionState.active = false;
    mentionDropdown.classList.remove('is-active');
    return;
  }
  mentionState.active = true;
  mentionState.startPos = atIdx;
  mentionState.query = text.slice(atIdx + 1);
  mentionState.selected = 0;
  fetchMentionSuggestions(mentionState.query).then((items) => {
    mentionState.items = items;
    renderMentionDropdown();
  });
}

composerEl.addEventListener('input', detectMentionTrigger);

// Merged keydown: @mention navigation + Enter to send
composerEl.addEventListener('keydown', async (event) => {
  // Handle @mention keyboard navigation
  if (mentionState.active && mentionDropdown.classList.contains('is-active')) {
    if (event.key === 'Escape') {
      mentionState.active = false;
      mentionDropdown.classList.remove('is-active');
      event.preventDefault();
      return;
    }
    if (event.key === 'ArrowDown') {
      mentionState.selected = Math.min(mentionState.selected + 1, mentionState.items.length - 1);
      renderMentionDropdown();
      event.preventDefault();
      return;
    }
    if (event.key === 'ArrowUp') {
      mentionState.selected = Math.max(mentionState.selected - 1, 0);
      renderMentionDropdown();
      event.preventDefault();
      return;
    }
    if (event.key === 'Enter' || event.key === 'Tab') {
      if (mentionState.items[mentionState.selected]) {
        insertMention(mentionState.items[mentionState.selected]);
        event.preventDefault();
        return;
      }
    }
  }

  // Enter to send (without shift)
  if (event.key === 'Enter' && !event.shiftKey && !(mentionState.active && mentionDropdown.classList.contains('is-active'))) {
    event.preventDefault();
    await sendPrompt(composerEl.value);
  }
});

themeSelect.addEventListener('change', updateTheme);
languageSelect.addEventListener('change', updateLanguage);
modelSelect.addEventListener('change', updateModel);
protocolPresetSelect.addEventListener('change', (event) => applyProtocolPreset(event.target.value));
modelPresetSelect.addEventListener('change', (event) => applyModelPreset(event.target.value));
sessionTypeSelect.addEventListener('change', () => {
  markManualProtocolEntry();
});
providerInput.addEventListener('input', () => {
  markManualProtocolEntry();
  markManualModelEntry();
});
displayNameInput.addEventListener('input', markManualModelEntry);
modelNameInput.addEventListener('input', markManualModelEntry);
baseUrlInput.addEventListener('input', markManualProtocolEntry);
apiKeyInput.addEventListener('input', () => {
  markManualProtocolEntry();
  markManualModelEntry();
});
workspaceNameInput.addEventListener('input', () => {
  state.workspaceNameDirty = true;
  state.workspaceNameAutoFilled = false;
});
apiKeyToggle.addEventListener('click', () => {
  apiKeyInput.type = apiKeyInput.type === 'password' ? 'text' : 'password';
  renderApiKeyToggle();
});
saveLlmButton.addEventListener('click', saveLlmConfig);
saveWorkspaceButton.addEventListener('click', saveWorkspace);
browseWorkspacePathButton.addEventListener('click', browseWorkspacePath);
connectRemoteButton.addEventListener('click', connectRemote);
installSkillButton.addEventListener('click', installSkill);

// ── Workflow event listeners ─────────────────────────────
document.querySelectorAll('.workflow-mode-btn').forEach((btn) => {
  btn.addEventListener('click', () => addToPipeline(btn.dataset.mode));
});
document.getElementById('workflow-save-button').addEventListener('click', saveWorkflow);
document.getElementById('workflow-reset-button').addEventListener('click', resetWorkflow);

document.getElementById('new-chat-button').addEventListener('click', async () => sendPrompt('/new'));
document.getElementById('session-button').addEventListener('click', async () => {
  await loadSessions();
  sessionsDialog.showModal();
});
document.getElementById('stop-button').addEventListener('click', stopTask);
document.getElementById('settings-button').addEventListener('click', async () => {
  await loadSettings();
  settingsDialog.showModal();
});
document.getElementById('close-sessions').addEventListener('click', () => sessionsDialog.close());
document.getElementById('close-settings').addEventListener('click', () => settingsDialog.close());

workspaceList.addEventListener('click', async (event) => {
  const button = event.target.closest('[data-workspace-path]');
  if (!button) return;
  workspacePathInput.value = button.dataset.workspacePath || '';
  workspaceNameInput.value = button.dataset.workspaceName || '';
  state.workspaceNameDirty = false;
  state.workspaceNameAutoFilled = false;
  await saveWorkspace();
});

remoteConfigList.addEventListener('click', (event) => {
  const button = event.target.closest('[data-remote-name]');
  if (!button) return;
  remoteNameInput.value = button.dataset.remoteName || '';
  remoteHostInput.value = button.dataset.remoteHost || '';
  remotePortInput.value = button.dataset.remotePort || '22';
  remoteUsernameInput.value = button.dataset.remoteUsername || 'root';
});

sessionsList.addEventListener('click', async (event) => {
  const button = event.target.closest('[data-session-index]');
  if (!button) return;
  sessionsDialog.close();
  await sendPrompt(`/continue ${button.dataset.sessionIndex}`);
});

window.addEventListener('click', (event) => {
  if (event.target === sessionsDialog) {
    sessionsDialog.close();
  }
  if (event.target === settingsDialog) {
    settingsDialog.close();
  }
});

const initialStoredTheme = window.localStorage.getItem('generic-coder-theme');
if (THEME_OPTIONS.includes(initialStoredTheme)) {
  state.theme = initialStoredTheme;
  document.body.dataset.theme = initialStoredTheme;
}

applyLocale(preferredLocale(), false);
loadBootstrap().catch((error) => {
  showToast(`启动失败: ${error.message}`);
});
