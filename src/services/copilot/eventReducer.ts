/**
 * Phase 4.4/4.7 · Event-sourcing reducer (FULL)
 *
 * 把对话状态从"逐事件 mutate"切成"events: WireEvent[] → state = reduce(events)"
 * 的纯函数模式。chatStore 用它替代手写 mutation 路径。
 *
 * 设计原则:
 *   1. 纯函数: applyEvent(state, event) → newState
 *   2. UUID 去重: 同 event_id 重复出现只生效一次 (Phase 4.1)
 *   3. SubAgent 嵌套: subagentStack 跟踪当前作用域,
 *      tool / narration 事件按 scope 路由到对应 subagent 的 steps
 *   4. message 工具特殊路径: mode=result → 整段 content 覆盖,
 *      其他 → narration step
 *   5. 过滤工具 (todo_write 等) 直接吞掉
 *
 * 覆盖的 wire 事件 (与 events.ts 对齐):
 *   - session / turn_id            会话/轮次元数据 (chatStore 旁路读)
 *   - thinking_* / phase_* / ...   过滤丢弃
 *   - content_delta / stream_text  增量内容
 *   - tool_call_start / _result    工具步骤 (含 message tool 特殊处理)
 *   - tool_progress                工具进度更新
 *   - human_ask / staged_confirmation / clarification_required → human_ask block
 *   - render_block / shell_output / file_preview / browser_step* / write_preview → 通用 block
 *   - subagent_start / _end        子 agent 嵌套
 *   - inbox_consumed               turn 边界
 *   - error / done                 turn 收尾
 */

import type { WireEvent } from './sse-parser';
import type {
  ChatTurn,
  BlockItem,
  ToolStep,
  NarrationStep,
  SubAgentStep,
  TimelineStep,
} from './types';
import { extractArgsSummary } from './events';

// ════════════════════════════════════════════════════════════════════
// State
// ════════════════════════════════════════════════════════════════════

export interface ReducerState {
  /** 完整对话, 按时间顺序; 每条新 user_message / inbox_consumed 开新 turn */
  turns: ChatTurn[];
  /** SubAgent 栈 — 当前作用域的 task_id, 决定 tool/narration 落在哪 */
  subagentStack: string[];
  /** 已处理过的 event_id, 用于去重 */
  seenEventIds: Set<string>;
  /** 已处理过的 block dedupe key */
  seenBlockKeys: Set<string>;
}

export function createInitialState(): ReducerState {
  return {
    turns: [],
    subagentStack: [],
    seenEventIds: new Set(),
    seenBlockKeys: new Set(),
  };
}

// ════════════════════════════════════════════════════════════════════
// Helpers
// ════════════════════════════════════════════════════════════════════

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

function asObj(v: unknown): Record<string, unknown> {
  return v && typeof v === 'object' ? (v as Record<string, unknown>) : {};
}

const FILTERED_TOOLS = new Set(['todo_write']);

/** 取数据里的 tool_call_id */
function toolId(d: Record<string, unknown>): string {
  return String(d.id ?? d.tool_call_id ?? '');
}

function toolName(d: Record<string, unknown>): string {
  return String(d.name ?? d.tool_name ?? '');
}

function pickDisplayName(d: Record<string, unknown>): string | undefined {
  const result = asObj(d.result);
  const display = asObj(result.display);
  for (const c of [d.display_name, result.display_name, display.name]) {
    if (typeof c === 'string' && c.trim()) return c;
  }
  return undefined;
}

function pickResultSummary(d: Record<string, unknown>): string | undefined {
  const result = asObj(d.result);
  const display = asObj(result.display);
  const nested = asObj(result.data);
  for (const c of [d.summary, result.summary, result.formatted, display.title, nested.brief]) {
    if (typeof c === 'string' && c.trim()) return c;
  }
  return undefined;
}

/** SubAgent 嵌套: 找到事件应落在哪个 steps 数组 */
function resolveStepsArray(turn: ChatTurn, taskId: string | null): TimelineStep[] {
  if (!taskId) return turn.steps;
  // 在 turn.steps 顶层找 subagent
  const sub = turn.steps.find(
    (s): s is SubAgentStep => s.type === 'subagent' && s.taskId === taskId,
  );
  if (sub) return sub.steps as TimelineStep[];
  return turn.steps;
}

