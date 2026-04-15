/**
 * TabPilot Chat Store
 *
 * 核心数据：`currentTurn` —— 一次 user→assistant 往返的完整状态
 *   ├─ steps[]     时间线（tool + narration）
 *   ├─ content     最终 markdown（content_delta 累积 or message(result) 覆盖）
 *   ├─ blocks[]    独立块区
 *   ├─ status      streaming | done | error
 *   └─ error
 *
 * 状态管理哲学：
 *   - 一次一个 turn；发新消息时覆盖 currentTurn
 *   - reconnect 走全量 replay，UI 层用 isReplaying 遮罩
 *   - 跨 session 切换时 currentTurn 清空（历史由 Web 端打开）
 */

import { defineStore } from 'pinia';
import { computed, ref, watch } from 'vue';
import { usePilotStore } from '../services/pilotStore';
import { usePreferenceStore } from './preferenceStore';
import * as copilot from '../services/copilot/api';
import { deriveHttpBase } from '../services/copilot/api';
import { extractArgsSummary } from '../services/copilot/events';
import type {
  BlockItem,
  ChatTurn,
  NarrationStep,
  ScopeRef,
  SessionSummary,
  SubAgentStep,
  ToolStep,
  TurnError,
} from '../services/copilot/types';

function genId(): string {
  return Math.random().toString(36).slice(2, 10);
}

function emptyTurn(userText: string): ChatTurn {
  return {
    userText,
    steps: [],
    content: '',
    blocks: [],
    status: 'streaming',
    error: null,
    startedAt: Date.now(),
    durationMs: null,
  };
}

