// 通用会话池 — 预热 + 快速分配
//
// 对应 Python app/services/session_pool.py
// 泛型设计，支持 Shell / Jupyter 等任意会话类型

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};

/// 异步工厂函数签名
pub type BoxFactory<T> =
    Arc<dyn Fn() -> Pin<Box<dyn Future<Output = Result<T, String>> + Send>> + Send + Sync>;

/// 同步清理函数签名
pub type BoxCleanup<T> = Arc<dyn Fn(T) + Send + Sync>;

/// 通用预热会话池
pub struct SessionPool<T: Send + 'static> {
    name: String,
    pool_size: usize,
    factory: BoxFactory<T>,
    cleanup: BoxCleanup<T>,

    pool: Mutex<Vec<T>>,
    available: Arc<Notify>,
    pending_waiters: AtomicUsize,
    initialized: AtomicBool,
    shutdown: AtomicBool,
}

impl<T: Send + 'static> SessionPool<T> {
    pub fn new(
        name: impl Into<String>,
        pool_size: usize,
        factory: BoxFactory<T>,
        cleanup: BoxCleanup<T>,
    ) -> Self {
        Self {
            name: name.into(),
            pool_size,
            factory,
            cleanup,
            pool: Mutex::new(Vec::with_capacity(pool_size)),
            available: Arc::new(Notify::new()),
            pending_waiters: AtomicUsize::new(0),
            initialized: AtomicBool::new(false),
            shutdown: AtomicBool::new(false),
        }
    }

    /// 并行预热所有会话
    pub async fn initialize(&self) {
        if self.initialized.load(Ordering::Relaxed) {
            return;
        }

        log::info!("[{}] 初始化会话池 (size={})", self.name, self.pool_size);

        let mut handles = Vec::with_capacity(self.pool_size);
        for _ in 0..self.pool_size {
            let factory = self.factory.clone();
            handles.push(tokio::spawn(async move { factory().await }));
        }

        let mut pool = self.pool.lock().await;
        for handle in handles {
            match handle.await {
                Ok(Ok(session)) => pool.push(session),
                Ok(Err(e)) => log::warn!("[{}] 预热失败: {}", self.name, e),
                Err(e) => log::warn!("[{}] 预热任务 panic: {}", self.name, e),
            }
        }

        self.initialized.store(true, Ordering::Release);
        if !pool.is_empty() {
            self.available.notify_waiters();
        }
        log::info!(
            "[{}] 会话池就绪, {} 个会话",
            self.name,
            pool.len()
        );
    }

    /// 从池中获取一个会话, 可选等待超时
    pub async fn acquire(&self, wait_timeout_ms: Option<u64>) -> Option<T> {
        if self.shutdown.load(Ordering::Relaxed) {
            return None;
        }

        // 先尝试立即获取
        if let Some(s) = self.try_acquire().await {
            return Some(s);
        }

        // 不等待则返回 None
        let timeout_ms = match wait_timeout_ms {
            Some(ms) if ms > 0 => ms,
            _ => return None,
        };

        self.pending_waiters.fetch_add(1, Ordering::Relaxed);
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(timeout_ms),
            self.wait_and_acquire(),
        )
        .await
        .ok()
        .flatten();
        self.pending_waiters.fetch_sub(1, Ordering::Relaxed);
        result
    }

    async fn try_acquire(&self) -> Option<T> {
        let mut pool = self.pool.lock().await;
        if pool.is_empty() {
            return None;
        }
        let session = pool.remove(0);
        log::debug!(
            "[{}] 获取会话, 剩余 {}",
            self.name,
            pool.len()
        );
        Some(session)
    }

    async fn wait_and_acquire(&self) -> Option<T> {
        loop {
            if self.shutdown.load(Ordering::Relaxed) {
                return None;
            }
            {
                let pool = self.pool.lock().await;
                if !pool.is_empty() {
                    drop(pool);
                    return self.try_acquire().await;
                }
            }
            self.available.notified().await;
        }
    }

    /// 归还/新增会话到池
    pub async fn release(&self, session: T) {
        if self.shutdown.load(Ordering::Relaxed) {
            (self.cleanup)(session);
            return;
        }
        let mut pool = self.pool.lock().await;
        if pool.len() < self.pool_size {
            pool.push(session);
            self.available.notify_one();
        } else {
            drop(pool);
            (self.cleanup)(session);
        }
    }

    /// 关闭池并清理所有会话
    pub async fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Release);
        let mut pool = self.pool.lock().await;
        let count = pool.len();
        for session in pool.drain(..) {
            (self.cleanup)(session);
        }
        log::info!("[{}] 会话池关闭, 清理 {} 个会话", self.name, count);
    }

    /// 同步关闭 (无事件循环时使用)
    pub fn shutdown_sync(&self) -> usize {
        self.shutdown.store(true, Ordering::Release);
        // 使用 try_lock 避免阻塞
        match self.pool.try_lock() {
            Ok(mut pool) => {
                let count = pool.len();
                for session in pool.drain(..) {
                    (self.cleanup)(session);
                }
                count
            }
            Err(_) => 0,
        }
    }

    /// 当前池大小
    pub fn size(&self) -> usize {
        self.pool.try_lock().map(|p| p.len()).unwrap_or(0)
    }

    /// 统计信息
    pub fn stats(&self) -> serde_json::Value {
        serde_json::json!({
            "name": self.name,
            "pool_size": self.pool_size,
            "current_size": self.size(),
            "initialized": self.initialized.load(Ordering::Relaxed),
            "pending_waiters": self.pending_waiters.load(Ordering::Relaxed),
        })
    }
}

/// 会话池集中管理器
pub struct SessionPoolManager {
    pools: Vec<(String, Box<dyn PoolOps>)>,
}

/// 类型擦除的池操作接口
#[async_trait::async_trait]
pub trait PoolOps: Send + Sync {
    async fn initialize(&self);
    async fn shutdown(&self);
    fn stats(&self) -> serde_json::Value;
}

#[async_trait::async_trait]
impl<T: Send + 'static> PoolOps for SessionPool<T> {
    async fn initialize(&self) {
        self.initialize().await;
    }
    async fn shutdown(&self) {
        self.shutdown().await;
    }
    fn stats(&self) -> serde_json::Value {
        self.stats()
    }
}

impl SessionPoolManager {
    pub fn new() -> Self {
        Self { pools: Vec::new() }
    }

    /// 注册一个池
    pub fn register(&mut self, name: impl Into<String>, pool: Box<dyn PoolOps>) {
        let n = name.into();
        log::debug!("注册会话池: {}", n);
        self.pools.push((n, pool));
    }

    /// 并行初始化所有池
    pub async fn initialize_all(&self) {
        if self.pools.is_empty() {
            return;
        }
        log::info!("初始化 {} 个会话池...", self.pools.len());
        let futs: Vec<_> = self.pools.iter().map(|(_, p)| p.initialize()).collect();
        futures_util::future::join_all(futs).await;
        log::info!("所有会话池已初始化");
    }

    /// 并行关闭所有池
    pub async fn shutdown_all(&self) {
        if self.pools.is_empty() {
            return;
        }
        let futs: Vec<_> = self.pools.iter().map(|(_, p)| p.shutdown()).collect();
        futures_util::future::join_all(futs).await;
        log::info!("所有会话池已关闭");
    }

    /// 统计信息
    pub fn stats(&self) -> serde_json::Value {
        let map: serde_json::Map<String, serde_json::Value> = self
            .pools
            .iter()
            .map(|(name, pool)| (name.clone(), pool.stats()))
            .collect();
        serde_json::Value::Object(map)
    }
}
