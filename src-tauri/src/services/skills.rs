// Skills 服务 — 技能注册/发现/管理
//
// 对应 Python app/services/skills.py
// 解析 SKILL.md 的 YAML front matter + 正文，支持目录注册和 zip 导入

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// 忽略的归档条目
const IGNORED_ENTRIES: &[&str] = &["__MACOSX", ".DS_Store"];

// ── 数据模型 ──────────────────────────────────────────────

/// 依赖命令解析结果
#[derive(Debug, Clone, serde::Serialize)]
pub struct DependencyCommand {
    pub command: Vec<String>,
    pub success: bool,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
}

/// 技能元数据
#[derive(Debug, Clone, serde::Serialize)]
pub struct SkillMetadata {
    pub name: String,
    pub path: String,
    pub metadata: serde_json::Value,
    #[serde(default)]
    pub dependency_commands: Vec<DependencyCommand>,
}

/// 注册结果
#[derive(Debug, serde::Serialize)]
pub struct SkillRegistrationResult {
    pub count: usize,
    pub registered: Vec<SkillMetadata>,
}

/// 技能列表
#[derive(Debug, serde::Serialize)]
pub struct SkillCollection {
    pub skills: Vec<SkillMetadata>,
}

/// 技能内容
#[derive(Debug, serde::Serialize)]
pub struct SkillContent {
    pub name: String,
    pub path: String,
    pub content: String,
}

// ── 内部记录 ──────────────────────────────────────────────

#[derive(Debug)]
struct SkillRecord {
    name: String,
    path: PathBuf,
    skill_file: PathBuf,
    metadata: serde_json::Value,
    dependency_commands: Vec<DependencyCommand>,
}

// ── 错误 ──────────────────────────────────────────────────

#[derive(Debug)]
pub enum SkillError {
    NotFound(String),
    BadRequest(String),
    Io(String),
}

impl std::fmt::Display for SkillError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(m) => write!(f, "未找到: {m}"),
            Self::BadRequest(m) => write!(f, "请求错误: {m}"),
            Self::Io(m) => write!(f, "IO 错误: {m}"),
        }
    }
}

impl std::error::Error for SkillError {}

// ── SkillService ──────────────────────────────────────────

pub struct SkillService {
    skills: HashMap<String, SkillRecord>,
}

impl SkillService {
    pub fn new() -> Self {
        let mut svc = Self {
            skills: HashMap::new(),
        };
        svc.auto_mount_from_env();
        svc
    }

    /// 清空所有技能
    pub fn clear(&mut self) -> usize {
        let count = self.skills.len();
        self.skills.clear();
        count
    }

    /// 注册目录下的所有技能
    pub fn register_directory(&mut self, path: &str) -> Result<SkillRegistrationResult, SkillError> {
        let root = Self::validate_path(path)?;
        let skill_files = Self::discover_skill_files(&root);
        if skill_files.is_empty() {
            return Err(SkillError::BadRequest(format!(
                "目录下未找到 SKILL.md: {}",
                root.display()
            )));
        }

        let pending = self.prepare_skills(&skill_files)?;
        let mut registered: Vec<SkillMetadata> = pending
            .into_iter()
            .map(|item| self.finalize_registration(item))
            .collect();
        registered.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(SkillRegistrationResult {
            count: registered.len(),
            registered,
        })
    }

    /// 列出技能元数据
    pub fn list_metadata(&self, filter_names: Option<&[&str]>) -> SkillCollection {
        let mut records: Vec<&SkillRecord> = self.skills.values().collect();

        if let Some(names) = filter_names {
            let set: HashSet<&str> = names.iter().copied().collect();
            records.retain(|r| set.contains(r.name.as_str()));
        }

        records.sort_by(|a, b| a.name.cmp(&b.name));

        SkillCollection {
            skills: records
                .iter()
                .map(|r| SkillMetadata {
                    name: r.name.clone(),
                    path: r.path.to_string_lossy().to_string(),
                    metadata: r.metadata.clone(),
                    dependency_commands: r.dependency_commands.clone(),
                })
                .collect(),
        }
    }

    /// 获取技能内容 (SKILL.md 正文, 不含 front matter)
    pub fn get_content(&self, name: &str) -> Result<SkillContent, SkillError> {
        let record = self
            .skills
            .get(name)
            .ok_or_else(|| SkillError::NotFound(format!("技能不存在: {name}")))?;

        let (_, body) = Self::parse_skill_file(&record.skill_file)?;
        Ok(SkillContent {
            name: name.to_string(),
            path: record.path.to_string_lossy().to_string(),
            content: body,
        })
    }