export const useChatStore = defineStore('chat', () => {
  const pilot = usePilotStore();
  const pref = usePreferenceStore();

  // ── 状态 ──
  // session_id 在 localStorage 持久化, 刷新/重启后仍能重连原会话
  // (后端如果还在跑同一 session → SSE 续播; 否则继续作为新请求的历史上下文 key)
  const SESSION_LS_KEY = 'tabpilot.chat.session_id';
  const initialSessionId =
    typeof localStorage !== 'undefined' ? localStorage.getItem(SESSION_LS_KEY) : null;
  const currentSessionId = ref<string | null>(initialSessionId);
  // session id 变化时同步到 localStorage
  watch(currentSessionId, (v) => {
    try {
      if (v) localStorage.setItem(SESSION_LS_KEY, v);
      else localStorage.removeItem(SESSION_LS_KEY);
    } catch { /* SSR / 权限拒绝时忽略 */ }
  });

  /** 按时间顺序：turns[0] 最早，turns[last] 最新（= 当前流） */
  const turns = ref<ChatTurn[]>([]);
  const isStreaming = ref(false);
  const isReplaying = ref(false);
  const recentSessions = ref<SessionSummary[]>([]);
  const historyIndex = ref(-1);
  /** 最近一次非致命错误, 供 UI 弹 toast; 读取后应由 UI clearError() 清掉 */
  const lastError = ref<string | null>(null);
  /** 用户在 streaming 期间排队的消息 (后端 inbox 的镜像, 供 UI 显示 pending 徽标)
   *
   * 后端 SSE 会发 `inbox_consumed` 事件时, events.ts 应该把对应 msgId 从这里移除。 */
  const pendingInbox = ref<Array<{ msgId: string; text: string }>>([]);
  /** 当前 session 最新的 turn_id (Phase 2), 由后端 SSE 发 turn_id 事件时更新,
   * 前端在 stop / inbox / tool-reply 请求里带上, 后端拒过期 turn 的请求 */
  const currentTurnId = ref<number>(0);

  let abortCtrl: AbortController | null = null;

  // in-flight 跟踪：只存"存在性"，不缓存对象引用（Vue 响应式要走数组代理）
  const activeToolIds = new Set<string>();
  const seenBlockKeys = new Set<string>();

  /**
   * 根据 scope.taskId 找到事件应该落到的 steps 数组：
   *  - 没有 scope 或找不到 subagent → turn.steps（根）
   *  - 匹配到 subagent → 该 subagent.steps
   */
  function resolveStepsScope(scope?: ScopeRef): Array<ToolStep | NarrationStep | SubAgentStep> | null {
    const t = currentTurn.value;
    if (!t) return null;
    if (!scope?.taskId) return t.steps;
    const sub = t.steps.find(
      (s): s is SubAgentStep => s.type === 'subagent' && s.taskId === scope.taskId,
    );
    if (sub) return sub.steps as Array<ToolStep | NarrationStep | SubAgentStep>;
    return t.steps;
  }

  /** 在指定 steps 数组里按 callId 找 tool index */
  function findToolIndexIn(
    arr: Array<ToolStep | NarrationStep | SubAgentStep>,
    id: string,
  ): number {
    return arr.findIndex((s) => s.type === 'tool' && s.callId === id);
  }

  /** 在当前 turn 根层按 taskId 找 subagent index */
  function findSubagentIndex(taskId: string): number {
    const t = currentTurn.value;
    if (!t) return -1;
    return t.steps.findIndex(
      (s) => s.type === 'subagent' && s.taskId === taskId,
    );
  }

  // ── 派生 ──
  const httpBase = computed(() => deriveHttpBase(pilot.serverUrl));
  /** 最新一轮（流式目标） */
  const currentTurn = computed<ChatTurn | null>(() =>
    turns.value.length ? turns.value[turns.value.length - 1] : null,
  );
  /** 除最新一轮外的历史（按时间顺序正序；渲染时反向） */
  const pastTurns = computed<ChatTurn[]>(() =>
    turns.value.length > 1 ? turns.value.slice(0, -1) : [],
  );
  const lastUserMessage = computed(() => currentTurn.value?.userText ?? '');
  const hasAnyContent = computed(
    () =>
      !!currentTurn.value &&
      (currentTurn.value.steps.length > 0 ||
        currentTurn.value.blocks.length > 0 ||
        currentTurn.value.content.length > 0 ||
        !!currentTurn.value.error),
  );

  function resetInflight() {
    activeToolIds.clear();
    seenBlockKeys.clear();
  }

  /** reconnect 时从当前 turn 重建 in-flight 集，防止事件重放导致步骤重复 */
  function rebuildInflightFromTurn(t: ChatTurn) {
    activeToolIds.clear();
    seenBlockKeys.clear();
    const walk = (arr: Array<ToolStep | NarrationStep | SubAgentStep>) => {
      for (const s of arr) {
        if (s.type === 'tool' && s.status === 'running') {
          activeToolIds.add(s.callId);
        } else if (s.type === 'subagent') {
          walk(s.steps as Array<ToolStep | NarrationStep | SubAgentStep>);
        }
      }
    };
    walk(t.steps);
    for (const b of t.blocks) {
      seenBlockKeys.add(b.dedupeKey);
    }
  }

  // ── 回调（绑到 currentTurn） ──
  function makeCallbacks() {
    const turn = () => currentTurn.value;

    return {
      onSession: (sid: string) => {
        if (!sid) return;
        currentSessionId.value = sid;
        // 若当前有"待生成 title 的首条消息", 触发一次后端 LLM 标题生成
        if (pendingTitleFirstMessage && !titledSessions.has(sid)) {
          const msg = pendingTitleFirstMessage;
          pendingTitleFirstMessage = null;
          titledSessions.add(sid);
          // 后台 fire-and-forget, 不阻塞 streaming
          copilot
            .generateSessionTitle(httpBase.value, sid, msg)
            .catch(() => { /* 标题失败不影响主流程 */ });
        }
      },

      onNarration: (text: string, scope?: ScopeRef) => {
        const t = turn();
        if (!t || !text.trim()) return;
        const arr = resolveStepsScope(scope);
        if (!arr) return;
        arr.push({
          id: genId(),
          type: 'narration',
          text,
          startedAt: Date.now(),
        });
      },

      onContentDelta: (_delta: string, full: string) => {
        const t = turn();
        if (!t) return;
        t.content = full;
      },

      onContentFull: (full: string) => {
        const t = turn();
        if (!t) return;
        t.content = full;
      },

      onToolStart: (p: {
        id: string;
        name: string;
        displayName?: string;
        args?: Record<string, unknown>;
        scope?: ScopeRef;
      }) => {
        const t = turn();
        if (!t) return;
        const arr = resolveStepsScope(p.scope);
        if (!arr) return;
        if (findToolIndexIn(arr, p.id) >= 0) return; // 幂等
        arr.push({
          id: p.id,
          type: 'tool',
          callId: p.id,
          name: p.name,
          displayName: p.displayName,
          status: 'running',
          summary: extractArgsSummary(p.name, p.args),
          args: p.args,
          startedAt: Date.now(),
        });
        activeToolIds.add(p.id);
      },

      onToolProgress: (p: { id: string; summary: string; scope?: ScopeRef }) => {
        const arr = resolveStepsScope(p.scope);
        if (!arr) return;
        const i = findToolIndexIn(arr, p.id);
        if (i < 0) return;
        const prev = arr[i] as ToolStep;
        arr.splice(i, 1, { ...prev, summary: p.summary });
      },

      onToolResult: (p: {
        id: string;
        name: string;
        success: boolean;
        summary?: string;
        errorMessage?: string;
        durationMs?: number;
        args?: Record<string, unknown>;
        scope?: ScopeRef;
      }) => {
        const arr = resolveStepsScope(p.scope);
        if (!arr) return;
        let i = findToolIndexIn(arr, p.id);
        if (i < 0) {
          // 容错：tool_result 早于 tool_call_start（Gemini 乱序）
          arr.push({
            id: p.id,
            type: 'tool',
            callId: p.id,
            name: p.name,
            status: 'running',
            args: p.args,
            startedAt: Date.now(),
          });
          i = arr.length - 1;
        }
        const prev = arr[i] as ToolStep;
        arr.splice(i, 1, {
          ...prev,
          status: p.success ? 'done' : 'error',
          summary:
            p.summary || prev.summary || extractArgsSummary(p.name, p.args),
          errorMessage: p.errorMessage,
          durationMs: p.durationMs,
        });
        activeToolIds.delete(p.id);
      },

      onSubagentStart: (p: {
        taskId: string;
        name: string;
        displayName?: string;
        description?: string;
      }) => {
        const t = turn();
        if (!t) return;
        if (findSubagentIndex(p.taskId) >= 0) return; // 幂等
        t.steps.push({
          id: p.taskId,
          type: 'subagent',
          taskId: p.taskId,
          name: p.name,
          displayName: p.displayName || p.name,
          description: p.description,
          status: 'running',
          steps: [],
          startedAt: Date.now(),
        });
      },

      onSubagentEnd: (p: {
        taskId: string;
        success: boolean;
        durationMs?: number;
        errorMessage?: string;
      }) => {
        const t = turn();
        if (!t) return;
        const i = findSubagentIndex(p.taskId);
        if (i < 0) return;
        const prev = t.steps[i] as SubAgentStep;
        t.steps.splice(i, 1, {
          ...prev,
          status: p.success ? 'done' : 'error',
          durationMs: p.durationMs,
          errorMessage: p.errorMessage,
        });
      },

      onBlock: (p: { blockType: string; payload: unknown; dedupeKey?: string }) => {
        const t = turn();
        if (!t) return;
        const key = p.dedupeKey || `${p.blockType}:${genId()}`;
        if (seenBlockKeys.has(key)) {
          // 同 key 已存在 → 覆盖 payload
          const i = t.blocks.findIndex((b) => b.dedupeKey === key);
          if (i >= 0) {
            t.blocks.splice(i, 1, { ...t.blocks[i], payload: p.payload });
          }
          return;
        }
        seenBlockKeys.add(key);
        t.blocks.push({
          id: genId(),
          blockType: p.blockType,
          payload: p.payload,
          dedupeKey: key,
        });
      },

      onError: (p: TurnError) => {
        const t = turn();
        if (!t) return;
        t.error = p;
        t.status = 'error';
      },

      onDone: () => {
        const t = turn();
        if (t) {
          if (t.status !== 'error') t.status = 'done';
          t.durationMs = Date.now() - t.startedAt;
          // 递归把所有 running 的 tool / subagent 改为 cancelled
          const cancelIn = (arr: Array<ToolStep | NarrationStep | SubAgentStep>) => {
            for (let i = 0; i < arr.length; i++) {
              const s = arr[i];
              if (s.type === 'tool' && s.status === 'running') {
                arr.splice(i, 1, { ...s, status: 'cancelled' });
              } else if (s.type === 'subagent') {
                if (s.status === 'running') {
                  arr.splice(i, 1, { ...s, status: 'done' });
                }
                cancelIn(s.steps as Array<ToolStep | NarrationStep | SubAgentStep>);
              }
            }
          };
          cancelIn(t.steps);
        }
        isStreaming.value = false;
        isReplaying.value = false;
        abortCtrl = null;
        resetInflight();
      },

      onUnknown: (type: string, data: unknown) => {
        console.warn('[TabPilot] 未知 SSE 事件:', type, data);
      },

      onTurnId: (turnId: number, sessionId: string) => {
        currentTurnId.value = turnId;
        if (sessionId && sessionId !== currentSessionId.value) {
          currentSessionId.value = sessionId;
        }
      },

      onInboxConsumed: (msgId: string, message: string) => {
        // 1) 从前端排队队列里移除
        const i = pendingInbox.value.findIndex((m) => m.msgId === msgId);
        if (i >= 0) pendingInbox.value.splice(i, 1);

        // 2) 当前 turn 封口为 done (固化已有 assistant 输出)
        const cur = currentTurn.value;
        if (cur && cur.status === 'streaming') {
          cur.status = 'done';
          cur.durationMs = Date.now() - cur.startedAt;
        }

        // 3) 用消费的文本开新 turn, in-flight 状态清空, 后续 SSE 事件归属新 turn
        resetInflight();
        turns.value.push(emptyTurn(message || '(继续)'));
        // isStreaming 保持 true (后端在同一 SSE 流里继续输出)
      },
      onInboxDrained: () => {
        pendingInbox.value.splice(0, pendingInbox.value.length);
      },
    };
  }

  // ── 动作 ──
  // 已经触发过 title 生成的 session, 避免每次发消息重复打
  const titledSessions = new Set<string>();
  /** 待 onSession 回调后用于生成 title 的首条消息文本 */
  let pendingTitleFirstMessage: string | null = null;

  async function sendMessage(text: string) {
    const trimmed = text.trim();
    if (!trimmed) return;

    // ── 分支 1: 正在 streaming → 走 inbox 队列, 由后端在合适时机吞入 ──
    // 前提: session_id 已知 (刚开始的第一条消息 session_id 可能还没回来, 此时回退 idle 路径)
    if (isStreaming.value && currentSessionId.value) {
      pendingInbox.value.push({
        msgId: `inbox-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
        text: trimmed,
      });
      // 发起请求 (失败时从 pendingInbox 移除)
      const last = pendingInbox.value[pendingInbox.value.length - 1];
      const res = await copilot.pushInbox(
        httpBase.value,
        currentSessionId.value,
        trimmed,
        last.msgId,
        currentTurnId.value,
      );
      if (!res.ok) {
        const i = pendingInbox.value.findIndex((m) => m.msgId === last.msgId);
        if (i >= 0) pendingInbox.value.splice(i, 1);
        lastError.value = `排队失败: ${res.error}`;
      }
      return;
    }

    // ── 分支 2: 正常路径 ──
    // 把上一轮（如果还在 streaming 状态）收尾成 done
    const lastTurn = currentTurn.value;
    if (lastTurn && lastTurn.status === 'streaming') {
      lastTurn.status = 'done';
    }

    resetInflight();
    turns.value.push(emptyTurn(trimmed));
    historyIndex.value = -1;
    isStreaming.value = true;

    // 判定是否是"首条消息" → 标题生成候选
    // 条件: 当前无 session_id (后端会新建), 或当前 session 还没生成过 title
    const shouldTryTitle =
      !currentSessionId.value || !titledSessions.has(currentSessionId.value);
    if (shouldTryTitle) {
      pendingTitleFirstMessage = trimmed;
    }

    abortCtrl = await copilot.chat(
      httpBase.value,
      currentSessionId.value,
      trimmed,
      makeCallbacks(),
      {
        mode: 'agent',
        provider: pref.activeProviderId || undefined,
      },
    );
  }

  async function tryReconnect() {
    if (!currentSessionId.value) return;
    if (isStreaming.value) return;
    const status = await copilot.getTaskStatus(
      httpBase.value,
      currentSessionId.value,
    );
    if (!status.running) return;

    // 若当前最新 turn 存在 → 重置其状态让 replay 从零重建；否则新建占位
    let t = currentTurn.value;
    if (!t) {
      turns.value.push(emptyTurn('(恢复中的会话)'));
      t = currentTurn.value!;
    }
    // 关键：清空 turn 的 steps/content/blocks/error，避免 replay 时
    // content_delta 从 0 累积覆盖已有内容造成长→短→长的跳跃
    t.steps.splice(0, t.steps.length);
    t.content = '';
    t.blocks.splice(0, t.blocks.length);
    t.error = null;
    t.status = 'streaming';
    resetInflight();
    isStreaming.value = true;
    isReplaying.value = true;

    abortCtrl = await copilot.reconnect(
      httpBase.value,
      currentSessionId.value,
      makeCallbacks(),
    );
  }

  async function stop() {
    if (currentSessionId.value) {
      await copilot.stopChat(httpBase.value, currentSessionId.value, currentTurnId.value);
    }
    abortCtrl?.abort();
    abortCtrl = null;
    isStreaming.value = false;
    isReplaying.value = false;
    const t = currentTurn.value;
    if (t && t.status === 'streaming') {
      t.status = 'done';
      const running = t.steps
        .map((s, i) => ({ s, i }))
        .filter(({ s }) => s.type === 'tool' && s.status === 'running');
      for (const { s, i } of running) {
        t.steps.splice(i, 1, { ...(s as ToolStep), status: 'cancelled' });
      }
    }
    resetInflight();
  }

  /** 回答 Agent 提问 — 发给后端同时把本地 block 标记为 answered */
  async function answerAsk(
    toolCallId: string,
    answers: Array<{ question_id?: string; answer_value?: string; answer_values?: string[] }>,
    preview: string,
  ): Promise<{ ok: true } | { ok: false; error: string }> {
    // Phase 3: 优先走新通用端点 /chat/tool-reply, 失败再退回旧 /human-ask/answer 作兼容
    // 包装 answers 成 ToolReplyRequest.result 格式
    const replyRes = await copilot.replyToolCall(
      httpBase.value,
      toolCallId,
      { answers },
      currentTurnId.value,
    );
    const res = replyRes.ok
      ? { ok: true as const }
      : await copilot.answerHumanAsk(httpBase.value, toolCallId, answers);
    if (!res.ok) {
      // 失败时弹 toast, 不标记 answered, 让用户可以重试
      lastError.value = `回答提交失败: ${res.error}`;
      return { ok: false, error: res.error };
    }
    // 成功: 本地标记对应 ask block 为 answered
    const t = currentTurn.value;
    if (!t) return { ok: true };
    const i = t.blocks.findIndex((b) => b.dedupeKey === `human_ask:${toolCallId}`);
    if (i < 0) return { ok: true };
    const prev = t.blocks[i];
    const prevPayload = (prev.payload || {}) as Record<string, unknown>;
    t.blocks.splice(i, 1, {
      ...prev,
      payload: { ...prevPayload, answered: true, answer_preview: preview },
    });
    return { ok: true };
  }

  /** Esc/hide：仅断开 SSE，Agent 继续后台跑 */
  function detachStream() {
    abortCtrl?.abort();
    abortCtrl = null;
  }

  function newSession() {
    currentSessionId.value = null;
    turns.value = [];
    historyIndex.value = -1;
    resetInflight();
  }

  // ── 历史穿梭 ──
  async function ensureRecentSessions() {
    if (recentSessions.value.length > 0) return;
    recentSessions.value = await copilot.getSessions(httpBase.value, 20);
  }

  async function historyUp(): Promise<SessionSummary | null> {
    await ensureRecentSessions();
    if (!recentSessions.value.length) return null;
    historyIndex.value = Math.min(
      historyIndex.value + 1,
      recentSessions.value.length - 1,
    );
    return recentSessions.value[historyIndex.value] || null;
  }

  async function historyDown(): Promise<SessionSummary | null> {
    if (historyIndex.value <= -1) return null;
    historyIndex.value -= 1;
    if (historyIndex.value < 0) return null;
    return recentSessions.value[historyIndex.value] || null;
  }

  function resetHistory() {
    historyIndex.value = -1;
  }

  return {
    currentSessionId,
    turns,
    currentTurn,
    pastTurns,
    isStreaming,
    isReplaying,
    lastUserMessage,
    hasAnyContent,
    recentSessions,
    historyIndex,
    lastError,
    clearError: () => { lastError.value = null; },
    pendingInbox,
    httpBase,
    sendMessage,
    tryReconnect,
    stop,
    answerAsk,
    detachStream,
    newSession,
    historyUp,
    historyDown,
    resetHistory,
    ensureRecentSessions,
  };
});
