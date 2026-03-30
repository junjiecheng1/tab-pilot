# TabPilot 功能接通实施计划

> 核心发现：后端 AuditLog / ToolGuard / AuthManager / Connector 都已实现，前端 UI 也已就位。
> **唯一断点是 IPC Server（`pilot/ipc/server.py`）没暴露已有能力，以及前端 bridge 没对接。**

---

## 现有资产盘点

| 模块 | 文件 | 已有能力 | 缺失 |
|------|------|---------|------|
| AuditLog | `security/audit.py` | `init()`, `log()`, `query()` — SQLite 完整 | **没被任何 Executor 调用** |
| ToolGuard | `security/guard.py` | `check()`, `remember()`, `clear_remembered()`, `load_remembered()` — 三模式 + SQLite 持久化 | 缺 `set_mode()`, 缺 `get_remembered()` 列表返回 |
| AuthManager | `connection/auth.py` | `get_token()`, `clear_token()`, `_save_token()` — 文件持久化 | 缺 `save_token()` 公开接口 |
| Connector | `connection/connector.py` | `state`, `run()`, `stop()` | 缺 `uptime` 计算（`time` 已 import 但没用） |
| PilotSettings | `config/settings.py` | `workspace_root`(默认$HOME), `guard_mode`, `data_dir` | 缺运行时修改 + 持久化 |
| IPC Server | `ipc/server.py` | `/status`, `/logs`, `/guard/clear` | `/status` 字段不全, `/logs` 返空, 缺大量端点 |

---

## 假功能完整清单（18 项）

| # | 假功能 | 归属阶段 | 依赖 |
|---|--------|---------|------|
| 1 | uptime 永远 0 | P0 | Connector 加 `_start_time` |
| 2 | todayOps 用 logs.length | P1 | 前端改为从 audit count 获取 |
| 3 | guardMode 硬编码 standard | P0 | `/status` 返回真值 |
| 4 | 最近操作列表永远空 | P0 | `/logs` 对接 AuditLog |
| 5 | 安全模式切换不同步 | P0 | `POST /guard/mode` |
| 6 | 白名单列表永远空 | P0 | `GET /guard/remembered` |
| 7 | 清空白名单看不到效果 | P0 | 管道通了自然可见 |
| 8 | 移除单条不同步 | P0 | `POST /guard/remove` |
| 9 | 锁定路径前端硬编码 | P0 | `GET /guard/protected_paths` |
| 10 | 登录按钮 alert | P1 | 真实 token 流程 |
| 11 | 退出登录假翻转 | P1 | `POST /auth/logout` |
| 12 | 工作空间 prompt() | P2 | Tauri 文件对话框 |
| 13 | 开机自启 Toggle 假 | P2 | tauri-plugin-autostart |
| 14 | 浏览器接管 Toggle 假 | P1 | `/settings/browser` |
| 15 | 审计落库 Toggle 假 | P1 | `/settings/audit` |
| 16 | Endpoint 显示 ws_state | P0 | 改为 `server_url` |
| 17 | Core 版本硬编码 | P0 | `/status` 加 `version` |
| 18 | 托盘状态文字不更新 | P1 | `MenuItem.set_text()` |

**P0 → 8 个变真 | P1 → +5 个变真 | P2 → +2 个变真 | 已完成 3 个配置项 = 18 个全部变真**

---

## Phase 0: 打通数据管道（Python IPC Server）

### 0.1 Connector 加 uptime

```python
# connector.py __init__
self._start_time = time.time()
```

### 0.2 IPC `/status` 补全字段

```python
# 改为返回:
{
    "running": True,
    "connected": connector.state == "connected",
    "ws_state": connector.state.value,
    "uptime": time.time() - connector._start_time,
    "guard_mode": guard._mode,
    "workspace": settings.workspace_root,
    "server_url": settings.server_url,
    "version": f"{VERSION} ({platform.system()}_{platform.machine()})",
    "audit_enabled": True,
    "browser_enabled": True,
}
```

### 0.3 IPC `/logs` 对接 AuditLog

```python
async def _handle_logs(self, request):
    limit = int(request.query.get("limit", "50"))
    rows = await self._audit.query(limit=limit)
    logs = [{
        "timestamp": datetime.fromtimestamp(r["timestamp"]).isoformat(),
        "tool": r["tool_type"],
        "action": r["action"],
        "status": r["guard_decision"] or "allowed",
    } for r in rows]
    return web.json_response({"logs": logs})
```

**IPCServer.__init__ 注入 AuditLog 实例，`__main__.py` 中初始化。**

### 0.4 Executor 写审计

在 `ShellExecutor.execute()` 和 `FileExecutor.execute()` 中：
- 执行前记录 `guard_decision`
- 执行后记录 `result` + `exit_code` + `duration`

### 0.5 新增 IPC 端点

| 端点 | 方法 | 功能 | 对接 |
|------|------|------|------|
| `/guard/mode` | POST | 切换安全模式 | `guard._mode = body["mode"]` |
| `/guard/remembered` | GET | 获取白名单列表 | `list(guard._remembered)` |
| `/guard/remove` | POST | 移除单条白名单 | `guard._remembered.discard(cmd)` + SQLite |
| `/guard/protected_paths` | GET | 获取保护路径 | `guard._protected_paths` |
| `/settings/workspace` | POST | 设置工作空间 | `settings.workspace_root = path` |
| `/settings/browser` | POST | 浏览器开关 | 内存标志位 |
| `/settings/audit` | POST | 审计开关 | 内存标志位 |
| `/auth/token` | POST | 保存 token | `auth._save_token(token)` |
| `/auth/logout` | POST | 清除 token | `auth.clear_token()` |