/** 把事件作用域 (subagent task_id) 解析出来 */
function resolveScope(evt: WireEvent, state: ReducerState): string | null {
  const d = asObj(evt.data);
  const wireTaskId =
    String((evt as { task_id?: string }).task_id ?? '') ||
    String(d.task_id ?? '') ||
    String(d.subagent_task_id ?? '');
  if (wireTaskId) return wireTaskId;
  const top = state.subagentStack[state.subagentStack.length - 1];
  return top || null;
}

/** 替换 turn 里指定 subagent 的 steps (返回新 turn) */
function withSubagentSteps(turn: ChatTurn, taskId: string, newSteps: TimelineStep[]): ChatTurn {
  const idx = turn.steps.findIndex((s) => s.type === 'subagent' && s.taskId === taskId);
  if (idx < 0) return turn;
  const sub = turn.steps[idx] as SubAgentStep;
  const updated: SubAgentStep = { ...sub, steps: newSteps as Array<ToolStep | NarrationStep> };
  return { ...turn, steps: [...turn.steps.slice(0, idx), updated, ...turn.steps.slice(idx + 1)] };
}

/** 替换 turn 顶层 steps (或 subagent 内 steps) 的统一入口 */
function withSteps(
  turn: ChatTurn,
  taskId: string | null,
  mutator: (steps: TimelineStep[]) => TimelineStep[],
): ChatTurn {
  if (!taskId) return { ...turn, steps: mutator(turn.steps) };
  const sub = turn.steps.find(
    (s): s is SubAgentStep => s.type === 'subagent' && s.taskId === taskId,
  );
  if (!sub) {
    // 找不到 subagent 则降级到根 steps (兼容事件先后乱序)
    return { ...turn, steps: mutator(turn.steps) };
  }
  return withSubagentSteps(turn, taskId, mutator(sub.steps as TimelineStep[]));
}

function blockDedupeKey(blockType: string, d: Record<string, unknown>): string {
  if (blockType === 'shell_output') {
    const cmd = String(d.command ?? '');
    const ts = String(d.timestamp ?? d.ts ?? '');
    return `shell_output:${cmd.slice(0, 60)}:${ts}`;
  }
  if (blockType === 'file_preview') {
    return `file_preview:${String(d.path ?? d.file_path ?? '')}`;
  }
  if (blockType === 'human_ask') {
    return `human_ask:${String(d.tool_call_id ?? Math.random())}`;
  }
  return `${blockType}:${JSON.stringify(d).slice(0, 120)}`;
}

// ════════════════════════════════════════════════════════════════════
// Apply (pure)
// ════════════════════════════════════════════════════════════════════

