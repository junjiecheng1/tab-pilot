/**
 * SSE wire event 类型契约 (与后端 LoopEventType 同步)
 *
 * 权威源:
 *   backend/libs/agent/query_engine/contracts/events.py (LoopEventType)
 *   backend/libs/agent/query_engine/contracts/event_payloads.py (payload dataclass)
 *   backend/libs/agent/channel/adapters/legacy_events.py (wire name 翻译)
 *
 * 使用方式:
 *   - events.ts dispatch 里 `switch (e.type)` 配合 exhaustive check
 *     (利用 TS never type 检查漏事件)
 *   - 漏处理某一支 TS 立刻红, 不再靠字符串 match
 *
 * 约束:
 *   - 只列 tabpilot 前端实际需要分支处理的事件
 *   - 纯内部/调试事件走 onUnknown 兜底, 不收口在此
 *   - 新增事件: 先在 events.py 加 LoopEventType, 再在这里加对应类型
 *
 * wire 名 vs enum 名:
 *   legacy_events.py 做过翻译, 例如 content_delta → stream_text, tool_call_ready → tool_call
 *   这里以 wire 上实际出现的 type 字段为准 (前端见到啥就是啥)
 */

// ═══════════════════════════════════════════════════════════
// 基础
// ═══════════════════════════════════════════════════════════

export interface WireEventBase<T extends string> {
  type: T;
  data?: Record<string, unknown>;
  /** 部分事件在顶层带 scope 信息, 用于 subagent 路由 */
  subagent_task_id?: string;
  task_id?: string;
}

// ═══════════════════════════════════════════════════════════
// 会话生命周期
// ═══════════════════════════════════════════════════════════

export interface SessionEvent extends WireEventBase<'session'> {
  data: { session_id: string };
}

export interface DoneEvent extends WireEventBase<'done'> {
  data: {
    stop_reason?: string;
    result_delivered?: boolean;
    transition?: string;
    recovery_action?: string;
    transition_reason?: string;
  };
}

export interface ErrorEvent extends WireEventBase<'error'> {
  data: {
    message: string;
    code?: string;
    // 后端 ErrorPayload 还可能带 details / stack, 这里只收关键字段
  };
}

// ═══════════════════════════════════════════════════════════
// 内容流
// ═══════════════════════════════════════════════════════════

/** content_delta 的 wire 名 (legacy_events.py:30) */
export interface StreamTextEvent extends WireEventBase<'stream_text'> {
  data: { text: string; delta?: string };
}

/** 部分路径下也会直接发 content_delta 原名 */
export interface ContentDeltaEvent extends WireEventBase<'content_delta'> {
  data: { text?: string; delta?: string };
}

/** 某些 tool 会整段覆盖 assistant 正文 (非 delta) */
export interface ContentEvent extends WireEventBase<'content'> {
  data: { text: string };
}

// ═══════════════════════════════════════════════════════════
// Tool 生命周期
// ═══════════════════════════════════════════════════════════

export interface ToolCallStartEvent extends WireEventBase<'tool_call_start'> {
  data: {
    tool_call_id: string;
    id?: string;
    name?: string;
    tool_name?: string;
    display_name?: string;
    args?: Record<string, unknown>;
  };
}

/** legacy wire name for tool_call_ready */
export interface ToolCallEvent extends WireEventBase<'tool_call'> {
  data: ToolCallStartEvent['data'];
}

export interface ToolCallReadyEvent extends WireEventBase<'tool_call_ready'> {
  data: ToolCallStartEvent['data'];
}

export interface ToolResultEvent extends WireEventBase<'tool_result'> {
  data: {
    tool_call_id: string;
    id?: string;
    tool_name?: string;
    name?: string;
    success: boolean;
    summary?: string;
    error_message?: string;
    duration_ms?: number;
    args?: Record<string, unknown>;
    result?: Record<string, unknown>;
  };
}

// ═══════════════════════════════════════════════════════════
// Human-in-the-loop
// ═══════════════════════════════════════════════════════════

export interface HumanAskEvent extends WireEventBase<'human_ask'> {
  data: {
    tool_call_id: string;
    tool_name?: string;
    prompt?: string;
    question?: string;
    questions?: Array<Record<string, unknown>>;
  };
}