---

## Phase 1: 前端 Bridge 对接

### 1.1 扩展 Rust Commands (`commands.rs`)

为每个新 IPC 端点添加 Tauri Command，代理到 Python IPC。

### 1.2 扩展 `bridge.ts`

为每个新 Rust Command 添加 TypeScript 函数。

### 1.3 前端页面对接

| 页面 | 改动 |
|------|------|
| DashboardView | `refresh()` 读取新字段: uptime, guard_mode; todayOps 改为从 audit count |
| SecurityView | `onMounted` 读 guard_mode + remembered + protected_paths；切模式 POST |
| LogsView | 已对接，IPC 返真数据即生效 |
| SettingsView | login → POST token; workspace → Tauri对话框; toggles → POST 对应端点; version/endpoint 读 /status |

### 1.4 托盘状态文字更新

把 `status_item` 存到 Tauri 全局 State，监听 `sidecar-status` 后调 `set_text()`。

---

## Phase 2: 首次启动引导 + 系统集成

### 2.1 工作空间选择

- **macOS**: 弹 Tauri `open({ directory: true })` → 用户选择即授权
- **Windows**: 默认 `%LOCALAPPDATA%\TabPilot\workspace`
- **Linux**: 默认 `~/.tabpilot/workspace`
- 未设置时 ToolGuard **拒绝所有写操作**

### 2.2 开机自启

- 安装 `tauri-plugin-autostart`
- 首次启动时**主动申请开机自启权限**
- 用户拒绝 → Toggle 关闭
- 用户同意 → Toggle 开启

### 2.3 Playwright 自动安装

- `BrowserExecutor.__init__` 检测 Playwright chromium 环境
- 不存在 → 自动执行 `playwright install chromium`
- 安装成功 → browser_enabled 默认 ON
- 安装失败 → 降级，Toggle 灰色不可用

---

## 托盘交互设计

### 交互模式

| 操作 | 行为 |
|------|------|
| **左键单击**托盘图标 | 显示/聚焦主窗口 |
| **右键单击**托盘图标 | 弹出右键菜单 |

### 右键菜单结构

```
┌──────────────────────────┐
│  🟢 已连接 · 标准模式     │  ← 只读，实时更新状态+模式
│  ──────────────────────  │
│  安全模式 ▸              │  ← 子菜单
│    ├ ● 保守模式           │
│    ├ ● 标准模式 ✓         │    ← 当前模式打勾
│    └ ● 信任模式           │
│  ☑ 开机启动              │  ← 勾选切换
│  ──────────────────────  │
│  重启 Sidecar            │
│  退出应用                │  ← 真正杀进程
└──────────────────────────┘
```

> 不需要"打开面板"菜单项，因为左键单击已完成此功能。

---

## 窗口行为

| 操作 | 行为 |
|------|------|
| 点窗口红色 X | **隐藏窗口**（最小化到托盘），应用继续后台运行 |
| 左键单击托盘图标 | 显示窗口 + 聚焦 |
| 托盘"退出应用" | 真正终止进程 (`app.exit(0)`) |
| macOS 绿色按钮 | **禁用**（`fullscreen: false, maximizable: false`） 已完成 |

关闭 = 隐藏（Rust `main.rs`）:
```rust
window.on_window_event(|event| {
    if let WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        window.hide();
    }
});
```

---

## 默认值决策

| 设置项 | 默认值 | 理由 |
|--------|--------|------|
| 安全模式 | `standard` | 平衡安全与效率 |
| 工作空间 | `$HOME`（macOS/Linux）/ `%LOCALAPPDATA%\TabPilot`（Windows） | 必须有值，否则阻断写操作 |
| 开机自启 | **ON**（首次启动主动申请，被拒才关） | 作为云端 Agent 代理，应该常驻 |
| 浏览器接管 | **ON**（Playwright 沙盒，自动安装） | 沙盒化无风险，环境异常才降级 |
| 审计落库 | **ON** | 安全基础能力，直接 SQLite |
| 外观 | `system` | 跟随系统 |

---

## 执行估时

```
P0 (管道打通) ≈ 2.5h
  ├── 0.1 Connector._start_time (5min)
  ├── 0.2 IPC /status 补字段 (30min)
  ├── 0.3 IPC /logs 对接 AuditLog (30min)
  ├── 0.4 Executor 写审计 (1h)
  └── 0.5 新增 IPC 端点 (30min)

P1 (前端对接) ≈ 3.5h
  ├── 1.1 Rust Commands 扩展 (1h)
  ├── 1.2 bridge.ts 扩展 (30min)
  ├── 1.3 前端页面对接 (1.5h)
  └── 1.4 托盘扩展 (30min)

P2 (系统集成) ≈ 2.5h
  ├── 2.1 工作空间引导 + Tauri 对话框 (1h)
  ├── 2.2 开机自启插件 (30min)
  └── 2.3 Playwright 自动安装 (1h)

总计 ≈ 8.5h
```
