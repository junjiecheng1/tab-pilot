// LocalStore — JSON 文件 KV 持久化
//
// 按命名空间隔离, 每个模块一个 JSON 文件

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use log;

/// JSON 文件 KV 存储
pub struct LocalStore {
    dir: PathBuf,
}

impl LocalStore {
    pub fn new(data_dir: &Path) -> Self {
        let dir = data_dir.join("store");
        let _ = std::fs::create_dir_all(&dir);
        Self { dir }
    }

    /// 读取指定命名空间的值
    pub fn get(&self, namespace: &str, key: &str) -> Option<serde_json::Value> {
        let data = self.load(namespace);
        data.get(key).cloned()
    }

    /// 读取指定命名空间的字符串值
    pub fn get_str(&self, namespace: &str, key: &str) -> Option<String> {
        self.get(namespace, key)?.as_str().map(|s| s.to_string())
    }

    /// 写入
    pub fn set(&self, namespace: &str, key: &str, value: serde_json::Value) {
        let mut data = self.load(namespace);
        data.insert(key.to_string(), value);
        self.save(namespace, &data);
    }

    /// 删除
    pub fn delete(&self, namespace: &str, key: &str) {
        let mut data = self.load(namespace);
        data.remove(key);
        self.save(namespace, &data);
    }

    /// 加载整个命名空间
    fn load(&self, namespace: &str) -> HashMap<String, serde_json::Value> {
        let path = self.dir.join(format!("{namespace}.json"));
        if !path.exists() {
            return HashMap::new();
        }
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                serde_json::from_str(&content).unwrap_or_default()
            }
            Err(_) => HashMap::new(),
        }
    }

    /// 保存整个命名空间
    fn save(&self, namespace: &str, data: &HashMap<String, serde_json::Value>) {
        let path = self.dir.join(format!("{namespace}.json"));
        match serde_json::to_string_pretty(data) {
            Ok(content) => {
                if let Err(e) = std::fs::write(&path, content) {
                    log::warn!("[Store:{namespace}] 保存失败: {e}");
                }
            }
            Err(e) => {
                log::warn!("[Store:{namespace}] 序列化失败: {e}");
            }
        }
    }
}
