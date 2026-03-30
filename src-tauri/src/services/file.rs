// File 服务 — 文件系统操作
//
// 对应 Python app/services/file.py
// 提供文件读写、目录操作、搜索等

use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use tokio::fs;
use tokio::io::AsyncWriteExt;

use crate::core::error::{ServiceError, ServiceResult};

/// 文件服务
pub struct FileService {
    /// 工作区根目录
    workspace: PathBuf,
}

impl FileService {
    pub fn new() -> Self {
        let workspace = PathBuf::from(
            std::env::var("WORKSPACE")
                .or_else(|_| std::env::var("HOME"))
                .unwrap_or_else(|_| "/tmp".to_string()),
        );
        Self { workspace }
    }

    /// 读取文件 (支持行范围 + base64 二进制)
    pub async fn read_file(
        &self,
        path: &str,
        start_line: Option<usize>,
        end_line: Option<usize>,
        encoding: Option<&str>,
    ) -> ServiceResult {
        let abs = self.resolve_path(path);
        if !abs.exists() {
            return Err(ServiceError::not_found(format!("文件不存在: {path}")));
        }

        // base64 模式: 读取原始字节
        if encoding == Some("base64") {
            let raw = fs::read(&abs)
                .await
                .map_err(|e| ServiceError::internal(format!("读取失败: {e}")))?;
            use base64::Engine as _;
            let b64 = base64::engine::general_purpose::STANDARD.encode(&raw);
            return Ok(json!({
                "path": abs.to_string_lossy(),
                "content": b64,
                "encoding": "base64",
                "size": raw.len(),
            }));
        }

        // 文本模式
        let content = fs::read_to_string(&abs)
            .await
            .map_err(|e| ServiceError::internal(format!("读取失败: {e}")))?;

        // 行范围过滤
        let result_content = if start_line.is_some() || end_line.is_some() {
            let lines: Vec<&str> = content.lines().collect();
            let start = start_line.unwrap_or(0);
            let end = end_line.unwrap_or(lines.len()).min(lines.len());
            lines[start..end].join("\n")
        } else {
            content.clone()
        };

        let meta = fs::metadata(&abs).await.ok();
        Ok(json!({
            "path": abs.to_string_lossy(),
            "content": result_content,
            "size": meta.as_ref().map(|m| m.len()),
        }))
    }