export interface StagedConfirmationEvent extends WireEventBase<'staged_confirmation'> {
  data: {
    tool_call_id: string;
    tool_name?: string;
    prompt?: string;
    payload?: Record<string, unknown>;
  };
}

// ═══════════════════════════════════════════════════════════
// Inbox / 消息队列
// ═══════════════════════════════════════════════════════════

export interface InboxConsumedEvent extends WireEventBase<'inbox_consumed'> {
  data: {
    msg_id: string;
    message: string;
    count?: number;
    turn?: number;
  };
}

export interface InboxDrainedEvent extends WireEventBase<'inbox_drained'> {
  data?: Record<string, unknown>;
}

// ═══════════════════════════════════════════════════════════
// SubAgent
// ═══════════════════════════════════════════════════════════

export interface SubagentStartEvent extends WireEventBase<'subagent_start'> {
  data: {
    subagent_task_id?: string;
    task_id?: string;
    name?: string;
    agent_name?: string;
    subagent_name?: string;
    display_name?: string;
  };
}

export interface SubagentEndEvent extends WireEventBase<'subagent_end'> {
  data: {
    subagent_task_id?: string;
    task_id?: string;
    status?: string;
  };
}

// ═══════════════════════════════════════════════════════════
// 非关键 (前端目前丢弃, 声明以便 TS 穷尽)
// ═══════════════════════════════════════════════════════════

export type DiscardableEventType =
  | 'thinking_start'
  | 'thinking_delta'
  | 'thinking_step'
  | 'phase_start'
  | 'phase_end'
  | 'plan_review'
  | 'plan_update'
  | 'schema_mapping'
  | 'follow_up'
  | 'recover'
  | 'artifacts_summary'
  | 'usage'
  | 'token_usage'
  | 'provider_status'
  | 'reset_stream'
  | 'retry_attempt'
  | 'retry_waiting'
  | 'fallback'
  | 'confirm'
  | 'pilot_status'
  | 'agent_working'
  | 'agent_done'
  // 后端声明但前端不处理 (CI 对齐)
  | 'content_done'
  | 'result_delivered'
  | 'tool_call_delta';

export interface DiscardableEvent extends WireEventBase<DiscardableEventType> {}

// ═══════════════════════════════════════════════════════════
// Unified discriminated union
// ═══════════════════════════════════════════════════════════

export type WireEventTyped =
  | SessionEvent
  | DoneEvent
  | ErrorEvent
  | StreamTextEvent
  | ContentDeltaEvent
  | ContentEvent
  | ToolCallStartEvent
  | ToolCallEvent
  | ToolCallReadyEvent
  | ToolResultEvent
  | HumanAskEvent
  | StagedConfirmationEvent
  | InboxConsumedEvent
  | InboxDrainedEvent
  | SubagentStartEvent
  | SubagentEndEvent
  | DiscardableEvent;

/** 所有已声明的 wire 事件类型字面量 — 用于 CI 一致性脚本校验 */
export const KNOWN_WIRE_EVENT_TYPES = [
  'session',
  'done',
  'error',
  'stream_text',
  'content_delta',
  'content',
  'tool_call_start',
  'tool_call',
  'tool_call_ready',
  'tool_result',
  'human_ask',
  'staged_confirmation',
  'inbox_consumed',
  'inbox_drained',
  'subagent_start',
  'subagent_end',
  // discardable
  'thinking_start',
  'thinking_delta',
  'thinking_step',
  'phase_start',
  'phase_end',
  'plan_review',
  'plan_update',
  'schema_mapping',
  'follow_up',
  'recover',
  'artifacts_summary',
  'usage',
  'token_usage',
  'provider_status',
  'reset_stream',
  'retry_attempt',
  'retry_waiting',
  'fallback',
  'confirm',
  'pilot_status',
  'agent_working',
  'agent_done',
  // 后端声明但前端不处理
  'content_done',
  'result_delivered',
  'tool_call_delta',
] as const;

export type KnownWireEventType = (typeof KNOWN_WIRE_EVENT_TYPES)[number];

/**
 * Exhaustiveness helper — dispatch switch 每个 case 消化完后的 `default` 分支
 * 调用这个函数。如果有 case 漏了, TS 编译期就会报 never 类型不匹配。
 */
export function assertNeverWireEvent(e: never): never {
  throw new Error(`Unhandled wire event type: ${JSON.stringify(e)}`);
}
