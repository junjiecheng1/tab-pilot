/**
 * Copilot HTTP/SSE API 客户端
 *
 * 从 record-view 硬 copy 后裁剪：
 * - 只保留 chat / reconnect / stop / getTaskStatus / getMessages / getSessions
 * - 鉴权换成 TabPilot 的 Pilot Token（通过 Tauri invoke 读取）
 * - baseUrl 从 pilotStore.serverUrl 派生
 */

import { consumeSSE, type WireEvent } from './sse-parser';
import { dispatchEvent, makeDispatchState } from './events';
import type { CopilotCallbacks, SessionSummary } from './types';

/** 从 Rust 侧读当前 Pilot Token（若无授权则返回空串） */
async function getPilotToken(): Promise<string> {
  try {
    const api = await import('@tauri-apps/api/core');
    return (await api.invoke<string>('get_pilot_token')) || '';
  } catch {
    return '';
  }
}

/** 把 pilotStore.serverUrl（ws://host/ws/...）转成 http base */
export function deriveHttpBase(serverUrl: string): string {
  return serverUrl
    .replace(/^wss:\/\//, 'https://')
    .replace(/^ws:\/\//, 'http://')
    .replace(/\/ws\/.*$/, '');
}

async function authHeaders(): Promise<Record<string, string>> {
  const token = await getPilotToken();
  const h: Record<string, string> = { 'Content-Type': 'application/json' };
  if (token) h['Authorization'] = `Bearer ${token}`;
  return h;
}

async function httpJson<T>(
  base: string,
  path: string,
  init?: RequestInit,
): Promise<T> {
  const headers = { ...(await authHeaders()), ...(init?.headers as Record<string, string> | undefined) };
  const res = await fetch(`${base}/api${path}`, { ...init, headers });
  if (!res.ok) {
    let msg = `HTTP ${res.status}`;
    try {
      const body = await res.json();
      msg = (body as any)?.error?.message || (body as any)?.detail || msg;
    } catch {
      /* ignore */
    }
    const err = new Error(msg) as Error & { status?: number };
    err.status = res.status;
    throw err;
  }
  return (await res.json()) as T;
}

/** 启动/续接一个 SSE 流；返回 AbortController 用于终止连接 */
async function startStream(
  base: string,
  path: string,
  body: unknown,
  callbacks: CopilotCallbacks,
): Promise<AbortController> {
  const ctrl = new AbortController();
  const state = makeDispatchState();

  (async () => {
    try {
      const res = await fetch(`${base}/api${path}`, {
        method: 'POST',
        headers: await authHeaders(),
        body: JSON.stringify(body),
        signal: ctrl.signal,
      });
      if (!res.ok) {
        let msg = `HTTP ${res.status}`;
        try {
          const j = await res.json();
          msg = (j as any)?.error?.message || (j as any)?.detail || msg;
        } catch {
          /* ignore */
        }
        callbacks.onError?.({ code: String(res.status), message: msg });
        callbacks.onDone?.();
        return;
      }
      await consumeSSE(res, {
        onEvent: (evt: WireEvent) => dispatchEvent(evt, callbacks, state),
        onDone: () => callbacks.onDone?.(),
        onParseError: (line, e) =>
          console.warn('[Copilot] SSE parse error', line, e),
      });
    } catch (err) {
      if ((err as Error)?.name === 'AbortError') {
        callbacks.onDone?.();
        return;
      }
      callbacks.onError?.({
        code: 'NETWORK_ERROR',
        message: (err as Error)?.message || '网络错误',
      });
      callbacks.onDone?.();
    }
  })();

  return ctrl;
}

/** /copilot/chat — 新消息（session_id=null → Lazy 创建） */
export async function chat(
  base: string,
  sessionId: string | null,
  message: string,
  callbacks: CopilotCallbacks,
  opts?: { mode?: 'agent' | 'ask'; provider?: string; skillId?: string },
): Promise<AbortController> {
  const body: Record<string, unknown> = {
    session_id: sessionId,
    message,
    mode: opts?.mode || 'agent',
    attachments: [],
    embedded_resources: [],
  };
  if (opts?.provider) body.provider = opts.provider;
  if (opts?.skillId) body.skill_id = opts.skillId;
  return startStream(base, '/copilot/chat', body, callbacks);
}

/** /copilot/chat/reconnect — 断线后全量 replay + follow */
export async function reconnect(
  base: string,
  sessionId: string,
  callbacks: CopilotCallbacks,
): Promise<AbortController> {
  return startStream(
    base,
    '/copilot/chat/reconnect',
    { session_id: sessionId },
    callbacks,
  );
}

/** 查询 Agent 是否还在跑（决定 reconnect vs getMessages） */
export async function getTaskStatus(
  base: string,
  sessionId: string,
): Promise<{ running: boolean; task_id: string | null; event_count: number }> {
  try {
    const res = await httpJson<{
      running: boolean;
      task_id: string | null;
      event_count: number;
    }>(base, `/copilot/chat/task-status/${encodeURIComponent(sessionId)}`);
    return res;
  } catch {
    return { running: false, task_id: null, event_count: 0 };
  }
}

/** /copilot/human-ask/answer — 回答 Agent 的提问
 *
 * 返回 { ok: true } 成功, { ok: false, error } 失败。
 * 调用方负责 toast / 重试, 避免静默吞错。
 */
export async function answerHumanAsk(
  base: string,
  toolCallId: string,
  answers: Array<{ question_id?: string; answer_value?: string; answer_values?: string[] }>,
): Promise<{ ok: true } | { ok: false; status: number; error: string }> {
  try {
    const res = await fetch(`${base}/api/copilot/human-ask/answer`, {
      method: 'POST',
      headers: await authHeaders(),
      body: JSON.stringify({ tool_call_id: toolCallId, answers }),
    });
    if (!res.ok) {
      // 尽量解析出 pydantic validation 细节
      let detail = `HTTP ${res.status}`;
      try {
        const body = await res.json();
        if (body?.error?.message) detail = String(body.error.message);
        else if (body?.detail) detail = JSON.stringify(body.detail);
      } catch { /* ignore */ }
      console.warn('[Copilot] answerHumanAsk failed', res.status, detail);
      return { ok: false, status: res.status, error: detail };
    }
    return { ok: true };
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    console.warn('[Copilot] answerHumanAsk network error', e);
    return { ok: false, status: 0, error: msg };
  }
}

/** /copilot/chat/inbox — 流式执行中追加一条新消息进队列, 后端会在合适时机吞入
 *
 * turnId 可选, 传了则后端会校验是否过期 (Phase 2)
 * 返回 { ok: true, pending } 或 { ok: false, error }
 */
export async function pushInbox(
  base: string,
  sessionId: string,
  message: string,
  msgId: string,
  turnId?: number,
): Promise<{ ok: true; pending: number } | { ok: false; error: string }> {
  try {
    const body: Record<string, unknown> = { session_id: sessionId, message, msg_id: msgId };
    if (turnId && turnId > 0) body.turn_id = turnId;
    const res = await fetch(`${base}/api/copilot/chat/inbox`, {
      method: 'POST',
      headers: await authHeaders(),
      body: JSON.stringify(body),
    });
    if (!res.ok) {
      let err = `HTTP ${res.status}`;
      try {
        const b = await res.json();
        err = b?.error?.message || err;
      } catch { /* ignore */ }
      return { ok: false, error: err };
    }
    const j = await res.json();
    return { ok: true, pending: Number(j?.data?.pending ?? 0) };
  } catch (e) {
    return { ok: false, error: e instanceof Error ? e.message : String(e) };
  }
}

/** /api/sessions/{sid}/title — 基于首条消息生成会话标题 (LLM)
 *
 * 后端只对 title 为空/默认值的 session 生成, 幂等。成功返回生成后的 title。
 * 调用失败不影响主流程, 安静失败即可。
 */
export async function generateSessionTitle(
  base: string,
  sessionId: string,
  firstMessage: string,
): Promise<string | null> {
  try {
    const res = await fetch(`${base}/api/sessions/${encodeURIComponent(sessionId)}/title`, {
      method: 'POST',
      headers: await authHeaders(),
      body: JSON.stringify({ message: firstMessage }),
    });
    if (!res.ok) return null;
    const j = await res.json();
    const t = j?.data?.title;
    return typeof t === 'string' && t ? t : null;
  } catch (e) {
    console.warn('[Copilot] generateSessionTitle failed', e);
    return null;
  }
}

/** /copilot/chat/stop — 真正终止 Agent 执行 */
export async function stopChat(
  base: string,
  sessionId: string,
  turnId?: number,
): Promise<void> {
  try {
    const body: Record<string, unknown> = { session_id: sessionId };
    if (turnId && turnId > 0) body.turn_id = turnId;
    await fetch(`${base}/api/copilot/chat/stop`, {
      method: 'POST',
      headers: await authHeaders(),
      body: JSON.stringify(body),
    });
  } catch (e) {
    console.warn('[Copilot] stopChat failed', e);
  }
}

/** /copilot/chat/tool-reply — 通用工具应答 (Phase 3)
 *
 * 替代 /copilot/human-ask/answer (后者保留作 alias)
 * 任何等待用户输入的 tool (ask / staged_confirm / 未来的 file_picker) 都走这里
 */
export async function replyToolCall(
  base: string,
  toolCallId: string,
  result: Record<string, unknown>,
  turnId?: number,
): Promise<{ ok: true } | { ok: false; status: number; error: string }> {
  try {
    const body: Record<string, unknown> = { tool_call_id: toolCallId, result };
    if (turnId && turnId > 0) body.turn_id = turnId;
    const res = await fetch(`${base}/api/copilot/chat/tool-reply`, {
      method: 'POST',
      headers: await authHeaders(),
      body: JSON.stringify(body),
    });
    if (!res.ok) {
      let detail = `HTTP ${res.status}`;
      try {
        const b = await res.json();
        detail = b?.error?.message || detail;
      } catch { /* ignore */ }
      return { ok: false, status: res.status, error: detail };
    }
    return { ok: true };
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    return { ok: false, status: 0, error: msg };
  }
}

/** /copilot/sessions — 获取最近对话（for ↑↓ 历史穿梭） */
export async function getSessions(
  base: string,
  limit = 20,
): Promise<SessionSummary[]> {
  try {
    const qs = new URLSearchParams({ limit: String(limit) });
    const res = await fetch(`${base}/api/copilot/sessions?${qs.toString()}`, {
      headers: await authHeaders(),
    });
    if (!res.ok) return [];
    const j = await res.json();
    const rows = (j?.data || []) as Array<Record<string, unknown>>;
    return rows.map((r) => ({
      id: String(r.id ?? ''),
      title: String(r.title ?? ''),
      message_count: Number(r.message_count ?? 0),
      updated_at: r.updated_at as string | undefined,
      last_user_message: (r.last_user_message as string | undefined) ?? '',
    }));
  } catch {
    return [];
  }
}
