<template>
  <div class="run-view" data-tauri-no-drag>
    <!-- 重放骨架屏 -->
    <div v-if="chat.isReplaying" class="replay-mask">
      <svg viewBox="0 0 24 24" width="16" height="16" class="replay-spin">
        <circle cx="12" cy="12" r="9" fill="none" stroke="currentColor" stroke-width="2" stroke-dasharray="14 8" />
      </svg>
      <span>正在恢复会话进度…</span>
    </div>

    <!-- 浮动操作栏：固定在视口右上 -->
    <div v-if="chat.turns.length" class="float-actions">
      <button
        v-if="chat.isStreaming"
        class="fa-btn danger"
        title="终止 Agent (Cmd+.)"
        @click="onStop"
      >
        <Square :size="12" />
      </button>
      <button
        v-if="chat.currentSessionId"
        class="fa-btn"
        title="在 Web 端全屏查看"
        @click="openInWeb"
      >
        <ArrowUpRight :size="13" />
      </button>
      <button class="fa-btn" title="发起新指令" @click="onNew">
        <Plus :size="13" />
      </button>
    </div>

    <!-- 主体：对话流 + sticky user message -->
    <div ref="scrollRef" class="run-body" @scroll.passive="onScroll">
      <div class="conversation" v-if="chat.turns.length">
        <section
          v-for="(t, idx) in chat.turns"
          :key="t.startedAt"
          :ref="(el) => bindTurnRef(el, idx)"
          class="conv-turn"
          :class="{
            'conv-last': idx === chat.turns.length - 1,
            'conv-streaming': idx === chat.turns.length - 1 && (chat.isStreaming || chat.isReplaying),
          }"
        >
          <!-- 用户指令：悬浮顶部，高密度排版 -->
          <div class="user-bubble" :title="t.userText">{{ t.userText }}</div>

          <!-- 助手执行区 -->
          <div class="assistant-body">
            <TurnBody :turn="t" :is-active="idx === chat.turns.length - 1" />
          </div>
        </section>

        <!-- 排队中的消息 (streaming 期间用户追加的, 后端 inbox 还没消费)
             样式淡一级, 用户点 stop 或 inbox_cancel 可撤回 -->
        <section
          v-for="p in chat.pendingInbox"
          :key="`pending-${p.msgId}`"
          class="conv-turn conv-pending"
        >
          <div class="user-bubble pending-bubble" :title="p.text">
            <span class="pending-tag">排队中</span>
            {{ p.text }}
          </div>
        </section>
      </div>

      <div v-else class="run-empty">
        <div class="empty-hint">Pilot Ready</div>
        <div class="empty-sub">
          按 <kbd>↵</kbd> 发送指令，或 <kbd>↑</kbd> 翻阅历史
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch, type ComponentPublicInstance } from 'vue';
import { useChatStore } from '../stores/chatStore';
import TurnBody from '../components/run/TurnBody.vue';
import { ArrowUpRight, Plus, Square } from 'lucide-vue-next';

const chat = useChatStore();
const scrollRef = ref<HTMLDivElement | null>(null);

/**
 * Follow 模式
 */
const autoFollow = ref(true);
const NEAR_BOTTOM_THRESHOLD = 60;

function onScroll() {
  const el = scrollRef.value;
  if (!el) return;
  const gap = el.scrollHeight - el.scrollTop - el.clientHeight;
  autoFollow.value = gap < NEAR_BOTTOM_THRESHOLD;
}

let scrollRafId: number | null = null;
function scheduleScrollToBottom() {
  if (!autoFollow.value) return;
  if (scrollRafId !== null) return;
  scrollRafId = requestAnimationFrame(() => {
    scrollRafId = null;
    const el = scrollRef.value;
    if (el) el.scrollTop = el.scrollHeight;
  });
}

// 记录每个 turn section 的 DOM 引用, 新 turn 加入时把其顶部对齐到视口顶部
// —— 让 user-bubble 立即进入 sticky 状态, 用户视觉上"新消息固定在顶"
const turnRefs = new Map<number, HTMLElement>();
function bindTurnRef(el: Element | ComponentPublicInstance | null, idx: number) {
  if (el && el instanceof HTMLElement) {
    turnRefs.set(idx, el);
  } else {
    turnRefs.delete(idx);
  }
}

watch(
  () => chat.turns.length,
  (len, oldLen) => {
    autoFollow.value = true;
    // 新 turn 产生 (len > oldLen): 把新 turn 的 section 顶部对齐视口
    if (len > (oldLen ?? 0)) {
      nextTick(() => {
        const lastIdx = len - 1;
        const section = turnRefs.get(lastIdx);
        const scroller = scrollRef.value;
        if (section && scroller) {
          // scrollTop = section 相对 scroller 的偏移, 使其顶部贴视口顶部
          scroller.scrollTop = section.offsetTop;
        }
      });
      return;
    }
    // turns 数量不变 (只是内容流式追加): 走常规 follow 模式滚底
    nextTick(() => scheduleScrollToBottom());
  },
);

// 流式内容更新 → 仅在 follow 模式下滚
watch(
  () => [
    chat.currentTurn?.content,
    chat.currentTurn?.blocks.length,
  ],
  () => scheduleScrollToBottom(),
);

onMounted(() => {
  chat.tryReconnect();
});

async function openInWeb() {
  if (!chat.currentSessionId) return;
  const url = `${chat.httpBase}/copilot?sid=${encodeURIComponent(chat.currentSessionId)}`;
  try {
    const { open } = await import('@tauri-apps/plugin-shell');
    await open(url);
  } catch {
    window.open(url, '_blank');
  }
}