export function applyEvent(state: ReducerState, evt: WireEvent): ReducerState {
  // 1. UUID 去重
  if (evt.event_id) {
    if (state.seenEventIds.has(evt.event_id)) return state;
    state = {
      ...state,
      seenEventIds: new Set(state.seenEventIds).add(evt.event_id),
    };
  }

  const d = asObj(evt.data);
  const turnIdx = state.turns.length - 1;
  const t = turnIdx >= 0 ? state.turns[turnIdx] : null;

  /** 写回 turn 的快捷函数 */
  const updateTurn = (newTurn: ChatTurn): ReducerState => ({
    ...state,
    turns: [...state.turns.slice(0, turnIdx), newTurn],
  });

  switch (evt.type) {
    // ── 元数据事件: 不影响 turns, chatStore 旁路读 ──
    case 'session':
    case 'turn_id':
      return state;

    // ── 合成事件: 用户本地发消息 → 开新 turn ──
    case 'user_message': {
      const text = String(d.text ?? '');
      const closedTurns = t && t.status === 'streaming'
        ? [
            ...state.turns.slice(0, turnIdx),
            { ...t, status: 'done' as const, durationMs: Date.now() - t.startedAt },
          ]
        : state.turns;
      return {
        ...state,
        subagentStack: [],
        turns: [...closedTurns, emptyTurn(text)],
      };
    }

    // ── 完全过滤的事件 ──
    case 'thinking_start':
    case 'thinking_delta':
    case 'thinking_step':
    case 'provider_status':
    case 'phase_start':
    case 'phase_end':
    case 'plan_review':
    case 'plan_update':
    case 'schema_mapping':
    case 'follow_up':
    case 'recover':
    case 'usage':
    case 'token_usage':
    case 'progress':
    case 'task_progress':
    case 'reset_stream':
    case 'retry_attempt':
    case 'retry_waiting':
    case 'fallback':
    case 'confirm':
    case 'pilot_status':
    case 'agent_working':
    case 'agent_done':
    case 'tool_call_delta':
    case 'tool_call_args':
    case 'tool_content':
    case 'inbox_drained':
    case 'artifacts_summary':
      return state;

    // ── 内容增量 ──
    case 'content_delta':
    case 'content':
    case 'stream_text': {
      const delta = String(d.delta ?? d.text ?? '');
      if (!t || !delta) return state;
      return updateTurn({ ...t, content: t.content + delta });
    }

    // ── Tool 生命周期 ──
    case 'tool_call_start':
    case 'tool_call':
    case 'tool_call_ready': {
      if (!t) return state;
      const id = toolId(d);
      const name = toolName(d);
      if (!id || !name) return state;
      if (FILTERED_TOOLS.has(name)) return state;
      // message 工具不进 timeline, 等 result 时按 mode 分流
      if (name === 'message') return state;

      const scope = resolveScope(evt, state);
      const args = asObj(d.args);
      const tool: ToolStep = {
        id: genId(),
        type: 'tool',
        callId: id,
        name,
        displayName: pickDisplayName(d),
        status: 'running',
        // 占位 summary 来自 args 字段, 待 tool_result 时被实际 summary 覆盖
        summary: extractArgsSummary(name, args),
        args,
        startedAt: Date.now(),
      };
      return updateTurn(
        withSteps(t, scope, (steps) => {
          // 防重: 同 callId 已存在则跳过
          if (steps.some((s) => s.type === 'tool' && s.callId === id)) return steps;
          return [...steps, tool];
        }),
      );
    }

    case 'tool_progress': {
      if (!t) return state;
      const id = toolId(d);
      const detail = String(d.detail ?? '');
      if (!id || !detail) return state;
      const scope = resolveScope(evt, state);
      return updateTurn(
        withSteps(t, scope, (steps) => {
          const i = steps.findIndex((s) => s.type === 'tool' && s.callId === id);
          if (i < 0) return steps;
          const prev = steps[i] as ToolStep;
          return [...steps.slice(0, i), { ...prev, summary: detail }, ...steps.slice(i + 1)];
        }),
      );
    }

    case 'tool_result': {
      if (!t) return state;
      const id = toolId(d);
      const name = toolName(d);
      const args = asObj(d.args);

      if (FILTERED_TOOLS.has(name)) return state;

      // message 工具特殊路径
      if (name === 'message') {
        const mode = String(args.mode ?? '');
        const text = String(args.message ?? '');
        if (!text) return state;
        if (mode === 'result') {
          // 整段 content 覆盖
          return updateTurn({ ...t, content: text });
        }
        // narration step
        const scope = resolveScope(evt, state);
        const narration: NarrationStep = {
          id: genId(),
          type: 'narration',
          text,
          startedAt: Date.now(),
        };
        return updateTurn(
          withSteps(t, scope, (steps) => [...steps, narration]),
        );
      }

      // 普通 tool result: 找 step 改 status
      const success = Boolean(d.success);
      const scope = resolveScope(evt, state);
      return updateTurn(
        withSteps(t, scope, (steps) => {
          const i = steps.findIndex((s) => s.type === 'tool' && s.callId === id);
          if (i < 0) {
            // result 早于 start 到达 (Gemini 等), 创建 stub tool
            const stub: ToolStep = {
              id: genId(),
              type: 'tool',
              callId: id,
              name,
              displayName: pickDisplayName(d),
              status: success ? 'done' : 'error',
              summary: pickResultSummary(d),
              errorMessage: success ? undefined : (d.error as string | undefined),
              durationMs: d.duration_ms as number | undefined,
              args,
              startedAt: Date.now(),
            };
            return [...steps, stub];
          }
          const prev = steps[i] as ToolStep;
          const updated: ToolStep = {
            ...prev,
            status: success ? 'done' : 'error',
            summary: pickResultSummary(d) || prev.summary,
            errorMessage: success ? undefined : ((d.error as string) || prev.errorMessage),
            durationMs: (d.duration_ms as number | undefined) ?? prev.durationMs,
          };
          return [...steps.slice(0, i), updated, ...steps.slice(i + 1)];
        }),
      );
    }

    // ── Block: 结构化区域 ──
    case 'human_ask':
    case 'staged_confirmation':
    case 'clarification_required':
    case 'render_block':
    case 'shell_output':
    case 'file_preview':
    case 'browser_step':
    case 'browser_screenshot':
    case 'write_preview': {
      if (!t) return state;
      let blockType = 'human_ask';
      let payload: Record<string, unknown> = d;
      if (evt.type === 'staged_confirmation') {
        const tcid = String(d.tool_call_id ?? '');
        payload = {
          tool_call_id: tcid,
          question: String(d.prompt ?? `确认执行 ${d.tool_name ?? '当前操作'}`),
          question_type: 'confirm',
          options: [
            { label: '确认执行', value: '是' },
            { label: '取消', value: '否' },
          ],
          ...d,
        };
      } else if (evt.type === 'clarification_required') {
        payload = {
          tool_call_id: String(d.tool_call_id ?? d.id ?? ''),
          question: String(d.message ?? d.title ?? '请补充信息'),
          question_type: 'open',
          ...d,
        };
      } else if (evt.type === 'render_block') {
        blockType = String(d.block_type ?? 'unknown');
        payload = (d.data as Record<string, unknown>) ?? d;
      } else if (evt.type !== 'human_ask') {
        // shell_output / file_preview / browser_* / write_preview
        blockType = evt.type;
      }

      const dedupeKey = blockDedupeKey(blockType, payload);
      if (state.seenBlockKeys.has(dedupeKey)) {
        // 已存在 → 替换 payload (例如 ask answered=true 更新)
        const i = t.blocks.findIndex((b) => b.dedupeKey === dedupeKey);
        if (i < 0) return state;
        const newBlocks = [...t.blocks];
        newBlocks[i] = { ...newBlocks[i], payload };
        return updateTurn({ ...t, blocks: newBlocks });
      }
      const block: BlockItem = {
        id: genId(),
        blockType,
        payload,
        dedupeKey,
      };
      return {
        ...state,
        seenBlockKeys: new Set(state.seenBlockKeys).add(dedupeKey),
        turns: [...state.turns.slice(0, turnIdx), { ...t, blocks: [...t.blocks, block] }],
      };
    }

    // ── SubAgent 生命周期 ──
    case 'subagent_start': {
      if (!t) return state;
      const taskId = String(
        d.subagent_task_id ?? d.task_id ?? (evt as { task_id?: string }).task_id ?? '',
      );
      if (!taskId) return state;
      const name = String(d.name ?? d.subagent_name ?? d.agent_name ?? '');
      const newSub: SubAgentStep = {
        id: taskId,
        type: 'subagent',
        taskId,
        name,
        displayName: String(
          d.display_name ?? d.subagent_display_name ?? d.agent_name ?? name,
        ),
        description: String(d.description ?? ''),
        status: 'running',
        steps: [],
        startedAt: Date.now(),
      };
      return {
        ...state,
        subagentStack: [...state.subagentStack, taskId],
        turns: [
          ...state.turns.slice(0, turnIdx),
          { ...t, steps: [...t.steps, newSub] },
        ],
      };
    }

    case 'subagent_end': {
      if (!t) return state;
      const taskId = String(
        d.subagent_task_id ?? d.task_id ?? (evt as { task_id?: string }).task_id ?? '',
      );
      if (!taskId) return state;
      const success = Boolean(d.success ?? d.status === 'completed');
      const i = t.steps.findIndex((s) => s.type === 'subagent' && s.taskId === taskId);
      if (i < 0) {
        return { ...state, subagentStack: state.subagentStack.filter((id) => id !== taskId) };
      }
      const sub = t.steps[i] as SubAgentStep;
      const updated: SubAgentStep = {
        ...sub,
        status: success ? 'done' : 'error',
        durationMs: (d.duration_ms as number | undefined) ?? sub.durationMs,
        errorMessage: success ? undefined : String(d.error ?? ''),
      };
      return {
        ...state,
        subagentStack: state.subagentStack.filter((id) => id !== taskId),
        turns: [
          ...state.turns.slice(0, turnIdx),
          { ...t, steps: [...t.steps.slice(0, i), updated, ...t.steps.slice(i + 1)] },
        ],
      };
    }

    // ── Inbox: turn 边界 ──
    case 'inbox_consumed': {
      const message = String(d.message ?? '');
      const closedTurns = t
        ? [
            ...state.turns.slice(0, turnIdx),
            { ...t, status: 'done' as const, durationMs: Date.now() - t.startedAt },
          ]
        : state.turns;
      return {
        ...state,
        subagentStack: [],  // 新 turn 重置 scope
        turns: [...closedTurns, emptyTurn(message || '(继续)')],
      };
    }

    // ── Done / Error: 收尾当前 turn ──
    case 'done': {
      if (!t || t.status !== 'streaming') return state;
      // 同时把所有 running 的 tool/subagent 标记为 cancelled/done
      const cancelInArr = (arr: TimelineStep[]): TimelineStep[] =>
        arr.map((s) => {
          if (s.type === 'tool' && s.status === 'running') {
            return { ...s, status: 'cancelled' as const };
          }
          if (s.type === 'subagent') {
            const sub = s as SubAgentStep;
            return {
              ...sub,
              status: sub.status === 'running' ? ('done' as const) : sub.status,
              steps: cancelInArr(sub.steps as TimelineStep[]) as Array<ToolStep | NarrationStep>,
            };
          }
          return s;
        });
      const updated: ChatTurn = {
        ...t,
        status: 'done',
        durationMs: Date.now() - t.startedAt,
        steps: cancelInArr(t.steps),
      };
      return updateTurn(updated);
    }

    case 'error': {
      if (!t) return state;
      const updated: ChatTurn = {
        ...t,
        status: 'error',
        error: {
          code: String(d.code ?? d.provider_error_kind ?? d.stop_reason ?? 'QE_ERROR'),
          message: String(d.message ?? d.transition_reason ?? d.stop_reason ?? '未知错误'),
        },
      };
      return updateTurn(updated);
    }

    default:
      return state;
  }
}

