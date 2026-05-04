//! Configuration loading for Generic Coder.
//!
//! Loads from mykey.json, ui_llm_config.json, tool schemas, system prompts,
//! and memory files.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use lazy_static::lazy_static;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::types::{LlmConfig, ToolSchema};

// ── Interned JSON types ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MixinConfig {
    pub llm_nos: Vec<String>,
    #[serde(default)]
    pub max_retries: usize,
    #[serde(default)]
    pub base_delay: f64,
    #[serde(default)]
    pub spring_back: Option<usize>,
}

#[derive(Debug, Clone, Default)]
pub struct Config {
    pub llm_configs: HashMap<String, LlmConfig>,
    pub mixin_configs: Vec<MixinConfig>,
    pub lang: String,
}

// ── Global singleton ──────────────────────────────────────────────────

lazy_static! {
    static ref CONFIG: RwLock<Option<Config>> = RwLock::new(None);
}

// ── Public API ────────────────────────────────────────────────────────

pub fn load_config(project_dir: impl AsRef<Path>) -> Config {
    let project_dir = project_dir.as_ref();
    let cfg = build_config(project_dir).unwrap_or_else(|e| {
        log::warn!("config load error: {e:#}, using defaults");
        Config::default()
    });
    *CONFIG.write() = Some(cfg.clone());
    cfg
}

pub fn get_config() -> Option<Config> {
    CONFIG.read().clone()
}

pub fn config_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".genericagent")
}

pub fn ui_config_path() -> PathBuf {
    config_dir().join("ui_llm_config.json")
}

pub fn load_ui_llm_configs() -> HashMap<String, LlmConfig> {
    let path = ui_config_path();
    if !path.exists() {
        return HashMap::new();
    }

    let Ok(data) = fs::read_to_string(path) else {
        return HashMap::new();
    };
    let Ok(raw) = serde_json::from_str::<HashMap<String, serde_json::Value>>(&data) else {
        return HashMap::new();
    };

    raw.into_iter()
        .filter_map(|(key, value)| {
            serde_json::from_value::<LlmConfig>(value)
                .ok()
                .map(normalize_llm_config)
                .map(|cfg| (key, cfg))
        })
        .collect()
}

fn normalize_llm_config(mut cfg: LlmConfig) -> LlmConfig {
    match cfg.model.trim().to_ascii_lowercase().as_str() {
        "deepseek-chat" => {
            cfg.model = "deepseek-v4-flash".to_string();
            if cfg.name.trim().is_empty() || cfg.name.trim().eq_ignore_ascii_case("deepseek-chat") {
                cfg.name = "deepseek-v4-flash".to_string();
            }
        }
        "deepseek-reasoner" => {
            cfg.model = "deepseek-v4-pro".to_string();
            if cfg.name.trim().is_empty()
                || cfg.name.trim().eq_ignore_ascii_case("deepseek-reasoner")
            {
                cfg.name = "deepseek-v4-pro".to_string();
            }
        }
        _ => {}
    }
    cfg
}

pub fn save_ui_llm_configs(configs: &HashMap<String, LlmConfig>) -> Result<()> {
    fs::create_dir_all(config_dir())?;
    let data = serde_json::to_string_pretty(configs)?;
    fs::write(ui_config_path(), data)?;
    Ok(())
}

pub fn save_ui_llm_config_entry(key: &str, config: &LlmConfig) -> Result<()> {
    let mut configs = load_ui_llm_configs();
    configs.insert(key.to_string(), config.clone());
    save_ui_llm_configs(&configs)
}

pub fn infer_session_type(hint: &str) -> &'static str {
    let lower = hint.to_lowercase();
    if lower.contains("native_claude") || (lower.contains("native") && lower.contains("claude")) {
        "native_claude"
    } else if lower.contains("native_oai") || (lower.contains("native") && lower.contains("oai")) {
        "native_oai"
    } else if lower.contains("claude") {
        "claude"
    } else {
        "oai"
    }
}

pub fn detect_lang() -> String {
    if let Ok(lang) = std::env::var("GA_LANG") {
        let l = lang.to_lowercase();
        if l.starts_with("zh") || l.contains("cn") {
            return "zh".into();
        }
        if l.starts_with("en") {
            return "en".into();
        }
    }
    let locale = std::env::var("LANG")
        .or_else(|_| std::env::var("LC_ALL"))
        .or_else(|_| std::env::var("LC_MESSAGES"))
        .unwrap_or_default();
    if locale.to_lowercase().starts_with("zh") {
        "zh".into()
    } else {
        "en".into()
    }
}

pub fn get_system_prompt(project_dir: impl AsRef<Path>) -> String {
    get_system_prompt_with_skills(project_dir, None)
}