    /// 写入文件 (支持 base64 二进制 + append)
    pub async fn write_file(
        &self,
        path: &str,
        content: &str,
        append: bool,
        encoding: Option<&str>,
    ) -> ServiceResult {
        let abs = self.resolve_path(path);
        // 确保父目录存在
        if let Some(parent) = abs.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| ServiceError::internal(format!("创建目录失败: {e}")))?;
        }

        let bytes_written = if encoding == Some("base64") {
            // base64 解码写入
            use base64::Engine as _;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(content)
                .map_err(|e| ServiceError::bad_request(format!("base64 解码失败: {e}")))?;
            let len = bytes.len();
            if append {
                let mut f = tokio::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&abs)
                    .await
                    .map_err(|e| ServiceError::internal(format!("打开文件失败: {e}")))?;
                f.write_all(&bytes)
                    .await
                    .map_err(|e| ServiceError::internal(format!("写入失败: {e}")))?;
            } else {
                fs::write(&abs, &bytes)
                    .await
                    .map_err(|e| ServiceError::internal(format!("写入失败: {e}")))?;
            }
            len
        } else {
            // 文本写入
            let len = content.len();
            if append {
                let mut f = tokio::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&abs)
                    .await
                    .map_err(|e| ServiceError::internal(format!("打开文件失败: {e}")))?;
                f.write_all(content.as_bytes())
                    .await
                    .map_err(|e| ServiceError::internal(format!("追加失败: {e}")))?;
            } else {
                fs::write(&abs, content)
                    .await
                    .map_err(|e| ServiceError::internal(format!("写入失败: {e}")))?;
            }
            len
        };

        Ok(json!({
            "path": abs.to_string_lossy(),
            "bytes_written": bytes_written,
            "written": true,
        }))
    }

    /// 追加写入
    pub async fn append_file(&self, path: &str, content: &str) -> ServiceResult {
        self.write_file(path, content, true, None).await
    }

    /// 删除文件/目录
    pub async fn delete_file(&self, path: &str) -> ServiceResult {
        let abs = self.resolve_path(path);
        if !abs.exists() {
            return Err(ServiceError::not_found(format!("文件不存在: {path}")));
        }
        if abs.is_dir() {
            fs::remove_dir_all(&abs)
                .await
                .map_err(|e| ServiceError::internal(format!("删除目录失败: {e}")))?;
        } else {
            fs::remove_file(&abs)
                .await
                .map_err(|e| ServiceError::internal(format!("删除文件失败: {e}")))?;
        }
        Ok(json!({"deleted": true, "path": abs.to_string_lossy()}))
    }

    /// 列出目录 (增强版: 过滤/排序/metadata)
    pub async fn list_dir(
        &self,
        path: &str,
        recursive: bool,
        show_hidden: bool,
        max_depth: Option<usize>,
        max_files: Option<usize>,
    ) -> ServiceResult {
        let abs = self.resolve_path(path);
        if !abs.is_dir() {
            return Err(ServiceError::bad_request(format!("不是目录: {path}")));
        }
        let entries = self.list_dir_entries(
            &abs, recursive, show_hidden, 0,
            max_depth.unwrap_or(3), max_files.unwrap_or(10000),
        ).await?;
        Ok(json!({
            "path": abs.to_string_lossy(),
            "entries": entries,
            "total": entries.len(),
        }))
    }

    /// 创建目录
    pub async fn mkdir(&self, path: &str) -> ServiceResult {
        let abs = self.resolve_path(path);
        fs::create_dir_all(&abs)
            .await
            .map_err(|e| ServiceError::internal(format!("创建目录失败: {e}")))?;
        Ok(json!({"created": true, "path": abs.to_string_lossy()}))
    }

    /// 移动/重命名
    pub async fn move_file(&self, src: &str, dst: &str) -> ServiceResult {
        let abs_src = self.resolve_path(src);
        let abs_dst = self.resolve_path(dst);
        if !abs_src.exists() {
            return Err(ServiceError::not_found(format!("源文件不存在: {src}")));
        }
        if let Some(parent) = abs_dst.parent() {
            fs::create_dir_all(parent).await.ok();
        }
        fs::rename(&abs_src, &abs_dst)
            .await
            .map_err(|e| ServiceError::internal(format!("移动失败: {e}")))?;
        Ok(json!({
            "moved": true,
            "from": abs_src.to_string_lossy(),
            "to": abs_dst.to_string_lossy(),
        }))
    }

    /// 复制
    pub async fn copy_file(&self, src: &str, dst: &str) -> ServiceResult {
        let abs_src = self.resolve_path(src);
        let abs_dst = self.resolve_path(dst);
        if !abs_src.exists() {
            return Err(ServiceError::not_found(format!("源文件不存在: {src}")));
        }
        if let Some(parent) = abs_dst.parent() {
            fs::create_dir_all(parent).await.ok();
        }
        fs::copy(&abs_src, &abs_dst)
            .await
            .map_err(|e| ServiceError::internal(format!("复制失败: {e}")))?;
        Ok(json!({
            "copied": true,
            "from": abs_src.to_string_lossy(),
            "to": abs_dst.to_string_lossy(),
        }))
    }

    /// 文件是否存在
    pub async fn exists(&self, path: &str) -> ServiceResult {
        let abs = self.resolve_path(path);
        Ok(json!({
            "exists": abs.exists(),
            "is_file": abs.is_file(),
            "is_dir": abs.is_dir(),
            "path": abs.to_string_lossy(),
        }))
    }

    /// 文件信息
    pub async fn stat(&self, path: &str) -> ServiceResult {
        let abs = self.resolve_path(path);
        let meta = fs::metadata(&abs)
            .await
            .map_err(|e| ServiceError::not_found(format!("stat 失败: {e}")))?;
        Ok(json!({
            "path": abs.to_string_lossy(),
            "size": meta.len(),
            "is_file": meta.is_file(),
            "is_dir": meta.is_dir(),
            "readonly": meta.permissions().readonly(),
        }))
    }

    /// 字符串替换 (对应 Python str_replace)
    pub async fn str_replace(
        &self,
        path: &str,
        old_str: &str,
        new_str: &str,
    ) -> ServiceResult {
        let abs = self.resolve_path(path);
        if !abs.exists() {
            return Err(ServiceError::not_found(format!("文件不存在: {path}")));
        }
        let content = fs::read_to_string(&abs)
            .await
            .map_err(|e| ServiceError::internal(format!("读取失败: {e}")))?;

        let count = content.matches(old_str).count();
        if count == 0 {
            return Ok(json!({"path": abs.to_string_lossy(), "replaced_count": 0}));
        }

        let updated = content.replace(old_str, new_str);
        fs::write(&abs, &updated)
            .await
            .map_err(|e| ServiceError::internal(format!("写入失败: {e}")))?;

        Ok(json!({
            "path": abs.to_string_lossy(),
            "replaced_count": count,
        }))
    }

    /// 文本搜索 (grep) — 优先用 ripgrep，降级纯 Rust 递归搜索
    pub async fn grep(
        &self,
        path: &str,
        pattern: &str,
        max_results: usize,
        include: Option<Vec<String>>,
        case_insensitive: bool,
        fixed_strings: bool,
    ) -> ServiceResult {
        let abs = self.resolve_path(path);

        // 优先用 ripgrep (外部命令, 不需要安装也不会报错)
        if let Ok(rg_result) = self.grep_with_rg(
            &abs, pattern, max_results, &include, case_insensitive, fixed_strings,
        ).await {
            return Ok(rg_result);
        }

        // 降级: 纯 Rust 搜索 (单文件或目录递归)
        if abs.is_file() {
            return self.search_in_file(&abs, pattern, max_results).await;
        }

        if abs.is_dir() {
            return self.grep_dir_recursive(&abs, pattern, max_results, case_insensitive).await;
        }

        Err(ServiceError::not_found(format!("路径不存在: {path}")))
    }

    /// ripgrep 搜索
    async fn grep_with_rg(
        &self,
        path: &Path,
        pattern: &str,
        max_results: usize,
        include: &Option<Vec<String>>,
        case_insensitive: bool,
        fixed_strings: bool,
    ) -> Result<Value, ServiceError> {
        let mut cmd = tokio::process::Command::new("rg");
        cmd.arg("--json")
            .arg("--max-count").arg(max_results.to_string());

        if case_insensitive {
            cmd.arg("-i");
        }
        if fixed_strings {
            cmd.arg("-F");
        }
        if let Some(includes) = include {
            for inc in includes {
                cmd.arg("--glob").arg(inc);
            }
        }

        cmd.arg(pattern).arg(path);

        let output = cmd.output().await
            .map_err(|e| ServiceError::internal(format!("rg 执行失败: {e}")))?;

        // rg exit 2 = 真错误
        if output.status.code() == Some(2) && output.stdout.is_empty() {
            return Err(ServiceError::internal("rg 执行错误"));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut matches = Vec::new();
        let mut files_matched = std::collections::HashSet::new();

        for line in stdout.lines() {
            if line.is_empty() { continue; }
            let Ok(msg) = serde_json::from_str::<Value>(line) else { continue };
            if msg["type"].as_str() != Some("match") { continue; }
            if matches.len() >= max_results { break; }

            let data = &msg["data"];
            let file_path = data["path"]["text"].as_str().unwrap_or("");
            let line_num = data["line_number"].as_u64().unwrap_or(0);
            let line_text = data["lines"]["text"].as_str().unwrap_or("").trim_end();
            files_matched.insert(file_path.to_string());

            matches.push(json!({
                "file": file_path,
                "line": line_num,
                "content": line_text,
            }));
        }

        Ok(json!({
            "path": path.to_string_lossy(),
            "pattern": pattern,
            "matches": matches,
            "match_count": matches.len(),
            "files_matched": files_matched.len(),
        }))
    }

    /// 单文件文本搜索 (fallback)
    async fn search_in_file(
        &self,
        path: &Path,
        pattern: &str,
        max_results: usize,
    ) -> ServiceResult {
        let content = fs::read_to_string(path)
            .await
            .map_err(|e| ServiceError::internal(format!("读取失败: {e}")))?;

        let mut matches = Vec::new();
        for (i, line) in content.lines().enumerate() {
            if line.contains(pattern) {
                matches.push(json!({
                    "line": i + 1,
                    "content": line,
                }));
                if matches.len() >= max_results {
                    break;
                }
            }
        }

        Ok(json!({
            "path": path.to_string_lossy(),
            "pattern": pattern,
            "matches": matches,
            "total": matches.len(),
        }))
    }

    /// 纯 Rust 目录递归搜索 (rg 不可用时的 fallback)
    async fn grep_dir_recursive(
        &self,
        dir: &Path,
        pattern: &str,
        max_results: usize,
        case_insensitive: bool,
    ) -> ServiceResult {
        let mut matches = Vec::new();
        let mut files_matched = std::collections::HashSet::new();
        let pattern_lower = if case_insensitive { pattern.to_lowercase() } else { String::new() };

        self.grep_walk(dir, pattern, &pattern_lower, case_insensitive, max_results, &mut matches, &mut files_matched).await;

        Ok(json!({
            "path": dir.to_string_lossy(),
            "pattern": pattern,
            "matches": matches,
            "match_count": matches.len(),
            "files_matched": files_matched.len(),
        }))
    }

    /// 递归遍历目录搜索文本
    fn grep_walk<'a>(
        &'a self,
        dir: &'a Path,
        pattern: &'a str,
        pattern_lower: &'a str,
        case_insensitive: bool,
        max_results: usize,
        matches: &'a mut Vec<Value>,
        files_matched: &'a mut std::collections::HashSet<String>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            if matches.len() >= max_results { return; }

            let Ok(mut read_dir) = fs::read_dir(dir).await else { return };
            while let Ok(Some(entry)) = read_dir.next_entry().await {
                if matches.len() >= max_results { break; }

                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();

                // 跳过隐藏文件和常见忽略目录
                if name.starts_with('.') || name == "node_modules" || name == "target" || name == "__pycache__" {
                    continue;
                }

                let is_dir = entry.metadata().await.map(|m| m.is_dir()).unwrap_or(false);

                if is_dir {
                    self.grep_walk(&path, pattern, pattern_lower, case_insensitive, max_results, matches, files_matched).await;
                } else {
                    // 跳过二进制文件 (简单判断: 后缀)
                    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                    if matches!(ext, "png" | "jpg" | "jpeg" | "gif" | "ico" | "woff" | "woff2" | "ttf" | "eot" | "zip" | "tar" | "gz" | "pdf" | "exe" | "dll" | "so" | "dylib") {
                        continue;
                    }

                    // 读取并搜索
                    let Ok(content) = fs::read_to_string(&path).await else { continue };
                    let file_path_str = path.to_string_lossy().to_string();

                    for (i, line) in content.lines().enumerate() {
                        if matches.len() >= max_results { break; }

                        let found = if case_insensitive {
                            line.to_lowercase().contains(pattern_lower)
                        } else {
                            line.contains(pattern)
                        };

                        if found {
                            files_matched.insert(file_path_str.clone());
                            matches.push(json!({
                                "file": file_path_str,
                                "line": i + 1,
                                "content": line,
                            }));
                        }
                    }
                }
            }
        })
    }

    /// 按名称查找文件 (glob)
    pub async fn find_by_name(
        &self,
        path: &str,
        pattern: &str,
        max_results: usize,
    ) -> ServiceResult {
        let abs = self.resolve_path(path);
        if !abs.is_dir() {
            return Err(ServiceError::bad_request(format!("不是目录: {path}")));
        }

        let mut results = Vec::new();
        self.walk_glob(&abs, &abs, pattern, max_results, &mut results).await;

        Ok(json!({
            "path": abs.to_string_lossy(),
            "pattern": pattern,
            "files": results,
            "total": results.len(),
        }))
    }

    /// WS handler — 统一分发
    pub async fn handle(&self, action: &str, params: Value) -> ServiceResult {
        match action {
            "read" => {
                let path = req_str(&params, "path")?;
                let encoding = params["encoding"].as_str();
                let start_line = params["start_line"].as_u64().map(|v| v as usize);
                let end_line = params["end_line"].as_u64().map(|v| v as usize);
                self.read_file(&path, start_line, end_line, encoding).await
            }
            "write" => {
                let path = req_str(&params, "path")?;
                let content = req_str(&params, "content")?;
                let append = params["append"].as_bool().unwrap_or(false);
                let encoding = params["encoding"].as_str();
                self.write_file(&path, &content, append, encoding).await
            }
            "append" => {
                let path = req_str(&params, "path")?;
                let content = req_str(&params, "content")?;
                self.append_file(&path, &content).await
            }
            "delete" => {
                let path = req_str(&params, "path")?;
                self.delete_file(&path).await
            }
            "list" => {
                let path = params["path"].as_str().unwrap_or(".");
                let recursive = params["recursive"].as_bool().unwrap_or(false);
                let show_hidden = params["show_hidden"].as_bool().unwrap_or(false);
                let max_depth = params["max_depth"].as_u64().map(|v| v as usize);
                let max_files = params["max_files"].as_u64().map(|v| v as usize);
                self.list_dir(path, recursive, show_hidden, max_depth, max_files).await
            }
            "mkdir" => {
                let path = req_str(&params, "path")?;
                self.mkdir(&path).await
            }
            "move" | "rename" => {
                let src = req_str(&params, "src")?;
                let dst = req_str(&params, "dst")?;
                self.move_file(&src, &dst).await
            }
            "copy" => {
                let src = req_str(&params, "src")?;
                let dst = req_str(&params, "dst")?;
                self.copy_file(&src, &dst).await
            }
            "exists" => {
                let path = req_str(&params, "path")?;
                self.exists(&path).await
            }
            "stat" => {
                let path = req_str(&params, "path")?;
                self.stat(&path).await
            }
            "str_replace" => {
                let path = req_str(&params, "path")?;
                let old_str = req_str(&params, "old_str")?;
                let new_str = req_str(&params, "new_str")?;
                self.str_replace(&path, &old_str, &new_str).await
            }
            "search" | "grep" => {
                let path = req_str(&params, "path")?;
                let pattern = req_str(&params, "pattern")?;
                let max = params["max_results"].as_u64().unwrap_or(100) as usize;
                let include: Option<Vec<String>> = params["include"]
                    .as_array()
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect());
                let case_insensitive = params["case_insensitive"].as_bool().unwrap_or(false);
                let fixed_strings = params["fixed_strings"].as_bool().unwrap_or(false);
                self.grep(&path, &pattern, max, include, case_insensitive, fixed_strings).await
            }
            "find" | "find_by_name" => {
                let path = req_str(&params, "path")?;
                let pattern = req_str(&params, "pattern")?;
                let max = params["max_results"].as_u64().unwrap_or(5000) as usize;
                self.find_by_name(&path, &pattern, max).await
            }
            _ => Err(ServiceError::bad_request(format!("未知 file 操作: {action}"))),
        }
    }

    // ── 内部 ──────────────────────────────────

    fn resolve_path(&self, path: &str) -> PathBuf {
        let p = Path::new(path);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.workspace.join(path)
        }
    }

    fn list_dir_entries<'a>(
        &'a self,
        dir: &'a Path,
        recursive: bool,
        show_hidden: bool,
        depth: usize,
        max_depth: usize,
        max_files: usize,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<Value>, ServiceError>> + Send + 'a>> {
        Box::pin(async move {
            let mut entries = Vec::new();
            let mut read_dir = fs::read_dir(dir)
                .await
                .map_err(|e| ServiceError::internal(format!("读取目录失败: {e}")))?;

            while let Some(entry) = read_dir
                .next_entry()
                .await
                .map_err(|e| ServiceError::internal(format!("目录遍历失败: {e}")))?
            {
                if entries.len() >= max_files { break; }

                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();

                // 跳过隐藏文件
                if !show_hidden && name.starts_with('.') {
                    continue;
                }

                let meta = entry.metadata().await.ok();
                let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);

                let mut e = json!({
                    "name": name,
                    "path": path.to_string_lossy(),
                    "is_dir": is_dir,
                });
                if let Some(m) = &meta {
                    e["size"] = json!(m.len());
                }

                if recursive && is_dir && depth < max_depth {
                    let children = self
                        .list_dir_entries(&path, recursive, show_hidden, depth + 1, max_depth, max_files)
                        .await?;
                    e["children"] = json!(children);
                }

                entries.push(e);
            }

            entries.sort_by(|a, b| {
                let a_dir = a["is_dir"].as_bool().unwrap_or(false);
                let b_dir = b["is_dir"].as_bool().unwrap_or(false);
                b_dir.cmp(&a_dir).then_with(|| {
                    a["name"].as_str().unwrap_or("").cmp(b["name"].as_str().unwrap_or(""))
                })
            });

            Ok(entries)
        })
    }

    /// 递归 glob 匹配
    fn walk_glob<'a>(
        &'a self,
        base: &'a Path,
        dir: &'a Path,
        pattern: &'a str,
        max_results: usize,
        results: &'a mut Vec<Value>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            if results.len() >= max_results { return; }

            let Ok(mut read_dir) = fs::read_dir(dir).await else { return };
            while let Ok(Some(entry)) = read_dir.next_entry().await {
                if results.len() >= max_results { break; }

                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();

                // 跳过隐藏
                if name.starts_with('.') { continue; }

                let is_dir = entry.metadata().await.map(|m| m.is_dir()).unwrap_or(false);

                // glob 匹配文件名
                if glob_match(pattern, &name) {
                    let meta = entry.metadata().await.ok();
                    results.push(json!({
                        "path": path.to_string_lossy(),
                        "name": name,
                        "is_dir": is_dir,
                        "size": meta.as_ref().map(|m| m.len()),
                    }));
                }

                if is_dir {
                    self.walk_glob(base, &path, pattern, max_results, results).await;
                }
            }
        })
    }
}

/// 简单 glob 匹配 (* 和 ?)
fn glob_match(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    glob_match_inner(&p, &n, 0, 0)
}

fn glob_match_inner(p: &[char], n: &[char], pi: usize, ni: usize) -> bool {
    if pi == p.len() && ni == n.len() { return true; }
    if pi == p.len() { return false; }

    if p[pi] == '*' {
        // * 匹配 0 或多个字符
        for skip in ni..=n.len() {
            if glob_match_inner(p, n, pi + 1, skip) { return true; }
        }
        return false;
    }

    if ni == n.len() { return false; }

    if p[pi] == '?' || p[pi] == n[ni] {
        glob_match_inner(p, n, pi + 1, ni + 1)
    } else {
        false
    }
}

fn req_str(params: &Value, key: &str) -> Result<String, ServiceError> {
    params[key]
        .as_str()
        .map(String::from)
        .ok_or_else(|| ServiceError::bad_request(format!("缺少 {key}")))
}
