# TabPilot 桌面端

## 简介

TabPilot 是 TabApp 的桌面端代理，运行在用户电脑上，提供：
- **Shell 命令执行**：Agent 可在用户电脑执行终端命令
- **文件读写**：Agent 可读写用户电脑上的文件
- **浏览器控制**（Phase 3）：Agent 可操控用户浏览器
- **MCP 本地运行**（Phase 4）：运行本地 MCP Server

## 架构

```
TabPilot
├── Tauri 壳 (src-tauri/)     # 系统托盘、窗口管理
├── Vue 管理面板 (src/)       # 状态查看、设置
└── Python Sidecar (pilot/)   # 核心逻辑
    ├── connection/           # WebSocket 连接层
    ├── executors/            # 命令执行层
    ├── security/             # ToolGuard 安全层
    └── mcp/                  # MCP 管理
```

## 快速开始

```bash
cd tabpilot

# Python 环境
python3 -m venv venv
source venv/bin/activate
pip install -r requirements.txt

# 启动 sidecar (开发模式)
python -m pilot --dev
```