pub fn get_system_prompt_with_skills(
    project_dir: impl AsRef<Path>,
    skills_summary: Option<&str>,
) -> String {
    let pd = project_dir.as_ref();
    let lang = detect_lang();
    let suffix = if lang == "en" { "_en" } else { "" };
    let path = pd.join("assets").join(format!("sys_prompt{}.txt", suffix));
    let mut prompt = fs::read_to_string(&path).unwrap_or_default();
    prompt.push_str(&format!(
        "\nToday: {}\n",
        chrono::Local::now().format("%Y-%m-%d %a")
    ));
    prompt.push_str(&get_global_memory(pd));
    if let Some(summary) = skills_summary {
        if !summary.is_empty() {
            prompt.push('\n');
            prompt.push_str(summary);
        }
    }
    prompt
}

pub fn get_global_memory(project_dir: impl AsRef<Path>) -> String {
    let pd = project_dir.as_ref();
    let lang = detect_lang();
    let suffix = if lang == "en" { "_en" } else { "" };
    let insight_path = pd.join("memory").join("global_mem_insight.txt");
    let structure_path = pd
        .join("assets")
        .join(format!("insight_fixed_structure{}.txt", suffix));
    let insight = fs::read_to_string(&insight_path).unwrap_or_default();
    let structure = fs::read_to_string(&structure_path).unwrap_or_default();
    format!(
        "cwd = {}/temp (./)\n\n[Memory] (../memory)\n{}{}",
        pd.display(),
        structure,
        insight
    )
}

pub fn load_tool_schema(
    project_dir: impl AsRef<Path>,
    lang_suffix: Option<&str>,
) -> Vec<ToolSchema> {
    let pd = project_dir.as_ref();
    let filename = match lang_suffix {
        Some("zh") | Some("cn") => "tools_schema_cn.json",
        Some("en") => "tools_schema.json",
        Some(_) => "tools_schema.json",
        None => {
            if detect_lang() == "zh" {
                "tools_schema_cn.json"
            } else {
                "tools_schema.json"
            }
        }
    };
    let path = pd.join("assets").join(filename);
    if !path.exists() {
        return vec![];
    }
    let data = fs::read_to_string(&path).unwrap_or_default();
    // In Windows, replace powershell with bash in schema for non-Windows
    #[cfg(not(windows))]
    let data = data.replace("powershell", "bash");
    serde_json::from_str(&data).unwrap_or_default()
}

// ── Internal building ──────────────────────────────────────────────────

fn build_config(project_dir: &Path) -> Result<Config> {
    let mut llm_configs = HashMap::new();
    let mut mixin_configs = Vec::new();

    // Try mykey.json in project dir
    let mykey_path = project_dir.join("mykey.json");
    if mykey_path.exists() {
        let data = fs::read_to_string(&mykey_path)?;
        if let Ok(map) = serde_json::from_str::<HashMap<String, serde_json::Value>>(&data) {
            for (key, val) in &map {
                if key.contains("api") || key.contains("config") || key.contains("cookie") {
                    if key.contains("mixin") {
                        if let Ok(mc) = serde_json::from_value::<MixinConfig>(val.clone()) {
                            mixin_configs.push(mc);
                        }
                    } else if let Ok(cfg) = serde_json::from_value::<LlmConfig>(val.clone()) {
                        llm_configs.insert(key.clone(), normalize_llm_config(cfg));
                    }
                }
            }
        }
    }

    // Merge UI config from ~/.genericagent/ui_llm_config.json
    let ui_config_path = ui_config_path();
    if ui_config_path.exists() {
        if let Ok(data) = fs::read_to_string(&ui_config_path) {
            if let Ok(map) = serde_json::from_str::<HashMap<String, serde_json::Value>>(&data) {
                for (key, val) in map {
                    if let Ok(cfg) = serde_json::from_value::<LlmConfig>(val) {
                        llm_configs.insert(key, normalize_llm_config(cfg));
                    }
                }
            }
        }
    }

    // Init memory files
    let mem_dir = project_dir.join("memory");
    let _ = fs::create_dir_all(&mem_dir);
    let mem_txt = mem_dir.join("global_mem.txt");
    if !mem_txt.exists() {
        let _ = fs::write(&mem_txt, "# [Global Memory - L2]\n");
    }
    let mem_insight = mem_dir.join("global_mem_insight.txt");
    if !mem_insight.exists() {
        let lang = detect_lang();
        let suffix = if lang == "en" { "_en" } else { "" };
        let tmpl = project_dir
            .join("assets")
            .join(format!("global_mem_insight_template{}.txt", suffix));
        if tmpl.exists() {
            let content = fs::read_to_string(&tmpl).unwrap_or_default();
            let _ = fs::write(&mem_insight, content);
        } else {
            let _ = fs::write(&mem_insight, "");
        }
    }

    // Init temp dir
    let _ = fs::create_dir_all(project_dir.join("temp"));

    Ok(Config {
        llm_configs,
        mixin_configs,
        lang: detect_lang(),
    })
}
