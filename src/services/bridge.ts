/**
 * Tauri Bridge — 前端 ↔ Rust 通信层
 *
 * Pure Rust 架构: invoke() 直连内存, 零 HTTP 代理
 */

export interface StatusResponse {
  running: boolean;
  connected: boolean;
  ws_state: string;
  server_reachable: boolean;
  uptime: number;
  guard_mode: string;
  workspace: string;
  server_url: string;
  version: string;
  browser_enabled: boolean;
  audit_enabled: boolean;
  user_id: string;
  user_display: string;
  tools_ready: boolean;
  tool_names: string[];
}

export interface LogEntry {
  id: number;
  timestamp: number;
  tool_type: string;
  action: string;
  args_json: string;
  result: string;
  exit_code: number;
  duration: number;
  guard_decision: string;
  status: string;
}

/** 调用 Tauri command */
async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T | null> {
  try {
    const api = await import('@tauri-apps/api/core');
    return await api.invoke<T>(cmd, args);
  } catch {
    return null;
  }
}

// ── 状态 ──

/** 获取 Pilot 状态 (直接读内存) */
export async function getStatus(): Promise<StatusResponse> {
  const result = await invoke<StatusResponse>('get_status');
  return result ?? {
    running: false,
    connected: false,
    ws_state: 'unavailable',
    server_reachable: false,
    uptime: 0,
    // invoke 失败才会走到这里 (如 dev 无 Tauri). 真实值由后端 get_status 返回.
    guard_mode: '',
    workspace: '',
    server_url: '',
    version: 'unknown',
    browser_enabled: true,
    audit_enabled: true,
    user_id: '',
    user_display: '',
    tools_ready: false,
    tool_names: [],
  };
}

/** 获取审计日志 */
export async function getLogs(limit = 50): Promise<LogEntry[]> {
  const result = await invoke<LogEntry[]>('get_logs', { limit });
  return result ?? [];
}

// ── 安全门控 ──

export async function clearGuard(): Promise<void> {
  await invoke('clear_guard');
}

export async function getRemembered(): Promise<string[]> {
  const result = await invoke<string[]>('get_remembered');
  return result ?? [];
}

export async function removeRemembered(prefix: string): Promise<void> {
  await invoke('remove_remembered', { prefix });
}

export async function setGuardMode(mode: string): Promise<void> {
  await invoke('set_guard_mode', { mode });
}

export async function getProtectedPaths(): Promise<string[]> {
  const result = await invoke<string[]>('get_protected_paths');
  return result ?? [];
}

// ── 设置 ──

export async function setWorkspace(path: string): Promise<void> {
  await invoke('set_workspace', { path });
}

export async function setBrowserEnabled(enabled: boolean): Promise<void> {
  await invoke('set_browser_enabled', { enabled });
}

export async function setAuditEnabled(enabled: boolean): Promise<void> {
  await invoke('set_audit_enabled', { enabled });
}

// ── 认证 ──

export async function getAuthChallenge(): Promise<string> {
  const result = await invoke<string>('get_auth_challenge');
  return result ?? '';
}

export async function saveToken(token: string, challenge: string): Promise<void> {
  await invoke('save_token', { token, challenge });
}

export async function logout(): Promise<void> {
  await invoke('logout');
}

// ── Shell 终端 ──

export interface ShellSessionInfo {
  id: string;
  label: string;
  is_main: boolean;
  alive: boolean;
  age_seconds: number;
}

/** 列出活跃 shell 会话 */
export async function listShellSessions(): Promise<ShellSessionInfo[]> {
  const result = await invoke<ShellSessionInfo[]>('list_shell_sessions');
  return result ?? [];
}

/** 读取 shell 会话增量输出 */
export async function readShellOutput(sessionId: string): Promise<string> {
  const result = await invoke<string>('read_shell_output', { sessionId });
  return result ?? '';
}

/** 终止 shell 会话 */
export async function killShellSession(sessionId: string): Promise<void> {
  await invoke('kill_shell_session', { sessionId });
}

/** 手动执行 shell 命令 (测试用) */
export interface ShellExecResult {
  session_id: string;
  command_id: string;
  status: string;
  command_done: boolean;
  timed_out: boolean;
  session_alive: boolean;
  exit_code: number | null;
  output: string;
  active: boolean;
  latest?: boolean;
}

export async function execShellCommand(command: string, timeout = 30): Promise<ShellExecResult | null> {
  return invoke<ShellExecResult>('exec_shell_command', { command, timeout });
}