async function onStop() {
  await chat.stop();
}

function onNew() {
  chat.newSession();
}
</script>

<style scoped>
.run-view {
  flex: 1;
  display: flex;
  flex-direction: column;
  height: 100%;
  position: relative;
  overflow: hidden;
  background: transparent;
}

/* ── 重放遮罩 ── */
.replay-mask {
  position: absolute;
  inset: 0;
  z-index: 20;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  color: #6b7280;
  background: rgba(255, 255, 255, 0.6);
  backdrop-filter: blur(8px);
  -webkit-backdrop-filter: blur(8px);
  font-size: 13px;
  font-weight: 500;
}
.replay-spin { animation: spin 1s linear infinite; color: #3b82f6; }

/* ── 主体：对话流 ── */
.run-body {
  flex: 1;
  overflow-y: auto;
  overflow-x: hidden;
  padding: 0 16px 24px;
  display: flex;
  flex-direction: column;
}

/* 美化滚动条以适应 Pilot悬浮窗 */
.run-body::-webkit-scrollbar {
  width: 4px;
}
.run-body::-webkit-scrollbar-thumb {
  background: rgba(0, 0, 0, 0.1);
  border-radius: 4px;
}

.conversation {
  display: flex;
  flex-direction: column;
}

.conv-turn {
  display: flex;
  flex-direction: column;
}

/* streaming 中的最新 turn: 至少填满视口, 保证 user-bubble 能 sticky 贴顶。
 * 非 streaming 状态下不强行填充, 避免对话结束后底部留一大片空白。
 *
 * 即使在 streaming 时, 也不要用 100% (整个视口), 减去顶部工具栏和 padding
 * 后取 calc 更贴合实际可用区域。若未来工具栏尺寸变化, 通过 CSS 变量或
 * getBoundingClientRect 动态算更佳。 */
.conv-streaming {
  min-height: calc(100% - 40px);
}

/* ── 用户指令 (Sticky) ──
 *
 * 注意: 作为 sticky 吸顶容器, 背景必须完全不透明, 否则滚动时
 * 下方 assistant-body 的内容会透过半透明玻璃看到, 产生"被 user
 * 组件遮挡"的错觉。
 */
.user-bubble {
  position: sticky;
  top: 0;
  z-index: 10;
  margin: 0 -16px;
  padding: 12px 80px 12px 16px; /* 右侧给 float buttons 留空 */

  /* 轻微灰底, 和纯白 assistant 区形成对比, 同时不透明避免滚动透视 */
  background: #f5f6f8;

  /* 上下都有细线, 明确圈出 user 区边界 */
  border-top: 1px solid rgba(0, 0, 0, 0.06);
  border-bottom: 1px solid rgba(0, 0, 0, 0.06);

  font-size: 14px;
  font-weight: 600;
  line-height: 1.5;
  color: #111827;
  white-space: pre-wrap;
  word-break: break-word;
}

/* 第一个 turn 的 user-bubble 顶部不要线, 避免和工具栏/容器边缘重线 */
.conv-turn:first-child .user-bubble {
  border-top: none;
}

/* 排队中的用户消息: 更淡, 明显弱于正在执行的 turn */
.conv-pending .pending-bubble {
  background: #f9fafb;
  color: #6b7280;
  font-weight: 500;
  opacity: 0.9;
}
.pending-tag {
  display: inline-block;
  padding: 1px 6px;
  margin-right: 8px;
  border-radius: 4px;
  background: rgba(107, 114, 128, 0.1);
  color: #6b7280;
  font-size: 11px;
  font-weight: 600;
  vertical-align: middle;
}

/* ── 浮动操作栏 ── */
.float-actions {
  position: absolute;
  top: 0;
  right: 0;
  z-index: 15;
  display: flex;
  gap: 2px;
  padding: 10px 12px;
  pointer-events: none;
}

.fa-btn {
  pointer-events: auto;
  width: 26px;
  height: 26px;
  border: none;
  background: transparent;
  border-radius: 6px;
  color: #9ca3af;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all 0.2s;
}
.fa-btn:hover { background: rgba(0, 0, 0, 0.05); color: #111827; }
.fa-btn.danger:hover { background: rgba(239, 68, 68, 0.1); color: #ef4444; }

/* 助手执行区 (Cursor风格无头像，直接排版)
 *
 * padding-top 给一点呼吸, 视觉上把 assistant 内容明确推到 sticky
 * user-bubble 下方, 避免"贴脸"观感。
 */
.assistant-body {
  flex: 1;
  min-width: 0;
  padding-top: 16px;
  /* 显式低于 sticky user-bubble 的 z-index, 防止内部 block 抢 z 轴 */
  position: relative;
  z-index: 1;
}

/* turn 之间的呼吸空间 */
.conv-turn + .conv-turn {
  margin-top: 32px;
}

/* ── 空态 ── */
.run-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  height: 100%;
  color: #9ca3af;
  gap: 12px;
  padding: 60px 0;
}
.empty-hint {
  font-size: 14px;
  font-weight: 700;
  color: #d1d5db;
  letter-spacing: 0.5px;
}
.empty-sub {
  font-size: 12px;
}
.empty-sub kbd {
  background: rgba(0, 0, 0, 0.04);
  border: 1px solid rgba(0, 0, 0, 0.06);
  padding: 1px 6px;
  border-radius: 4px;
  font-family: inherit;
  font-size: 11px;
  color: #6b7280;
  margin: 0 2px;
}

@keyframes spin { from { transform: rotate(0deg); } to { transform: rotate(360deg); } }
</style>
