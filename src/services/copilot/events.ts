/**
 * SSE wire event → CopilotCallbacks 分发
 *
 * 对齐 AssistantMessage.vue 的过滤/合并规则：
 *  - thinking_*               丢弃
 *  - todo_write 工具          丢弃
 *  - message 工具             特殊：mode=result → onContentFull；mode=info / 无 → onNarration
 *  - task 工具                当普通 tool（不展开 subagent inline）
 *  - subagent_start/end       走 cb.onSubagentStart/onSubagentEnd, 同时维护 subagentStack
 *                             以便子工具事件通过 resolveScope 路由到对应 taskId
 *  - plan_review/plan_update  暂时丢弃 (linear timeline)
 *  - artifacts_summary        通过 message.contents 的 artifacts_summary block 持久化,
 *                             前端从 message contents 解析 (utils/artifactParser),
 *                             此处不单独事件流处理
 *  - 其他未识别                onUnknown 兜底
 */

import type { WireEvent } from './sse-parser';
import type { CopilotCallbacks, ScopeRef } from './types';
import { KNOWN_WIRE_EVENT_TYPES, type KnownWireEventType } from '../../types/wire-events';

// 运行时校验: 未知 type 走 onUnknown, 开发模式下打 warn 便于发现后端漂移
const KNOWN_TYPE_SET: ReadonlySet<string> = new Set(KNOWN_WIRE_EVENT_TYPES);
function isKnownWireType(t: string): t is KnownWireEventType {
  return KNOWN_TYPE_SET.has(t);
}

interface DispatchState {
  /** content_delta 累积 */
  content: string;
  /** 活跃 subagent 栈：tool/narration 事件会路由到栈顶 */
  subagentStack: string[];
}

// ── 工具函数 ──

function asObj(v: unknown): Record<string, unknown> {
  return v && typeof v === 'object' ? (v as Record<string, unknown>) : {};
}

function toolId(d: Record<string, unknown>): string {
  return String(d.id ?? d.tool_call_id ?? '');
}

function toolName(d: Record<string, unknown>): string {
  return String(d.name ?? d.tool_name ?? '');
}

function pickDisplayName(d: Record<string, unknown>): string | undefined {
  const result = asObj(d.result);
  const display = asObj(result.display);
  const candidates = [d.display_name, result.display_name, display.name];
  for (const c of candidates) if (typeof c === 'string' && c.trim()) return c;
  return undefined;
}

function pickResultSummary(d: Record<string, unknown>): string | undefined {
  const result = asObj(d.result);
  const display = asObj(result.display);
  const nested = asObj(result.data);
  const candidates = [
    d.summary,
    result.summary,
    result.formatted,
    display.title,
    nested.brief,
  ];
  for (const c of candidates) if (typeof c === 'string' && c.trim()) return c;
  return undefined;
}

/**
 * 从 tool args 里挑一个最有信息量的字段，显示在 tool 卡片 summary 里。
 * 对齐 ToolActivity.vue:143-162 的提取优先级。
 */
export function extractArgsSummary(
  name: string,
  args: Record<string, unknown> | undefined,
): string | undefined {
  if (!args) return undefined;
  const keys = [
    'query',
    'command',
    'path',
    'file_path',
    'filename',
    'url',
    'prompt',
    'message',
    'code',
    'text',
    'name',
  ];
  for (const k of keys) {
    const v = args[k];
    if (typeof v === 'string' && v.trim()) {
      const s = v.trim();
      // 取第一行，过长省略
      const firstLine = s.split(/\r?\n/)[0];
      return firstLine.length > 80 ? firstLine.slice(0, 80) + '…' : firstLine;
    }
  }
  // 兜底：找第一个 string
  for (const v of Object.values(args)) {
    if (typeof v === 'string' && v.trim()) {
      const s = v.trim();
      return s.length > 80 ? s.slice(0, 80) + '…' : s;
    }
  }
  // 实在没有，显示 tool name
  return name;
}

// ── 块去重 key（参考 AssistantMessage.vue:330-342） ──
function blockDedupeKey(blockType: string, d: Record<string, unknown>): string {
  if (blockType === 'shell_output') {
    return `shell:${String(d.command_id ?? d.commandId ?? `${d.command ?? ''}|${d.cwd ?? ''}`)}`;
  }
  if (blockType === 'file_preview') {
    return `file:${String(d.action ?? '')}:${String(d.path ?? '')}`;
  }
  if (blockType === 'browser_step' || blockType === 'browser_steps') {
    return 'browser_steps';
  }
  if (blockType === 'pilot_confirm') {
    return `pilot_confirm:${String(d.confirm_id ?? d.tool_call_id ?? Math.random())}`;
  }
  if (blockType === 'human_ask') {
    return `human_ask:${String(d.tool_call_id ?? Math.random())}`;
  }
  return `${blockType}:${JSON.stringify(d).slice(0, 120)}`;
}

