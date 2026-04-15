/**
 * Phase 4.4/4.7 · Event-sourcing reducer (skeleton)
 *
 * 把 chatStore 的"逐事件 mutate state"模式改成"events: WireEvent[] →
 * state = reduce(events)" 的纯函数模式。
 *
 * 这是 reducer 的雏形, 暂时不替换 chatStore 的现行手动 mutation 路径,
 * 而是作为 alternate read path 与之并存 + 验证。下一个 session 把
 * chatStore 改为完全基于 reducer 后, 删除手动 mutation 路径。
 *
 * 设计原则:
 *   1. 纯函数: applyEvent(state, event) → newState, 不修改 state
 *   2. UUID 去重: 同 event_id 重复出现只生效一次 (Phase 4.1)
 *   3. 事件 → ChatTurn[] 完全可重放, 用于 reconnect / hydrate / 单测
 *   4. 不维护单例; chatStore 持有 events[] 数组并在变化时重算 state
 *
 * 当前覆盖的 wire 事件 (与 events.ts dispatch 对齐):
 *   - session / turn_id           会话/轮次元数据
 *   - tool_call_start / _result   工具步骤
 *   - human_ask / staged_confirm  block
 *   - inbox_consumed              turn 边界
 *   - content_delta / stream_text 增量内容
 *   - error / done                turn 收尾
 *
 * 不收口的事件 (走 onUnknown / 丢弃):
 *   - thinking_*, plan_*, phase_*, provider_status 等
 */

import type { WireEvent } from './sse-parser';
import type { ChatTurn, BlockItem, ToolStep, NarrationStep } from './types';

// ════════════════════════════════════════════════════════════════════
// State
// ════════════════════════════════════════════════════════════════════

export interface ReducerState {
  /** 完整对话, 按时间顺序; 每条新 user_message / inbox_consumed 开新 turn */
  turns: ChatTurn[];
  /** 当前最新 turn_id (从 turn_id 事件取) */
  turnId: number;
  /** 当前 session_id */
  sessionId: string;
  /** 已处理过的 event_id, 用于去重 (UUID set) */
  seenEventIds: Set<string>;
  /** 已处理过的 block dedupe key (例如 'human_ask:tool_call_id') */
  seenBlockKeys: Set<string>;
}

export function createInitialState(): ReducerState {
  return {
    turns: [],
    turnId: 0,
    sessionId: '',
    seenEventIds: new Set(),
    seenBlockKeys: new Set(),
  };
}

// ════════════════════════════════════════════════════════════════════
// Apply (pure)
// ════════════════════════════════════════════════════════════════════

/** 创建一个空 turn 占位, userText 来自 inbox/user message */
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

function genId(): string {
  return Math.random().toString(36).slice(2, 10) + Date.now().toString(36);
}

/** 取数据字段的 helper */
function asObj(v: unknown): Record<string, unknown> {
  return v && typeof v === 'object' ? (v as Record<string, unknown>) : {};
}