// ════════════════════════════════════════════════════════════════════
// Replay helpers
// ════════════════════════════════════════════════════════════════════

export function replayEvents(events: WireEvent[]): ReducerState {
  return events.reduce(applyEvent, createInitialState());
}

export function appendEvents(state: ReducerState, events: WireEvent[]): ReducerState {
  return events.reduce(applyEvent, state);
}

/**
 * 用户本地发了一条消息 — 不是来自后端事件, 但需要立即出现在 turns 里。
 * 在 events 数组里塞一个合成事件 (类型 user_message), 这样重放也能复现。
 */
export interface UserMessageEvent {
  type: 'user_message';
  data: { text: string };
  event_id?: string;
}

export function makeUserMessageEvent(text: string): UserMessageEvent {
  return {
    type: 'user_message',
    data: { text },
    event_id: 'um-' + genId(),
  };
}

/** 把 user_message 合成事件应用到 state — 开新 turn */
export function applyUserMessage(state: ReducerState, text: string): ReducerState {
  const turnIdx = state.turns.length - 1;
  const t = turnIdx >= 0 ? state.turns[turnIdx] : null;
  const closedTurns = t && t.status === 'streaming'
    ? [
        ...state.turns.slice(0, turnIdx),
        { ...t, status: 'done' as const, durationMs: Date.now() - t.startedAt },
      ]
    : state.turns;
  return {
    ...state,
    subagentStack: [],
    turns: [...closedTurns, emptyTurn(text)],
  };
}
