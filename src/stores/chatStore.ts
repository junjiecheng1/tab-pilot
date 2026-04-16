/**
 * TabPilot Chat Store · Phase 4.4 reducer 模式
 *
 * 状态来源:
 *   live  events: ref<WireEvent[]>  ← SSE 收到的原始事件 + 本地合成 user_message
 *   live state    = replayEvents(events) (纯函数派生)
 *   liveTurns     = state.turns (computed)
 *   historyTurns  = ref<ChatTurn[]> ← 从 /messages API 拉的历史快照 (reducer 之前)
 *   turns         = computed([...historyTurns, ...liveTurns])
 *
 * 旁路状态 (与 events 解耦, 不进 reducer):
 *   currentSessionId / currentTurnId / lastEventId / isStreaming /
 *   isReplaying / lastError / pendingInbox / recentSessions / historyIndex
 *
 * 行为对照旧版:
 *   - 旧: callbacks.onToolStart/onContentDelta/onBlock 等直接 mutate turn
 *   - 新: callbacks.onAnyEvent 把事件追加到 events 数组, reducer 派生 turns
 *
 * 重连 / 关闭 app 重开后的恢复:
 *   - lastEventId 有值 → 增量 reconnect, events 不清空
 *   - lastEventId 无值 → 完整 reconnect, events 清空让 SSE 重放
 *   - 任务未跑 + session 持久化 → 拉 /messages 历史 → 写 historyTurns
 */

