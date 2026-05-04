use std::collections::HashMap;
use std::io::BufRead;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Metadata for a single installed skill
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillEntry {
    pub name: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub source_url: String,
    #[serde(default)]
    pub version: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub installed_at: String,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub file_count: usize,
}

fn default_enabled() -> bool {
    true
}

/// The on-disk .meta.json structure
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillsMeta {
    #[serde(default)]
    pub skills: HashMap<String, SkillEntry>,
}

/// Manages the skills/ directory and .meta.json
#[derive(Debug, Clone)]
pub struct SkillsManager {
    pub skills_dir: PathBuf,
    pub meta_path: PathBuf,
}

impl SkillsManager {
    pub fn new(project_dir: &Path) -> Self {
        let skills_dir = project_dir.join("skills");
        let meta_path = skills_dir.join(".meta.json");
        Self {
            skills_dir,
            meta_path,
        }
    }

    /// Ensure the skills directory and meta file exist
    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.skills_dir)?;
        if !self.meta_path.exists() {
            let meta = SkillsMeta::default();
            let json = serde_json::to_string_pretty(&meta).unwrap_or_default();
            std::fs::write(&self.meta_path, json)?;
        }
        Ok(())
    }

    /// Load meta from disk
    pub fn load_meta(&self) -> Result<SkillsMeta, String> {
        let data = std::fs::read_to_string(&self.meta_path)
            .map_err(|e| format!("Failed to read skills meta: {e}"))?;
        serde_json::from_str(&data)
            .map_err(|e| format!("Failed to parse skills meta: {e}"))
    }

    /// Save meta to disk
    pub fn save_meta(&self, meta: &SkillsMeta) -> Result<(), String> {
        let json = serde_json::to_string_pretty(meta)
            .map_err(|e| format!("Failed to serialize skills meta: {e}"))?;
        std::fs::write(&self.meta_path, json)
            .map_err(|e| format!("Failed to write skills meta: {e}"))?;
        Ok(())
    }

    /// List all installed skills
    pub fn list_skills(&self) -> Result<Vec<SkillEntry>, String> {
        self.ensure_dirs().map_err(|e| format!("{e}"))?;
        let meta = self.load_meta()?;
        let mut skills: Vec<SkillEntry> = meta.skills.into_values().collect();
        skills.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(skills)
    }

    /// Normalize a GitHub/clawhub URL to a skill name
    fn url_to_name(url: &str) -> String {
        let url = url.trim().trim_end_matches('/').trim_end_matches(".git");
        // Extract "owner/repo" from GitHub URL
        if let Some(rest) = url.strip_prefix("https://github.com/") {
            return rest.replace('/', "_").to_lowercase();
        }
        if url.contains("github.com") {
            // Try to extract from any github url
            if let Some(pos) = url.find("github.com/") {
                let rest = &url[pos + 11..];
                return rest.replace('/', "_").to_lowercase();
            }
        }
        // Generic: take last two path segments
        let parts: Vec<&str> = url
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();
        if parts.len() >= 2 {
            return format!("{}_{}", parts[parts.len() - 2], parts[parts.len() - 1])
                .to_lowercase();
        }
        if let Some(last) = parts.last() {
            return last.to_lowercase();
        }
        "unknown_skill".to_string()
    }

    /// Read the first non-empty line of a file, useful for extracting description
    fn read_first_line(path: &Path) -> String {
        if let Ok(file) = std::fs::File::open(path) {
            let reader = std::io::BufReader::new(file);
            for line in reader.lines().flatten() {
                let trimmed = line.trim();
                if !trimmed.is_empty() && !trimmed.starts_with('#') && !trimmed.starts_with("//") {
                    return trimmed.chars().take(200).collect();
                }
                if trimmed.starts_with("# ") {
                    return trimmed[2..].chars().take(200).collect();
                }
            }
        }
        String::new()
    }

    /// Scan installed skill directory and update file list + file_count
    fn scan_skill_files(&self, dir: &Path) -> Vec<String> {
        let mut files = Vec::new();
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name == ".meta.json" {
                    continue;
                }
                if entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                    files.push(name);
                }
            }
        }
        files.sort();
        files
    }

    /// Install a skill from a URL (GitHub, Clawhub, or raw file URL)
    pub fn install_skill(&self, url: &str) -> Result<SkillEntry, String> {
        self.ensure_dirs().map_err(|e| format!("{e}"))?;

        let name = Self::url_to_name(url);
        let skill_dir = self.skills_dir.join(&name);
        let now = Utc::now().to_rfc3339();

        // Determine download URL: GitHub URL -> raw zip download
        let download_url = if url.contains("github.com") && !url.ends_with(".md") && !url.ends_with(".json") && !url.ends_with(".yaml") {
            // Convert to archive download
            let clean = url.trim().trim_end_matches('/').trim_end_matches(".git");
            // Extract owner/repo
            if let Some(rest) = clean.strip_prefix("https://github.com/") {
                let parts: Vec<&str> = rest.split('/').collect();
                if parts.len() >= 2 {
                    let branch = if parts.len() >= 4 && parts[2] == "tree" {
                        parts[3..].join("/")
                    } else {
                        "main".to_string()
                    };
                    let owner = parts[0];
                    let repo = parts[1];
                    format!("https://api.github.com/repos/{owner}/{repo}/zipball/{branch}")
                } else {
                    url.to_string()
                }
            } else {
                url.to_string()
            }
        } else {
            url.to_string()
        };

        // Download
        let client = reqwest::blocking::Client::builder()
            .user_agent("Generic-Coder-Skills/1.0")
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {e}"))?;

        let response = client
            .get(&download_url)
            .send()
            .map_err(|e| format!("Failed to download skill: {e}"))?;

        if !response.status().is_success() {
            return Err(format!(
                "Failed to download skill: HTTP {}",
                response.status()
            ));
        }

        let bytes = response
            .bytes()
            .map_err(|e| format!("Failed to read response: {e}"))?;

        // Try to extract as zip (GitHub archive)
        let was_extracted = if bytes.starts_with(b"PK") || download_url.contains("zipball") {
            let cursor = std::io::Cursor::new(&bytes);
            if let Ok(mut archive) = zip::ZipArchive::new(cursor) {
                std::fs::create_dir_all(&skill_dir).ok();
                // Only extract first-level content, skipping the wrapper dir
                for i in 0..archive.len() {
                    if let Ok(mut file) = archive.by_index(i) {
                        let name_in_zip = file.name().to_string();
                        let rel_path = if let Some(slash) = name_in_zip.find('/') {
                            &name_in_zip[slash + 1..]
                        } else {
                            &name_in_zip
                        };
                        if rel_path.is_empty() {
                            continue;
                        }
                        let dest = skill_dir.join(rel_path);
                        if file.is_dir() {
                            std::fs::create_dir_all(&dest).ok();
                        } else {
                            if let Some(parent) = dest.parent() {
                                std::fs::create_dir_all(parent).ok();
                            }
                            let mut out = std::fs::File::create(&dest).ok();
                            if let Some(ref mut f) = out {
                                std::io::copy(&mut file, f).ok();
                            }
                        }
                    }
                }
                std::fs::create_dir_all(&skill_dir)
                    .map_err(|e| format!("Failed to create skill dir: {e}"))?;
                true
            } else {
                false
            }
        } else {
            false
        };

        // If not zip, save as a single file in the skill dir
        if !was_extracted {
            std::fs::create_dir_all(&skill_dir)
                .map_err(|e| format!("Failed to create skill dir: {e}"))?;
            let ext = if download_url.ends_with(".md") {
                "md"
            } else if download_url.ends_with(".json") {
                "json"
            } else if download_url.ends_with(".yaml") || download_url.ends_with(".yml") {
                "yaml"
            } else {
                "txt"
            };
            let filename = format!("skill.{ext}");
            std::fs::write(skill_dir.join(&filename), &bytes)
                .map_err(|e| format!("Failed to write skill file: {e}"))?;
        }

        // Scan files and build metadata
        let files = self.scan_skill_files(&skill_dir);
        let file_count = files.len();

        // Try to extract description from README or first md file
        let readme_path = skill_dir.join("README.md");
        let description_path = if readme_path.exists() {
            readme_path
        } else {
            files
                .iter()
                .find(|f| f.ends_with(".md"))
                .map(|f| skill_dir.join(f))
                .unwrap_or_else(|| skill_dir.join("skill.md"))
        };
        let description = Self::read_first_line(&description_path);

        let display_name = name
            .replace('_', " ")
            .split_whitespace()
            .map(|w| {
                let mut c = w.chars();
                c.next().map(|f| f.to_uppercase().to_string() + c.as_str()).unwrap_or_default()
            })
            .collect::<Vec<_>>()
            .join(" ");

        let entry = SkillEntry {
            name: name.clone(),
            display_name,
            description,
            source_url: url.to_string(),
            version: "1.0.0".to_string(),
            enabled: true,
            installed_at: now.clone(),
            updated_at: now,
            files: files.clone(),
            file_count,
        };

        // Update meta
        let mut meta = self.load_meta()?;
        meta.skills.insert(name, entry.clone());
        self.save_meta(&meta)?;

        Ok(entry)
    }

    /// Toggle enable/disable of a skill
    pub fn toggle_skill(&self, name: &str) -> Result<SkillEntry, String> {
        self.ensure_dirs().map_err(|e| format!("{e}"))?;
        let mut meta = self.load_meta()?;
        let entry = meta
            .skills
            .get_mut(name)
            .ok_or_else(|| format!("Skill not found: {name}"))?;
        entry.enabled = !entry.enabled;
        let entry = entry.clone();
        self.save_meta(&meta)?;
        Ok(entry)
    }

    /// Delete a skill completely
    pub fn delete_skill(&self, name: &str) -> Result<(), String> {
        self.ensure_dirs().map_err(|e| format!("{e}"))?;
        let mut meta = self.load_meta()?;
        if meta.skills.remove(name).is_none() {
            return Err(format!("Skill not found: {name}"));
        }
        let skill_dir = self.skills_dir.join(name);
        if skill_dir.exists() {
            std::fs::remove_dir_all(&skill_dir)
                .map_err(|e| format!("Failed to delete skill dir: {e}"))?;
        }
        self.save_meta(&meta)?;
        Ok(())
    }

    /// Upgrade a skill by re-downloading from its source URL
    pub fn upgrade_skill(&self, name: &str) -> Result<SkillEntry, String> {
        self.ensure_dirs().map_err(|e| format!("{e}"))?;
        let meta = self.load_meta()?;
        let existing = meta
            .skills
            .get(name)
            .ok_or_else(|| format!("Skill not found: {name}"))?;
        let url = existing.source_url.clone();

        // Delete old install
        let skill_dir = self.skills_dir.join(name);
        if skill_dir.exists() {
            std::fs::remove_dir_all(&skill_dir)
                .map_err(|e| format!("Failed to remove old skill during upgrade: {e}"))?;
        }

        // Re-install
        self.install_skill(&url)
    }

    /// Get a single skill entry
    pub fn get_skill(&self, name: &str) -> Result<SkillEntry, String> {
        let meta = self.load_meta()?;
        meta.skills
            .get(name)
            .cloned()
            .ok_or_else(|| format!("Skill not found: {name}"))
    }

    /// Read the main definition file of a skill for preview
    pub fn preview_skill(&self, name: &str) -> Result<Value, String> {
        let entry = self.get_skill(name)?;
        let skill_dir = self.skills_dir.join(name);

        let main_file = entry
            .files
            .iter()
            .find(|f| f.ends_with(".md") || f.contains("README"))
            .or_else(|| entry.files.first())
            .ok_or_else(|| format!("No files found in skill: {name}"))?;

        let path = skill_dir.join(main_file);
        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read skill file: {e}"))?;

        Ok(serde_json::json!({
            "name": name,
            "file": main_file,
            "content": content,
            "size": content.len(),
        }))
    }

    /// Scan skills/ directory for preset skill dirs not yet in .meta.json and auto-register them.
    /// Called once at server startup so preset skills are always available to the agent.
    pub fn bootstrap_presets(&self) -> Result<Vec<SkillEntry>, String> {
        self.ensure_dirs().map_err(|e| format!("{e}"))?;
        let mut meta = self.load_meta()?;
        let now = Utc::now().to_rfc3339();
        let mut added = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&self.skills_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                // Skip meta file and anything already registered
                if name == ".meta.json" || meta.skills.contains_key(&name) {
                    continue;
                }
                // Only consider directories
                if !entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                    continue;
                }

                let skill_dir = self.skills_dir.join(&name);
                let files = self.scan_skill_files(&skill_dir);
                let file_count = files.len();

                let readme_path = skill_dir.join("README.md");
                let description_path = if readme_path.exists() {
                    readme_path
                } else {
                    files
                        .iter()
                        .find(|f| f.ends_with(".md"))
                        .map(|f| skill_dir.join(f))
                        .unwrap_or_else(|| skill_dir.join("skill.md"))
                };
                let description = Self::read_first_line(&description_path);

                let display_name = name
                    .replace('_', " ")
                    .replace('-', " ")
                    .split_whitespace()
                    .map(|w| {
                        let mut c = w.chars();
                        c.next()
                            .map(|f| f.to_uppercase().to_string() + c.as_str())
                            .unwrap_or_default()
                    })
                    .collect::<Vec<_>>()
                    .join(" ");

                let entry = SkillEntry {
                    name: name.clone(),
                    display_name,
                    description,
                    source_url: String::new(), // local preset, no URL
                    version: "1.0.0".to_string(),
                    enabled: true,
                    installed_at: now.clone(),
                    updated_at: now.clone(),
                    files: files.clone(),
                    file_count,
                };

                meta.skills.insert(name.clone(), entry.clone());
                added.push(entry);
                log::info!("Bootstrapped preset skill: {name}");
            }
        }

        if !added.is_empty() {
            self.save_meta(&meta)?;
        }

        Ok(added)
    }

    /// Get active (enabled) skills summary for injection into system prompt
    pub fn active_skills_summary(&self) -> String {
        let meta = match self.load_meta() {
            Ok(m) => m,
            Err(_) => return String::new(),
        };

        let enabled: Vec<&SkillEntry> = meta
            .skills
            .values()
            .filter(|s| s.enabled)
            .collect();

        if enabled.is_empty() {
            return String::new();
        }

        let mut lines = vec![
            format!("\n## Active Agent Skills ({} installed, {} enabled)\n", meta.skills.len(), enabled.len()),
            "You have the following skills installed. Each skill is a reusable workflow — read its file before following its instructions.\n".to_string(),
            "| Skill | Description | When to Use |".to_string(),
            "|-------|-------------|-------------|".to_string(),
        ];

        for s in &enabled {
            let desc = if s.description.is_empty() { "-" } else { &s.description };
            let trigger = match s.name.as_str() {
                "webfetch" => "Reading URLs, fetching web content",
                "create-skill" => "User wants to create a new skill",
                "file-search" => "Exploring/searching codebases",
                "code-review" => "Reviewing code changes",
                "self-audit" => "Task stalled, failures, need to pivot",
                _ => "See skill README",
            };
            lines.push(format!(
                "| **{}** | {} | {} |",
                s.display_name, desc, trigger
            ));
        }

        lines.push(String::new());
        lines.push("**How to use a skill**: When a task matches a skill's trigger conditions, call `file_read` on `skills/<skill-name>/README.md` to read the full workflow, then follow its instructions.\n".to_string());

        lines.join("\n")
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_to_name_github() {
        let name = SkillsManager::url_to_name("https://github.com/user/my-skill");
        assert_eq!(name, "user_my-skill");
    }

    #[test]
    fn test_url_to_name_github_with_git() {
        let name = SkillsManager::url_to_name("https://github.com/user/my-skill.git");
        assert_eq!(name, "user_my-skill");
    }

    #[test]
    fn test_url_to_name_generic() {
        let name = SkillsManager::url_to_name("https://clawhub.io/skills/awesome-skill");
        assert!(name.contains("skills"));
        assert!(name.contains("awesome-skill"));
    }

    #[test]
    fn test_skills_manager_init() {
        let temp = std::env::temp_dir().join("gc_skills_test");
        let _ = std::fs::remove_dir_all(&temp);
        let mgr = SkillsManager::new(&temp);
        mgr.ensure_dirs().unwrap();
        assert!(mgr.skills_dir.exists());
        assert!(mgr.meta_path.exists());
        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_list_empty_skills() {
        let temp = std::env::temp_dir().join("gc_skills_test2");
        let _ = std::fs::remove_dir_all(&temp);
        let mgr = SkillsManager::new(&temp);
        let skills = mgr.list_skills().unwrap();
        assert!(skills.is_empty());
        let _ = std::fs::remove_dir_all(&temp);
    }
}