// ── 被过滤的工具 ──
const FILTERED_TOOLS = new Set(['todo_write']);

export function dispatchEvent(
  evt: WireEvent,
  cb: CopilotCallbacks,
  state: DispatchState,
): void {
  const { type, data } = evt;
  const d = asObj(data);

  switch (type) {
    // Lazy Session
    case 'session':
      cb.onSession?.(String(d.session_id ?? ''));
      return;

    // ── Thinking：彻底丢弃 ──
    case 'thinking_start':
    case 'thinking_delta':
    case 'thinking_step':
      return;

    case 'provider_status':
      // 完全丢弃（retry/fallback/thinking_delta 都不在 TabPilot 展示）
      return;

    // ── Content ──
    case 'content_delta':
    case 'content':
    case 'stream_text': {
      const delta = String(d.delta ?? d.text ?? '');
      if (!delta) return;
      state.content += delta;
      cb.onContentDelta?.(delta, state.content);
      return;
    }

    // ── Tool（先拦截特殊工具） ──
    case 'tool_call_start':
    case 'tool_call':
    case 'tool_call_ready': {
      const id = toolId(d);
      const name = toolName(d);
      // 被过滤的工具直接吞掉
      if (FILTERED_TOOLS.has(name)) return;

      // message 工具不进 timeline —— 等 tool_result 时按 mode 分流
      if (name === 'message') return;

      const args = asObj(d.args) as Record<string, unknown>;
      cb.onToolStart?.({
        id: id || name,
        name,
        displayName: pickDisplayName(d),
        args,
        scope: resolveScope(evt, state),
      });
      return;
    }

    case 'tool_call_delta':
    case 'tool_call_args':
    case 'tool_content':
      return;

    case 'tool_progress': {
      const detail = String(d.detail ?? '');
      const id = toolId(d);
      if (detail && id) {
        cb.onToolProgress?.({ id, summary: detail, scope: resolveScope(evt, state) });
      }
      return;
    }

    case 'tool_result': {
      const id = toolId(d);
      const name = toolName(d);
      const args = asObj(d.args) as Record<string, unknown>;

      if (FILTERED_TOOLS.has(name)) return;

      // message 工具：按 mode 分流
      if (name === 'message') {
        const mode = String(args.mode ?? '');
        const text = String(args.message ?? '');
        if (!text) return;
        if (mode === 'result') {
          cb.onContentFull?.(text);
        } else {
          cb.onNarration?.(text, resolveScope(evt, state));
        }
        return;
      }

      const success = Boolean(d.success);
      cb.onToolResult?.({
        id: id || name,
        name,
        success,
        summary: pickResultSummary(d),
        errorMessage: success ? undefined : (d.error as string | undefined),
        durationMs: d.duration_ms as number | undefined,
        args,
        scope: resolveScope(evt, state),
      });
      return;
    }

    // ── 结构化块（独立区） ──
    case 'render_block': {
      const blockType = String(d.block_type ?? 'unknown');
      const payload = d.data ?? d;
      cb.onBlock?.({
        blockType,
        payload,
        dedupeKey: blockDedupeKey(blockType, asObj(payload)),
      });
      return;
    }

    case 'shell_output':
    case 'file_preview':
    case 'browser_step':
    case 'browser_screenshot':
    case 'write_preview': {
      const blockType = type === 'browser_step' ? 'browser_step' : type;
      cb.onBlock?.({
        blockType,
        payload: d,
        dedupeKey: blockDedupeKey(blockType, d),
      });
      return;
    }

    // ── 用户交互 ask ──
    case 'human_ask': {
      cb.onBlock?.({
        blockType: 'human_ask',
        payload: d,
        dedupeKey: blockDedupeKey('human_ask', d),
      });
      return;
    }

    case 'staged_confirmation': {
      // 合成成 human_ask 的 confirm 形态
      const tcid = String(d.tool_call_id ?? '');
      const payload = {
        tool_call_id: tcid,
        question: String(d.prompt ?? `确认执行 ${d.tool_name ?? '当前操作'}`),
        question_type: 'confirm',
        options: [
          { label: '确认执行', value: '是' },
          { label: '取消', value: '否' },
        ],
        ...d,
      };
      cb.onBlock?.({
        blockType: 'human_ask',
        payload,
        dedupeKey: blockDedupeKey('human_ask', payload),
      });
      return;
    }

    case 'clarification_required': {
      const payload = {
        tool_call_id: String(d.tool_call_id ?? d.id ?? ''),
        question: String(d.message ?? d.title ?? '请补充信息'),
        question_type: 'open',
        ...d,
      };
      cb.onBlock?.({
        blockType: 'human_ask',
        payload,
        dedupeKey: blockDedupeKey('human_ask', payload),
      });
      return;
    }

    // ── 终态 ──
    case 'error':
      cb.onError?.({
        code: String(d.code ?? d.provider_error_kind ?? d.stop_reason ?? 'QE_ERROR'),
        message: String(d.message ?? d.transition_reason ?? d.stop_reason ?? '未知错误'),
      });
      return;

    case 'done':
      cb.onDone?.();
      return;

    // ── 显式忽略的事件 ──
    case 'usage':
    case 'token_usage':
    case 'progress':
    case 'task_progress':
    case 'plan_review':
    case 'plan_update':
    case 'schema_mapping':
    case 'follow_up':
    case 'turn_id': {
      // Phase 2.2: 后端在新 turn 开头发, 前端记下用于后续 stop/inbox/tool-reply
      const turnId = Number(d.turn_id);
      const sid = String(d.session_id ?? '');
      if (turnId > 0) cb.onTurnId?.(turnId, sid);
      return;
    }
    case 'inbox_consumed': {
      // 后端消费了某条 pending 消息, 视为 turn 边界:
      // 1) 把当前 turn 封口
      // 2) 用消费的 message 文本开新 turn
      // 3) SSE 继续流, 后续事件落到新 turn 上
      // 见 chatStore 的 onInboxConsumed 实现
      const msgId = String(d.msg_id ?? d.id ?? '');
      const message = String(d.message ?? '');
      if (msgId) cb.onInboxConsumed?.(msgId, message);
      return;
    }
    case 'inbox_drained': {
      cb.onInboxDrained?.();
      return;
    }
    case 'reset_stream':
    case 'retry_attempt':
    case 'retry_waiting':
    case 'fallback':
    case 'confirm':
    case 'pilot_status':
    case 'agent_working':
    case 'agent_done':
      return;

    // ── SubAgent 生命周期 ──
    case 'subagent_start': {
      const taskId = String(
        d.subagent_task_id ?? d.task_id ?? (evt as unknown as { task_id?: string }).task_id ?? '',
      );
      if (!taskId) return;
      const name = String(d.name ?? d.subagent_name ?? d.agent_name ?? '');
      cb.onSubagentStart?.({
        taskId,
        name,
        displayName: String(
          d.display_name ?? d.subagent_display_name ?? d.agent_name ?? name,
        ),
        description: String(d.description ?? ''),
      });
      state.subagentStack.push(taskId);
      return;
    }

    case 'subagent_end': {
      const taskId = String(
        d.subagent_task_id ?? d.task_id ?? (evt as unknown as { task_id?: string }).task_id ?? '',
      );
      if (!taskId) return;
      const success = Boolean(d.success ?? d.status === 'completed');
      cb.onSubagentEnd?.({
        taskId,
        success,
        durationMs: d.duration_ms as number | undefined,
        errorMessage: success ? undefined : String(d.error ?? ''),
      });
      state.subagentStack = state.subagentStack.filter((id) => id !== taskId);
      return;
    }

    default:
      // 未知 type: 运行时校验一下是不是后端新增但前端未声明的事件
      // (生产无副作用, 开发时 warn 便于及时发现契约漂移)
      if (typeof type === 'string' && !isKnownWireType(type)) {
        if (import.meta && (import.meta as { env?: { DEV?: boolean } }).env?.DEV) {
          console.warn(
            '[Copilot] Unknown wire event type:', type,
            '— add to types/wire-events.ts or map to known',
          );
        }
      }
      cb.onUnknown?.(String(type), data);
      return;
  }
}

export function makeDispatchState(): DispatchState {
  return { content: '', subagentStack: [] };
}

/** 从事件里抽出 scope（taskId）— 事件自身字段优先，否则用栈顶 subagent */
function resolveScope(evt: WireEvent, state: DispatchState): ScopeRef | undefined {
  const d = asObj(evt.data);
  const wireTaskId =
    (evt as unknown as { task_id?: string }).task_id
    || (d.task_id as string | undefined)
    || (d.subagent_task_id as string | undefined);
  if (wireTaskId) return { taskId: wireTaskId };
  const top = state.subagentStack[state.subagentStack.length - 1];
  if (top) return { taskId: top };
  return undefined;
}