import { defineStore } from 'pinia';
import { computed, ref, watch } from 'vue';
import { usePilotStore } from '../services/pilotStore';
import { usePreferenceStore } from './preferenceStore';
import * as copilot from '../services/copilot/api';
import { deriveHttpBase } from '../services/copilot/api';
import {
  applyEvent,
  applyUserMessage,
  createInitialState,
  makeUserMessageEvent,
  replayEvents,
  type ReducerState,
} from '../services/copilot/eventReducer';
import { extractArgsSummary } from '../services/copilot/events';
import type { WireEvent } from '../services/copilot/sse-parser';
import type {
  ChatTurn,
  SessionSummary,
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

  // ── 持久化 session_id ──
  const SESSION_LS_KEY = 'tabpilot.chat.session_id';
  const initialSessionId =
    typeof localStorage !== 'undefined' ? localStorage.getItem(SESSION_LS_KEY) : null;
  const currentSessionId = ref<string | null>(initialSessionId);
  watch(currentSessionId, (v) => {
    try {
      if (v) localStorage.setItem(SESSION_LS_KEY, v);
      else localStorage.removeItem(SESSION_LS_KEY);
    } catch { /* ignore */ }
  });

  // ── 旁路状态 ──
  const isStreaming = ref(false);
  const isReplaying = ref(false);
  const recentSessions = ref<SessionSummary[]>([]);
  const historyIndex = ref(-1);
  const lastError = ref<string | null>(null);
  const pendingInbox = ref<Array<{ msgId: string; text: string }>>([]);
  const currentTurnId = ref<number>(0);
  const lastEventId = ref<string | null>(null);

  // ── reducer 状态源 ──
  /** SSE 原始事件 + 本地合成 user_message, 按时间顺序 */
  const events = ref<WireEvent[]>([]);
  /** 从 /messages API 拉的历史 (reducer 出现之前的对话) */
  const historyTurns = ref<ChatTurn[]>([]);

  let abortCtrl: AbortController | null = null;

  // ── 派生 ──
  const httpBase = computed(() => deriveHttpBase(pilot.serverUrl));
  /** reducer 派生的 live state — 每次 events 变更都会重算 (Vue computed) */
  const liveState = computed<ReducerState>(() => replayEvents(events.value));
  const liveTurns = computed(() => liveState.value.turns);
  /** 给 UI 的完整 turns: 历史 + live */
  const turns = computed<ChatTurn[]>(() => [
    ...historyTurns.value,
    ...liveTurns.value,
  ]);
  const currentTurn = computed<ChatTurn | null>(() => {
    const arr = turns.value;
    return arr.length ? arr[arr.length - 1] : null;
  });
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

  // ── 内部辅助 ──
  /** 已生成 title 的 session, 避免重复打 */
  const titledSessions = new Set<string>();
  /** 待 onSession 回调时用的首条消息文本, 触发 title 生成 */
  let pendingTitleFirstMessage: string | null = null;

  /** 把一个事件追加到 events; live state 自动重算 */
  function pushEvent(evt: WireEvent) {
    events.value.push(evt);
  }

  /** 在当前 live 流上合成一个事件 (用于 stop / answerAsk 这类本地动作的 UI 即时反馈) */
  function pushSyntheticDone() {
    pushEvent({ type: 'done', data: {}, event_id: 'syn-done-' + genId() });
  }

  /** 重置 live 流 — 用于 newSession / 完整 reconnect */
  function resetLive() {
    events.value = [];
  }

  // ── 回调 ──
  function makeCallbacks() {
    return {
      // Phase 4.4: 唯一的结构化数据入口 — 把事件灌进 reducer
      onAnyEvent: (evt: WireEvent) => {
        // turn_id / session 等元数据事件也会进 reducer 但被忽略 (reducer 旁路)
        // pendingInbox 移除在 onInboxConsumed 里单独处理
        events.value.push(evt);
      },

      onSession: (sid: string) => {
        if (!sid) return;
        currentSessionId.value = sid;
        if (pendingTitleFirstMessage && !titledSessions.has(sid)) {
          const msg = pendingTitleFirstMessage;
          pendingTitleFirstMessage = null;
          titledSessions.add(sid);
          copilot
            .generateSessionTitle(httpBase.value, sid, msg)
            .catch(() => { /* 标题失败不影响主流程 */ });
        }
      },

      onTurnId: (turnId: number, sessionId: string) => {
        currentTurnId.value = turnId;
        if (sessionId && sessionId !== currentSessionId.value) {
          currentSessionId.value = sessionId;
        }
      },

      onLastEventId: (eventId: string) => {
        lastEventId.value = eventId;
      },

      onError: (p: TurnError) => {
        // Phase 5.3: 后端 409 SESSION_BUSY → 自动转 reconnect
        if (p.code === '409' && /SESSION_BUSY|session.*busy|reconnect/i.test(p.message)) {
          // 把刚加进 events 的 error 撤掉 (reducer 还没收尾时不留 error 痕迹)
          // (简单起见: events 里保留, 只是不弹 lastError)
          tryReconnect().catch((e) =>
            console.warn('[chat] auto-reconnect after SESSION_BUSY failed', e),
          );
          return;
        }
        // 其他 error 已经被 reducer 写到 currentTurn.error, 这里再弹一个 toast
        lastError.value = p.message;
      },

      onDone: () => {
        // reducer 已经在收到 'done' 事件时把 turn 收尾, 这里只更新旁路 flag
        isStreaming.value = false;
        isReplaying.value = false;
        abortCtrl = null;
      },

      onInboxConsumed: (msgId: string, _message: string) => {
        // 1) 从前端排队队列里移除
        const i = pendingInbox.value.findIndex((m) => m.msgId === msgId);
        if (i >= 0) pendingInbox.value.splice(i, 1);
        // 2) reducer 已经在 'inbox_consumed' 事件里开了新 turn, 这里不需要额外处理
      },

      onInboxDrained: () => {
        pendingInbox.value.splice(0, pendingInbox.value.length);
      },
    };
  }

  // ── 动作 ──
  async function sendMessage(text: string) {
    const trimmed = text.trim();
    if (!trimmed) return;

    // ── 分支 1: streaming 中 → 走 inbox 队列 ──
    if (isStreaming.value && currentSessionId.value) {
      const entry = {
        msgId: `inbox-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
        text: trimmed,
      };
      pendingInbox.value.push(entry);
      const res = await copilot.pushInbox(
        httpBase.value,
        currentSessionId.value,
        trimmed,
        entry.msgId,
        currentTurnId.value,
      );
      if (!res.ok) {
        const i = pendingInbox.value.findIndex((m) => m.msgId === entry.msgId);
        if (i >= 0) pendingInbox.value.splice(i, 1);
        lastError.value = `排队失败: ${res.error}`;
      }
      return;
    }

    // ── 分支 2: idle → 起新 turn ──
    // 本地立即合成 user_message 事件, reducer 开新 turn (UI 即时反馈)
    pushEvent(makeUserMessageEvent(trimmed));
    historyIndex.value = -1;
    isStreaming.value = true;
    lastEventId.value = null;  // 新 turn 重置事件指针

    // 标题候选
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

  // ── 历史恢复 (对齐 PC AssistantMessage.vue 的渲染逻辑) ──
  /** 把后端 /messages 返回的消息列表转成 ChatTurn[] */
  function restoreToTurns(rawMessages: any[]): ChatTurn[] {
    const result: ChatTurn[] = [];
    let currentTurn: ChatTurn | null = null;

    for (const msg of rawMessages) {
      if (msg.role === 'user') {
        const textContent = msg.contents?.find((c: any) => c.type === 'text');
        const text = typeof textContent?.data === 'string'
          ? textContent.data
          : textContent?.data?.markdown || '';
        currentTurn = emptyTurn(text);
        currentTurn.status = 'done';
        result.push(currentTurn);
      } else if (msg.role === 'assistant') {
        if (!currentTurn) {
          currentTurn = emptyTurn('');
          currentTurn.status = 'done';
          result.push(currentTurn);
        }

        // 1. 错误信息
        const errorData = msg.contents?.find((c: any) => c.type === 'error')?.data;
        if (errorData?.message) {
          currentTurn.status = 'error';
          currentTurn.error = { code: 'HISTORY', message: errorData.message };
        }

        // 2. 主文本 (contents[type=text] → turn.content)
        const textBlock = msg.contents?.find((c: any) => c.type === 'text');
        if (textBlock) {
          const md = typeof textBlock.data === 'string'
            ? textBlock.data
            : textBlock.data?.markdown;
          if (md) {
            currentTurn.content = (currentTurn.content ? currentTurn.content + '\n\n' : '') + md;
          }
        }

        // 3. 时间线步骤 — 对齐 PC AssistantMessage.vue 的 collectIntoSection 逻辑
        const stepsData = msg.contents?.find((c: any) => c.type === 'steps')?.data;
        if (Array.isArray(stepsData)) {
          // 预扫描: 是否包含 message(result) — 有则 narration 跳过 (PC 同逻辑)
          const hasMessageResult = stepsData.some(
            (s: any) => s.type === 'tool_activity' && s.name === 'message' && s.args?.mode === 'result',
          );

          for (const s of stepsData) {
            // ── message 工具分流 (对齐 PC L268-L276) ──
            if (s.type === 'tool_activity' && s.name === 'message') {
              const mode = s.args?.mode;
              if (mode === 'result') {
                // result 模式 → 提取 args.message 作为最终 Markdown 内容
                // 不进时间线 (和 PC 完全一致)
                const msgText = typeof s.args?.message === 'string' ? s.args.message : '';
                if (msgText) {
                  currentTurn.content = (currentTurn.content ? currentTurn.content + '\n\n' : '') + msgText;
                }
                continue;
              }
              // info 模式 → 转成 narration (过程独白)
              const infoText = typeof s.args?.message === 'string' ? s.args.message : '';
              if (infoText.trim()) {
                currentTurn.steps.push({
                  id: genId(),
                  type: 'narration',
                  text: infoText,
                  startedAt: Date.now(),
                });
              }
              continue;
            }

            // ── todo_write 跳过 (PC L257-L263 跳过同理) ──
            if (s.type === 'tool_activity' && s.name === 'todo_write') {
              continue;
            }

            // ── 普通 tool_activity ──
            if (s.type === 'tool_activity') {
              const toolStepId = String(s.id || genId());
              currentTurn.steps.push({
                id: toolStepId,
                type: 'tool',
                callId: toolStepId,
                name: s.name || 'tool',
                displayName: s.display_name,
                status: s.status === 'completed' ? 'done' : s.status === 'failed' ? 'error' : 'cancelled',
                summary: s.summary || s.result?.summary || s.result?.formatted
                  || s.result?.display?.title || s.result?.data?.brief
                  || extractArgsSummary(s.name || '', s.args) || '',
                args: s.args,
                errorMessage: s.error,
                durationMs: s.duration_ms,
                startedAt: Date.now(),
              });

              // ask/confirm 工具: 从 interaction 字段恢复 human_ask block
              // 对齐 PC ToolActivity.vue 的 humanAskData computed
              if (s.interaction && typeof s.interaction === 'object') {
                const ix = s.interaction;
                const questions = Array.isArray(ix.questions) ? ix.questions : [];
                const response = ix.response && typeof ix.response === 'object' ? ix.response : {};
                // 构造和 SSE human_ask 事件一致的 payload
                const askPayload: Record<string, unknown> = {
                  tool_call_id: toolStepId,
                  question: typeof ix.prompt === 'string' ? ix.prompt : questions[0]?.question || '',
                  question_type: questions[0]?.question_type || (ix.kind === 'confirm' ? 'confirm' : 'open'),
                  questions,
                  options: questions[0]?.options,
                  answered: Boolean(ix.answered),
                  answer_preview: (() => {
                    // 提取用户回答的预览文本
                    if (response.answer_value) return String(response.answer_value);
                    if (Array.isArray(response.answer_values)) return (response.answer_values as string[]).join('、');
                    if (Array.isArray(response.answers)) {
                      const first = (response.answers as any[])[0];
                      return first?.answer_value || first?.answer_values?.join('、') || '';
                    }
                    return '';
                  })(),
                };
                currentTurn.blocks.push({
                  id: genId(),
                  blockType: 'human_ask',
                  payload: askPayload,
                  dedupeKey: `human_ask:${toolStepId}`,
                });
              }
              continue;
            }

            // ── narration / content ──
            if (s.type === 'narration' || s.type === 'content') {
              // PC: 如果有 message(result) 则 narration 跳过 (已在 content 展示)
              if (hasMessageResult) continue;
              if (s.text) {
                currentTurn.steps.push({
                  id: genId(),
                  type: 'narration',
                  text: s.text,
                  startedAt: Date.now(),
                });
              }
              continue;
            }

            // ── subagent (递归恢复内部 steps) ──
            if (s.type === 'subagent') {
              const innerSteps: Array<import('../services/copilot/types').ToolStep | import('../services/copilot/types').NarrationStep> = [];
              if (Array.isArray(s.steps)) {
                for (const inner of s.steps) {
                  if (inner.type === 'tool_activity') {
                    // subagent 内部的 message tool 也做同样分流
                    if (inner.name === 'message') {
                      if (inner.args?.mode === 'result') {
                        const t = typeof inner.args?.message === 'string' ? inner.args.message : '';
                        if (t) currentTurn.content = (currentTurn.content ? currentTurn.content + '\n\n' : '') + t;
                      } else {
                        const t = typeof inner.args?.message === 'string' ? inner.args.message : '';
                        if (t.trim()) innerSteps.push({ id: genId(), type: 'narration', text: t, startedAt: Date.now() });
                      }
                      continue;
                    }
                    if (inner.name === 'todo_write') continue;
                    innerSteps.push({
                      id: String(inner.id || genId()),
                      type: 'tool',
                      callId: String(inner.id || genId()),
                      name: inner.name || 'tool',
                      displayName: inner.display_name,
                      status: inner.status === 'completed' ? 'done' : inner.status === 'failed' ? 'error' : 'cancelled',
                      summary: inner.summary || inner.result?.summary || extractArgsSummary(inner.name || '', inner.args) || '',
                      args: inner.args,
                      errorMessage: inner.error,
                      durationMs: inner.duration_ms,
                      startedAt: Date.now(),
                    });
                  } else if (inner.type === 'narration' || inner.type === 'content') {
                    if (inner.text) {
                      innerSteps.push({ id: genId(), type: 'narration', text: inner.text, startedAt: Date.now() });
                    }
                  }
                }
              }
              currentTurn.steps.push({
                id: s.task_id || s.name || genId(),
                type: 'subagent',
                taskId: s.task_id || s.name || genId(),
                name: s.name || 'subagent',
                displayName: s.display_name || s.name,
                description: s.description,
                status: s.status === 'executing' ? 'cancelled' : s.status === 'failed' ? 'error' : 'done',
                durationMs: s.duration_ms,
                errorMessage: s.error,
                steps: innerSteps,
                startedAt: Date.now(),
              });
              continue;
            }

            // ── block ──
            if (s.type === 'block') {
              const b = s.block;
              if (b) {
                const blockType = b.block_type || b.type;
                const payload = b.data || b.payload;
                currentTurn.blocks.push({
                  id: genId(),
                  blockType,
                  payload,
                  dedupeKey: String(b.id || genId()),
                });
              }
            }
          }

          // PC contentSteps fallback: 如果没有 content 也没有 message(result)，
          // 但有 narration 且无 tool → 用最后一个 narration 作为 content
          if (!currentTurn.content) {
            const hasTools = stepsData.some((s: any) => s.type === 'tool_activity' && s.name !== 'message' && s.name !== 'todo_write');
            if (!hasTools) {
              const lastNarration = [...stepsData].reverse().find((s: any) => s.type === 'narration' || s.type === 'content');
              if (lastNarration?.text) {
                currentTurn.content = lastNarration.text;
                // 同时从 steps 里移除这个 narration (避免重复展示)
                const idx = currentTurn.steps.findIndex(
                  (st) => st.type === 'narration' && (st as any).text === lastNarration.text,
                );
                if (idx >= 0) currentTurn.steps.splice(idx, 1);
              }
            }
          }
        }
      }
    }
    return result;
  }

  async function tryReconnect() {
    if (!currentSessionId.value) return;
    if (isStreaming.value) return;
    const status = await copilot.getTaskStatus(
      httpBase.value,
      currentSessionId.value,
    );
    if (!status.running) {
      // 任务未跑 → 拉历史 messages 写 historyTurns; 清 live (没有 in-flight)
      const res = await copilot.getMessages(httpBase.value, currentSessionId.value, 50);
      if (res && res.messages && res.messages.length > 0) {
        // 后端 /messages 返回正序 (旧→新)，直接用，不 reverse
        historyTurns.value = restoreToTurns(res.messages);
        resetLive();
      }
      return;
    }

    // 任务在跑 → reconnect SSE
    // Phase 4.3: 有 lastEventId → 增量重连 (events 不清, 后端只发新事件)
    //           无 lastEventId → 完整重连 (清 events, 让 SSE 全量重放)
    const hasIncremental = !!lastEventId.value;
    if (!hasIncremental) {
      resetLive();
    }
    isStreaming.value = true;
    isReplaying.value = !hasIncremental;

    abortCtrl = await copilot.reconnect(
      httpBase.value,
      currentSessionId.value,
      makeCallbacks(),
      lastEventId.value,
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
    // 合成一个 done 事件让 reducer 收尾当前 turn
    pushSyntheticDone();
  }

  /** 回答 Agent 提问 — 后端发到 /chat/tool-reply, 本地合成事件标记 answered */
  async function answerAsk(
    toolCallId: string,
    answers: Array<{ question_id?: string; answer_value?: string; answer_values?: string[] }>,
    preview: string,
  ): Promise<{ ok: true } | { ok: false; error: string }> {
    const res = await copilot.replyToolCall(
      httpBase.value,
      toolCallId,
      { answers },
      currentTurnId.value,
    );
    if (!res.ok) {
      lastError.value = `回答提交失败: ${res.error}`;
      return { ok: false, error: res.error };
    }
    // 成功: 合成一个 human_ask 事件覆盖原 block 的 payload (标记 answered)
    // dedupeKey 用 'human_ask:tool_call_id', 跟 reducer 的 blockDedupeKey 对齐
    pushEvent({
      type: 'human_ask',
      data: {
        tool_call_id: toolCallId,
        answered: true,
        answer_preview: preview,
      } as Record<string, unknown>,
      event_id: 'answer-' + genId(),
    });
    return { ok: true };
  }

  function detachStream() {
    abortCtrl?.abort();
    abortCtrl = null;
  }

  function newSession() {
    currentSessionId.value = null;
    currentTurnId.value = 0;
    lastEventId.value = null;
    resetLive();
    historyTurns.value = [];
    historyIndex.value = -1;
    pendingInbox.value = [];
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
    // 主状态
    currentSessionId,
    turns,
    currentTurn,
    pastTurns,
    isStreaming,
    isReplaying,
    lastUserMessage,
    hasAnyContent,
    // 旁路
    recentSessions,
    historyIndex,
    lastError,
    clearError: () => { lastError.value = null; },
    pendingInbox,
    httpBase,
    // 内部 (调试用)
    events,
    liveState,
    historyTurns,
    // 动作
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