    /// 删除技能
    pub fn delete(&mut self, name: &str) -> Result<SkillMetadata, SkillError> {
        let record = self
            .skills
            .remove(name)
            .ok_or_else(|| SkillError::NotFound(format!("技能不存在: {name}")))?;

        Ok(SkillMetadata {
            name: record.name,
            path: record.path.to_string_lossy().to_string(),
            metadata: record.metadata,
            dependency_commands: record.dependency_commands,
        })
    }

    // ── WS handler ────────────────────────────────────────

    /// 通过 WS 路由分发的统一 handler
    pub async fn handle(
        &mut self,
        action: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, SkillError> {
        match action {
            "sync" => {
                let skill_id = params["skill_id"]
                    .as_str()
                    .ok_or_else(|| SkillError::BadRequest("缺少 skill_id".into()))?;
                let version = params["version"]
                    .as_str()
                    .ok_or_else(|| SkillError::BadRequest("缺少 version".into()))?;
                let oss_url = params["oss_url"]
                    .as_str()
                    .ok_or_else(|| SkillError::BadRequest("缺少 oss_url".into()))?;
                    
                self.sync_remote(skill_id, version, oss_url).await?;
                Ok(serde_json::json!({ "status": "synced" }))
            }
            "list" => {
                let names: Option<Vec<String>> = params
                    .get("names")
                    .and_then(|v| serde_json::from_value(v.clone()).ok());
                let name_refs: Option<Vec<&str>> =
                    names.as_ref().map(|v| v.iter().map(|s| s.as_str()).collect());
                let result = self.list_metadata(name_refs.as_deref());
                Ok(serde_json::to_value(result).unwrap_or_default())
            }
            "get" => {
                let name = params["name"]
                    .as_str()
                    .ok_or_else(|| SkillError::BadRequest("缺少 name 参数".into()))?;
                let result = self.get_content(name)?;
                Ok(serde_json::to_value(result).unwrap_or_default())
            }
            "register" => {
                let path = params["path"]
                    .as_str()
                    .ok_or_else(|| SkillError::BadRequest("缺少 path 参数".into()))?;
                let result = self.register_directory(path)?;
                Ok(serde_json::to_value(result).unwrap_or_default())
            }
            "delete" => {
                let name = params["name"]
                    .as_str()
                    .ok_or_else(|| SkillError::BadRequest("缺少 name 参数".into()))?;
                let result = self.delete(name)?;
                Ok(serde_json::to_value(result).unwrap_or_default())
            }
            "clear" => {
                let count = self.clear();
                Ok(serde_json::json!({ "cleared": count }))
            }
            _ => Err(SkillError::BadRequest(format!("未知操作: {action}"))),
        }
    }

    // ── 内部实现 ──────────────────────────────────────────

    pub async fn sync_remote(
        &mut self,
        skill_id: &str,
        version: &str,
        oss_url: &str,
    ) -> Result<(), SkillError> {
        let skills_path = std::env::var("AIO_SKILLS_PATH")
            .unwrap_or_else(|_| "/tmp/skills".to_string());
        
        let root = PathBuf::from(&skills_path.trim_matches('"').trim_matches('\''));
        if !root.exists() {
            std::fs::create_dir_all(&root).map_err(|e| SkillError::Io(format!("创建目录失败: {e}")))?;
        }
        
        let target_dir = root.join(skill_id);
        
        log::info!("[Skills] 下载并部署技能包 {} v{} -> {}", skill_id, version, target_dir.display());
        
        let resp = reqwest::get(oss_url).await.map_err(|e| SkillError::Io(format!("下载失败: {e}")))?;
        let bytes = resp.bytes().await.map_err(|e| SkillError::Io(format!("读取内容失败: {e}")))?;
        
        let tmp_path = std::env::temp_dir().join(format!("tab_dl_skill_{}.tar.gz", skill_id));
        std::fs::write(&tmp_path, &bytes).map_err(|e| SkillError::Io(format!("写入临时文件失败: {e}")))?;
        
        if !target_dir.exists() {
            std::fs::create_dir_all(&target_dir).map_err(|e| SkillError::Io(format!("创建目标目录失败: {e}")))?;
        }
        
        // 借用 tar 解压内容到原地
        let status = tokio::process::Command::new("tar")
            .arg("-xzf")
            .arg(&tmp_path)
            .arg("-C")
            .arg(&target_dir)
            .status()
            .await
            .map_err(|e| SkillError::Io(format!("TAR 解析失败: {e}")))?;
            
        let _ = std::fs::remove_file(&tmp_path);
        if !status.success() {
            return Err(SkillError::BadRequest("解压失败，返回非 0 状态".into()));
        }
        
        // 写入版本标志信息
        let meta_file = target_dir.join(".meta.json");
        let _ = std::fs::write(&meta_file, serde_json::json!({
            "skill_id": skill_id,
            "version": version
        }).to_string());
        
        // 自动注册挂载
        let res = self.register_directory(&target_dir.to_string_lossy())?;
        log::info!("[Skills] {} 已部署完毕, 当前已挂载技能数目: {}", skill_id, res.count);
        Ok(())
    }

    /// 从环境变量自动挂载
    fn auto_mount_from_env(&mut self) {
        let skills_path = match std::env::var("AIO_SKILLS_PATH") {
            Ok(p) => p.trim_matches('"').trim_matches('\'').to_string(),
            Err(_) => return,
        };

        let root = match PathBuf::from(&skills_path).canonicalize() {
            Ok(p) if p.exists() => p,
            _ => {
                log::error!("[Skills] AIO_SKILLS_PATH 不存在: {}", skills_path);
                return;
            }
        };

        let skill_files = Self::discover_skill_files(&root);
        if skill_files.is_empty() {
            log::warn!("[Skills] AIO_SKILLS_PATH 下无 SKILL.md: {}", skills_path);
            return;
        }

        match self.prepare_skills(&skill_files) {
            Ok(pending) => {
                let count = pending.len();
                for item in pending {
                    self.finalize_registration(item);
                }
                log::info!(
                    "[Skills] 从 AIO_SKILLS_PATH 自动挂载 {} 个技能",
                    count
                );
            }
            Err(e) => {
                log::error!("[Skills] 自动挂载失败: {}", e);
            }
        }
    }

    fn prepare_skills(
        &self,
        skill_files: &[PathBuf],
    ) -> Result<Vec<PendingSkill>, SkillError> {
        let mut pending = Vec::new();
        for skill_file in skill_files {
            let (metadata, _) = Self::parse_skill_file(skill_file)?;
            let name = metadata
                .get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
                    skill_file
                        .parent()
                        .and_then(|p| p.file_name())
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "unknown".to_string())
                });

