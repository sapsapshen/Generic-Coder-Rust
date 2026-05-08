use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ProviderProfile {
    pub id: &'static str,
    pub label: &'static str,
    pub provider: &'static str,
    pub description: &'static str,
    pub session_type: &'static str,
    pub api_mode: &'static str,
    pub apibase: &'static str,
    pub model: &'static str,
    pub reasoning_effort: Option<&'static str>,
}

pub fn built_in_provider_profiles() -> Vec<ProviderProfile> {
    vec![
        ProviderProfile {
            id: "deepseek-global-flash",
            label: "DeepSeek Global · Flash",
            provider: "DeepSeek",
            description: "Official global DeepSeek endpoint on beta chat completions, tuned for fast daily coding turns.",
            session_type: "native_oai",
            api_mode: "chat_completions",
            apibase: "https://api.deepseek.com/beta",
            model: "deepseek-v4-flash",
            reasoning_effort: Some("low"),
        },
        ProviderProfile {
            id: "deepseek-global-pro",
            label: "DeepSeek Global · Pro",
            provider: "DeepSeek",
            description: "Official global DeepSeek endpoint with the stronger Pro model for harder debugging and architecture work.",
            session_type: "native_oai",
            api_mode: "chat_completions",
            apibase: "https://api.deepseek.com/beta",
            model: "deepseek-v4-pro",
            reasoning_effort: Some("high"),
        },
        ProviderProfile {
            id: "deepseek-china-flash",
            label: "DeepSeek China · Flash",
            provider: "DeepSeek CN",
            description: "China mainland friendly DeepSeek endpoint for lower-latency Flash usage.",
            session_type: "native_oai",
            api_mode: "chat_completions",
            apibase: "https://api.deepseeki.com/beta",
            model: "deepseek-v4-flash",
            reasoning_effort: Some("low"),
        },
        ProviderProfile {
            id: "deepseek-china-pro",
            label: "DeepSeek China · Pro",
            provider: "DeepSeek CN",
            description: "China mainland friendly DeepSeek endpoint with Pro for heavier engineering work.",
            session_type: "native_oai",
            api_mode: "chat_completions",
            apibase: "https://api.deepseeki.com/beta",
            model: "deepseek-v4-pro",
            reasoning_effort: Some("high"),
        },
        ProviderProfile {
            id: "nvidia-nim-deepseek-pro",
            label: "NVIDIA NIM · DeepSeek V4 Pro",
            provider: "NVIDIA NIM",
            description: "Managed DeepSeek V4 Pro through NVIDIA NIM's OpenAI-compatible endpoint.",
            session_type: "native_oai",
            api_mode: "chat_completions",
            apibase: "https://integrate.api.nvidia.com/v1",
            model: "deepseek-ai/deepseek-v4-pro",
            reasoning_effort: Some("high"),
        },
        ProviderProfile {
            id: "fireworks-deepseek-pro",
            label: "Fireworks · DeepSeek V4 Pro",
            provider: "Fireworks",
            description: "Fireworks-hosted DeepSeek V4 Pro, useful when you want an alternate managed provider.",
            session_type: "native_oai",
            api_mode: "chat_completions",
            apibase: "https://api.fireworks.ai/inference/v1",
            model: "accounts/fireworks/models/deepseek-v4-pro",
            reasoning_effort: Some("high"),
        },
        ProviderProfile {
            id: "sglang-deepseek-flash",
            label: "Self-hosted SGLang · Flash",
            provider: "SGLang",
            description: "Local or self-hosted SGLang endpoint using a DeepSeek V4 Flash-compatible model.",
            session_type: "native_oai",
            api_mode: "chat_completions",
            apibase: "http://localhost:30000/v1",
            model: "deepseek-ai/DeepSeek-V4-Flash",
            reasoning_effort: Some("low"),
        },
        ProviderProfile {
            id: "vllm-deepseek-pro",
            label: "Self-hosted vLLM · Pro",
            provider: "vLLM",
            description: "Local or self-hosted vLLM endpoint using a DeepSeek V4 Pro-compatible model.",
            session_type: "native_oai",
            api_mode: "chat_completions",
            apibase: "http://localhost:8000/v1",
            model: "deepseek-ai/DeepSeek-V4-Pro",
            reasoning_effort: Some("high"),
        },
        ProviderProfile {
            id: "ollama-deepseek-local",
            label: "Ollama · Local DeepSeek",
            provider: "Ollama",
            description: "Local Ollama profile for smaller DeepSeek-compatible models when you want fully offline usage.",
            session_type: "native_oai",
            api_mode: "chat_completions",
            apibase: "http://localhost:11434/v1",
            model: "deepseek-coder:1.3b",
            reasoning_effort: Some("low"),
        },
    ]
}

pub fn get_provider_profile(id: &str) -> Option<ProviderProfile> {
    built_in_provider_profiles()
        .into_iter()
        .find(|profile| profile.id.eq_ignore_ascii_case(id))
}
