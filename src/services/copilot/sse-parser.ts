/**
 * SSE 解析器 — 从 fetch 的 ReadableStream 里吐出 `{type, data}` 事件
 *
 * 协议：行分帧，`data: {...json...}` 前缀，`data: [DONE]` 作为终止 sentinel。
 * 从 record-view copilot-api.ts 的 consumeSSE 裁出独立函数，无依赖。
 */

export interface WireEvent {
  type: string;
  data: unknown;
  /** Phase 4.1: 后端 LoopEvent.event_id, 前端用于 reconnect since_event_id 增量重放 */
  event_id?: string;
  /** Query engine turn context */
  turn_id?: string;
  phase?: string;
}

export interface SSEParseOptions {
  onEvent: (evt: WireEvent) => void;
  onDone: () => void;
  onParseError?: (line: string, err: unknown) => void;
}

/**
 * 消费 Response.body 直到流结束或 abort。
 *
 * 注意：调用方负责处理 AbortError — 此函数内部仅把 reader 读空。
 */
export async function consumeSSE(
  response: Response,
  opts: SSEParseOptions,
): Promise<void> {
  const reader = response.body?.getReader();
  if (!reader) throw new Error('No response body');

  const decoder = new TextDecoder();
  let buffer = '';
  let sawDone = false;

  while (true) {
    const { done, value } = await reader.read();
    if (done) break;

    buffer += decoder.decode(value, { stream: true });
    const lines = buffer.split('\n');
    buffer = lines.pop() || '';

    for (const line of lines) {
      if (!line.startsWith('data: ')) continue;
      const data = line.slice(6).trim();
      if (!data) continue;
      if (data === '[DONE]') {
        sawDone = true;
        opts.onDone();
        continue;
      }
      try {
        const evt = JSON.parse(data) as WireEvent;
        opts.onEvent(evt);
      } catch (e) {
        opts.onParseError?.(line, e);
      }
    }
  }

  if (!sawDone) opts.onDone();
}