/** 主入口: 把单个事件应用到 state, 返回新 state (浅拷贝, 数组用 splice 后重新赋值) */
export function applyEvent(state: ReducerState, evt: WireEvent): ReducerState {
  // 1. UUID 去重 (Phase 4.1)
  if (evt.event_id) {
    if (state.seenEventIds.has(evt.event_id)) return state;
    state = {
      ...state,
      seenEventIds: new Set(state.seenEventIds).add(evt.event_id),
    };
  }

  const d = asObj(evt.data);
  const t = state.turns.length > 0 ? state.turns[state.turns.length - 1] : null;

  switch (evt.type) {
    // ── Session metadata ──
    case 'session': {
      return { ...state, sessionId: String(d.session_id ?? '') };
    }
    case 'turn_id': {
      const tid = Number(d.turn_id);
      const sid = String(d.session_id ?? '');
      return {
        ...state,
        turnId: tid > 0 ? tid : state.turnId,
        sessionId: sid || state.sessionId,
      };
    }

    // ── 内容增量 ──
    case 'content_delta':
    case 'stream_text': {
      const delta = String(d.text ?? d.delta ?? '');
      if (!t || !delta) return state;
      const updated: ChatTurn = { ...t, content: t.content + delta };
      return { ...state, turns: [...state.turns.slice(0, -1), updated] };
    }
    case 'content': {
      const text = String(d.text ?? '');
      if (!t || !text) return state;
      const updated: ChatTurn = { ...t, content: text };
      return { ...state, turns: [...state.turns.slice(0, -1), updated] };
    }

    // ── Tool 生命周期 ──
    case 'tool_call_start':
    case 'tool_call':
    case 'tool_call_ready': {
      if (!t) return state;
      const id = String(d.tool_call_id ?? d.id ?? '');
      const name = String(d.name ?? d.tool_name ?? '');
      if (!id || !name) return state;
      const exists = t.steps.some((s) => s.type === 'tool' && s.callId === id);
      if (exists) return state;
      const tool: ToolStep = {
        id: genId(),
        type: 'tool',
        callId: id,
        name,
        displayName: d.display_name as string | undefined,
        status: 'running',
        args: d.args as Record<string, unknown> | undefined,
        startedAt: Date.now(),
      };
      const updated: ChatTurn = { ...t, steps: [...t.steps, tool] };
      return { ...state, turns: [...state.turns.slice(0, -1), updated] };
    }
    case 'tool_result': {
      if (!t) return state;
      const id = String(d.tool_call_id ?? d.id ?? '');
      const idx = t.steps.findIndex((s) => s.type === 'tool' && s.callId === id);
      if (idx < 0) return state;
      const prev = t.steps[idx] as ToolStep;
      const success = Boolean(d.success);
      const updated: ToolStep = {
        ...prev,
        status: success ? 'done' : 'error',
        summary: (d.summary as string) || prev.summary,
        errorMessage: success ? undefined : ((d.error_message as string) || '工具执行失败'),
        durationMs: typeof d.duration_ms === 'number' ? d.duration_ms : prev.durationMs,
      };
      const newSteps = [...t.steps.slice(0, idx), updated, ...t.steps.slice(idx + 1)];
      return { ...state, turns: [...state.turns.slice(0, -1), { ...t, steps: newSteps }] };
    }

    // ── Block 类 (human_ask / staged_confirmation 走同一渲染) ──
    case 'human_ask':
    case 'staged_confirmation': {
      if (!t) return state;
      const tcid = String(d.tool_call_id ?? '');
      const dedupeKey = `human_ask:${tcid || genId()}`;
      if (state.seenBlockKeys.has(dedupeKey)) {
        // 已存在 → 替换 payload (例如答案已填)
        const i = t.blocks.findIndex((b) => b.dedupeKey === dedupeKey);
        if (i < 0) return state;
        const newBlocks = [...t.blocks];
        newBlocks[i] = { ...newBlocks[i], payload: d };
        return { ...state, turns: [...state.turns.slice(0, -1), { ...t, blocks: newBlocks }] };
      }
      const block: BlockItem = {
        id: genId(),
        blockType: 'human_ask',
        payload: d,
        dedupeKey,
      };
      return {
        ...state,
        seenBlockKeys: new Set(state.seenBlockKeys).add(dedupeKey),
        turns: [...state.turns.slice(0, -1), { ...t, blocks: [...t.blocks, block] }],
      };
    }

    // ── Inbox: 视为 turn 边界 ──
    case 'inbox_consumed': {
      const message = String(d.message ?? '');
      // 1. 把当前 turn 收尾
      const closedTurns = t
        ? [...state.turns.slice(0, -1), { ...t, status: 'done' as const, durationMs: Date.now() - t.startedAt }]
        : state.turns;
      // 2. 开新 turn
      return { ...state, turns: [...closedTurns, emptyTurn(message || '(继续)')] };
    }

    // ── Done / Error: 收尾当前 turn ──
    case 'done': {
      if (!t || t.status !== 'streaming') return state;
      const updated: ChatTurn = {
        ...t,
        status: 'done',
        durationMs: Date.now() - t.startedAt,
      };
      return { ...state, turns: [...state.turns.slice(0, -1), updated] };
    }
    case 'error': {
      if (!t) return state;
      const updated: ChatTurn = {
        ...t,
        status: 'error',
        error: {
          code: String(d.code ?? 'UNKNOWN'),
          message: String(d.message ?? '未知错误'),
        },
      };
      return { ...state, turns: [...state.turns.slice(0, -1), updated] };
    }

    // ── 其他事件: 透传不动 (narration / phase / plan / thinking 等) ──
    default:
      return state;
  }
}

// ════════════════════════════════════════════════════════════════════
// Replay helpers
// ════════════════════════════════════════════════════════════════════

/** 从初始状态依次应用一组事件 — 用于 hydrate / reconnect 增量重放 */
export function replayEvents(events: WireEvent[]): ReducerState {
  return events.reduce(applyEvent, createInitialState());
}

/** 在已有 state 上追加一段新事件 — Phase 4.3 since_event_id 增量重放后接续 */
export function appendEvents(state: ReducerState, events: WireEvent[]): ReducerState {
  return events.reduce(applyEvent, state);
}

/** 在指定 user message 处开新 turn — 用于本地 "用户发消息" 立即反映 UI */
export function applyUserMessage(state: ReducerState, text: string): ReducerState {
  const t = state.turns[state.turns.length - 1];
  // 上一 turn 若仍 streaming, 关掉
  const closedTurns = t && t.status === 'streaming'
    ? [...state.turns.slice(0, -1), { ...t, status: 'done' as const, durationMs: Date.now() - t.startedAt }]
    : state.turns;
  return { ...state, turns: [...closedTurns, emptyTurn(text)] };
}