            let mut meta = metadata.clone();
            if meta.get("name").is_none() {
                if let Some(obj) = meta.as_object_mut() {
                    obj.insert("name".to_string(), serde_json::json!(name));
                }
            }

            pending.push(PendingSkill {
                name,
                path: skill_file
                    .parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_default(),
                skill_file: skill_file.clone(),
                metadata: meta,
            });
        }
        self.validate_pending(&pending)?;
        Ok(pending)
    }

    fn validate_pending(
        &self,
        pending: &[PendingSkill],
    ) -> Result<(), SkillError> {
        let existing_names: HashSet<&str> = self.skills.keys().map(|s| s.as_str()).collect();
        let existing_paths: HashSet<&Path> = self.skills.values().map(|r| r.path.as_path()).collect();
        let mut seen_names: HashMap<&str, &Path> = HashMap::new();

        for item in pending {
            if existing_paths.contains(item.path.as_path()) {
                return Err(SkillError::BadRequest(format!(
                    "路径已注册: {}",
                    item.path.display()
                )));
            }
            if existing_names.contains(item.name.as_str()) {
                return Err(SkillError::BadRequest(format!(
                    "名称已注册: {}",
                    item.name
                )));
            }
            if let Some(prev) = seen_names.get(item.name.as_str()) {
                return Err(SkillError::BadRequest(format!(
                    "批次内名称重复: {} ({})",
                    item.name,
                    prev.display()
                )));
            }
            seen_names.insert(&item.name, &item.path);
        }
        Ok(())
    }

    fn finalize_registration(&mut self, item: PendingSkill) -> SkillMetadata {
        let dep_commands = Self::parse_dependencies(&item.path);

        let meta = SkillMetadata {
            name: item.name.clone(),
            path: item.path.to_string_lossy().to_string(),
            metadata: item.metadata.clone(),
            dependency_commands: dep_commands.clone(),
        };

        self.skills.insert(
            item.name.clone(),
            SkillRecord {
                name: item.name,
                path: item.path,
                skill_file: item.skill_file,
                metadata: item.metadata,
                dependency_commands: dep_commands,
            },
        );

        meta
    }

    /// 解析 SKILL.md — 返回 (front_matter_value, body_text)
    fn parse_skill_file(path: &Path) -> Result<(serde_json::Value, String), SkillError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| SkillError::Io(format!("{}: {}", path.display(), e)))?;

        let lines: Vec<&str> = content.lines().collect();
        if lines.is_empty() || lines[0].trim() != "---" {
            return Err(SkillError::BadRequest(format!(
                "SKILL.md 缺少 YAML front matter: {}",
                path.display()
            )));
        }

        let closing = lines[1..]
            .iter()
            .position(|line| line.trim() == "---")
            .map(|i| i + 1);

        let closing_idx = closing.ok_or_else(|| {
            SkillError::BadRequest(format!(
                "SKILL.md front matter 未闭合: {}",
                path.display()
            ))
        })?;

        let front_matter_text: String = lines[1..closing_idx].join("\n");
        let body: String = lines[closing_idx + 1..].join("\n");

        // 简单的 YAML key: value 解析 (不依赖 yaml crate)
        let metadata = Self::parse_simple_yaml(&front_matter_text, path)?;

        Ok((metadata, body.trim_start_matches('\n').to_string()))
    }

    /// 简单 YAML 解析 — 仅支持 key: value 单层结构
    fn parse_simple_yaml(text: &str, path: &Path) -> Result<serde_json::Value, SkillError> {
        let mut map = serde_json::Map::new();
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if let Some((key, value)) = trimmed.split_once(':') {
                let k = key.trim().to_string();
                let v = value
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string();
                map.insert(k, serde_json::Value::String(v));
            } else {
                return Err(SkillError::BadRequest(format!(
                    "无效的 YAML 行: {} ({})",
                    trimmed,
                    path.display()
                )));
            }
        }
        Ok(serde_json::Value::Object(map))
    }

    /// 解析依赖命令 (不执行)
    fn parse_dependencies(skill_dir: &Path) -> Vec<DependencyCommand> {
        let mut commands = Vec::new();

        // Python: requirements.txt
        let req_file = skill_dir.join("requirements.txt");
        if req_file.exists() {
            if let Ok(content) = std::fs::read_to_string(&req_file) {
                let has_packages = content
                    .lines()
                    .any(|l| !l.trim().is_empty() && !l.trim().starts_with('#'));
                if has_packages {
                    let venv = skill_dir.join(".venv").join("bin").join("python");
                    commands.push(DependencyCommand {
                        command: vec![
                            "uv".into(),
                            "pip".into(),
                            "install".into(),
                            "--python".into(),
                            venv.to_string_lossy().to_string(),
                            "-r".into(),
                            req_file.to_string_lossy().to_string(),
                        ],
                        success: true,
                        stdout: None,
                        stderr: None,
                    });
                }
            }
        }

        // Node.js: package.json
        let pkg_file = skill_dir.join("package.json");
        if pkg_file.exists() {
            if let Ok(content) = std::fs::read_to_string(&pkg_file) {
                if let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&content) {
                    let has_deps = pkg.get("dependencies").and_then(|v| v.as_object()).map_or(false, |m| !m.is_empty())
                        || pkg.get("devDependencies").and_then(|v| v.as_object()).map_or(false, |m| !m.is_empty());
                    if has_deps {
                        commands.push(DependencyCommand {
                            command: vec![
                                "npm".into(),
                                "install".into(),
                                "--prefix".into(),
                                skill_dir.to_string_lossy().to_string(),
                            ],
                            success: true,
                            stdout: None,
                            stderr: None,
                        });
                    }
                }
            }
        }

        commands
    }

    /// 发现目录下所有 SKILL.md 文件
    fn discover_skill_files(root: &Path) -> Vec<PathBuf> {
        let mut candidates: HashSet<PathBuf> = HashSet::new();

        if root.is_file() {
            if root.file_name().map_or(false, |n| n == "SKILL.md") {
                candidates.insert(root.to_path_buf());
            }
            return candidates.into_iter().collect();
        }

        // 直接子目录
        let single = root.join("SKILL.md");
        if single.is_file() {
            candidates.insert(single);
        }

        // 递归搜索
        if let Ok(walker) = Self::walk_dir(root) {
            for entry in walker {
                if entry.file_name().map_or(false, |n| n == "SKILL.md") {
                    // 跳过忽略条目
                    let dominated = entry.components().any(|c| {
                        let s = c.as_os_str().to_string_lossy();
                        IGNORED_ENTRIES.contains(&s.as_ref()) || s.starts_with('.')
                    });
                    if !dominated {
                        candidates.insert(entry);
                    }
                }
            }
        }

        let mut result: Vec<_> = candidates.into_iter().collect();
        result.sort();
        result
    }

    /// 简单递归目录遍历
    fn walk_dir(root: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
        let mut result = Vec::new();
        Self::walk_dir_inner(root, &mut result)?;
        Ok(result)
    }

    fn walk_dir_inner(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), std::io::Error> {
        if !dir.is_dir() {
            return Ok(());
        }
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                Self::walk_dir_inner(&path, out)?;
            } else {
                out.push(path);
            }
        }
        Ok(())
    }

    fn validate_path(path: &str) -> Result<PathBuf, SkillError> {
        let p = Path::new(path);
        if !p.exists() {
            return Err(SkillError::BadRequest(format!("路径不存在: {path}")));
        }
        Ok(p.to_path_buf())
    }
}

// ── 内部类型 ──────────────────────────────────────────────

struct PendingSkill {
    name: String,
    path: PathBuf,
    skill_file: PathBuf,
    metadata: serde_json::Value,
}
