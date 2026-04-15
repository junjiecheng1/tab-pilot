/**
 * TabPilot Copilot 数据模型
 *
 * 对齐 record-view AssistantMessage.vue 的分层结构：
 *   Turn = 一次 user→assistant 往返
 *     ├─ steps[]    时间线（tool + narration）
 *     ├─ content    最终回答（markdown，由 content_delta 累积 or message(result) 覆盖）
 *     ├─ blocks[]   结构化块（shell_output / file_preview / browser_* / render_block 等）
 *     ├─ status     streaming | done | error
 *     └─ error?
 *
 * 被过滤不落数据的事件：
 *   - thinking_*
 *   - todo_write 工具
 *   - subagent / plan （Phase 1 不展开，task 工具当普通 tool）
 *   - 未知事件走 onUnknown 兜底
 */

// ── 时间线里的步骤类型 ──
export type TimelineStep = ToolStep | NarrationStep | SubAgentStep;

interface StepBase {
  id: string;
  startedAt: number;
}

/** 工具调用（单行卡片） */
export interface ToolStep extends StepBase {
  type: 'tool';
  callId: string;
  name: string;
  displayName?: string;
  status: 'running' | 'done' | 'error' | 'cancelled';
  summary?: string;       // args 提取或 result.summary
  errorMessage?: string;
  durationMs?: number;
  /** 保留 args，summary 提取用（tool_result 时可能需要回退回 args） */
  args?: Record<string, unknown>;
}

/** 过程性叙述（message(info) 或 provider 级别消息） */
export interface NarrationStep extends StepBase {
  type: 'narration';
  text: string;
}

/** SubAgent 嵌套步骤（对齐 PC SubAgentPanel） */
export interface SubAgentStep extends StepBase {
  type: 'subagent';
  taskId: string;
  name: string;              // 工具/agent 原名（用于图标映射）
  displayName?: string;
  description?: string;
  status: 'running' | 'done' | 'error';
  durationMs?: number;
  errorMessage?: string;
  /** 嵌套的 tool / narration 步骤 */
  steps: Array<ToolStep | NarrationStep>;
}

// ── 独立块区域 ──
export interface BlockItem {
  id: string;
  blockType: string;
  payload: unknown;
  /** 用于 reconnect 去重 */
  dedupeKey: string;
}

// ── 错误 ──
export interface TurnError {
  code: string;
  message: string;
}

/** 一次完整的对话轮次 */
export interface ChatTurn {
  userText: string;
  steps: TimelineStep[];
  content: string;            // 最终 markdown（累积 or 覆盖）
  blocks: BlockItem[];
  status: 'streaming' | 'done' | 'error';
  error: TurnError | null;
  startedAt: number;
  durationMs: number | null;
}

/** 历史穿梭用 */
export interface SessionSummary {
  id: string;
  title: string;
  message_count: number;
  updated_at?: string;
  last_user_message?: string;
}

/** SSE 回调协议 */
export interface CopilotCallbacks {
  onSession?: (sessionId: string) => void;
  onNarration?: (text: string, scope?: ScopeRef) => void;
  onContentDelta?: (delta: string, full: string) => void;
  onContentFull?: (full: string) => void;   // message(result) 整段覆盖
  onToolStart?: (p: {
    id: string;
    name: string;
    displayName?: string;
    args?: Record<string, unknown>;
    scope?: ScopeRef;
  }) => void;
  onToolProgress?: (p: { id: string; summary: string; scope?: ScopeRef }) => void;
  onToolResult?: (p: {
    id: string;
    name: string;
    success: boolean;
    summary?: string;
    errorMessage?: string;
    durationMs?: number;
    args?: Record<string, unknown>;
    scope?: ScopeRef;
  }) => void;
  onBlock?: (p: { blockType: string; payload: unknown; dedupeKey?: string }) => void;
  onError?: (p: TurnError) => void;
  onDone?: () => void;
  onUnknown?: (type: string, data: unknown) => void;
  /** 后端吞入了一条 pending inbox 消息 — 前端职责:
   * 1) 从 pendingInbox 队列移除该 msgId
   * 2) 把当前 turn 封口为 done
   * 3) 用 message 文本开新 turn (SSE 继续流, 后续事件归属新 turn)
   */
  onInboxConsumed?: (msgId: string, message: string) => void;
  /** 新 turn 开始时后端下发 turn_id, 前端记住用于 stop/inbox/tool-reply */
  onTurnId?: (turnId: number, sessionId: string) => void;
  /** 每个带 event_id 的事件都会触发 (Phase 4.3), 前端存为 lastEventId 供 reconnect */
  onLastEventId?: (eventId: string) => void;
  /** 后端标记 inbox 已清空 — 前端清空全部 pending */
  onInboxDrained?: () => void;
  /** SubAgent 生命周期 */
  onSubagentStart?: (p: {
    taskId: string;
    name: string;
    displayName?: string;
    description?: string;
  }) => void;
  onSubagentEnd?: (p: {
    taskId: string;
    success: boolean;
    durationMs?: number;
    errorMessage?: string;
  }) => void;
}

/** 事件作用域：有 taskId 说明属于某个 SubAgent 内部 */
export interface ScopeRef {
  taskId?: string;
}
