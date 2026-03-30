<template>
  <div class="browser-view-page">
    <!-- 浏览器工具栏 -->
    <div class="browser-toolbar">
      <div class="toolbar-left">
        <button class="tb-btn" @click="goBack" title="后退">
          <ArrowLeft :size="16" />
        </button>
        <button class="tb-btn" @click="goForward" title="前进">
          <ArrowRight :size="16" />
        </button>
        <button class="tb-btn" @click="doReload" title="刷新">
          <RotateCw :size="16" />
        </button>
      </div>
      <div class="url-bar">
        <Globe :size="14" class="url-icon" />
        <span class="url-text">{{ currentUrl || '等待浏览器启动...' }}</span>
      </div>
    </div>

    <!-- 浏览器视口 (填满剩余空间) -->
    <div
      class="browser-viewport"
      ref="viewportEl"
      @mousedown="onMouse"
      @mousemove="onMouse"
      @mouseup="onMouse"
      @wheel.prevent="onWheel"
      @keydown.prevent="onKey"
      @keyup.prevent="onKey"
      tabindex="0"
      :class="{ 'agent-active': agentActive }"
    >
      <img
        v-if="frameSrc"
        :src="frameSrc"
        class="frame-image"
        draggable="false"
        alt="浏览器画面"
      />
      <div v-else class="viewport-placeholder">
        <Globe :size="48" stroke-width="1" class="placeholder-icon" />
        <p class="placeholder-text">浏览器未启动</p>
        <p class="placeholder-hint">Agent 操作浏览器时, 画面将实时显示在此处</p>
      </div>

      <!-- Agent 光标指示 -->
      <transition name="cursor-fade">
        <div
          v-if="agentCursor"
          class="agent-cursor"
          :class="agentCursor.action"
          :style="{ left: agentCursor.x + 'px', top: agentCursor.y + 'px' }"
        ></div>
      </transition>

    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import {
  ArrowLeft, ArrowRight, RotateCw, Globe,
} from 'lucide-vue-next';

const frameSrc = ref('');
const connected = ref(false);
const currentUrl = ref('');
const agentActive = ref(false);
const agentCursor = ref<{ x: number; y: number; action: string } | null>(null);
const viewportEl = ref<HTMLElement | null>(null);

let ws: WebSocket | null = null;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;

onMounted(() => { connectStream(); });
onUnmounted(() => {
  if (reconnectTimer) clearTimeout(reconnectTimer);
  ws?.close();
});

function connectStream() {
  if (reconnectTimer) { clearTimeout(reconnectTimer); reconnectTimer = null; }
  const STREAM_PORT = 9223;
  try { ws = new WebSocket(`ws://localhost:${STREAM_PORT}`); }
  catch { scheduleReconnect(); return; }

  ws.onmessage = (event) => {
    try {
      const msg = JSON.parse(event.data);
      if (msg.type === 'frame') {
        frameSrc.value = `data:image/jpeg;base64,${msg.data}`;
      } else if (msg.type === 'status') {
        connected.value = msg.connected ?? false;
        if (msg.url) currentUrl.value = msg.url;
      } else if (msg.type === 'agent_state') {
        agentActive.value = msg.active ?? false;
      }
    } catch { /* ignore */ }
  };

  ws.onopen = () => { connected.value = true; };
  ws.onclose = () => {
    connected.value = false;
    frameSrc.value = '';
    scheduleReconnect();
  };
  ws.onerror = () => {};
}

function scheduleReconnect() {
  if (reconnectTimer) return;
  reconnectTimer = setTimeout(connectStream, 3000);
}

async function sendAction(action: string) {
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('browser_action', { action });
  } catch (e) {
    console.warn('[BrowserView] invoke 失败:', e);
  }
}

function goBack() { sendAction('back'); }
function goForward() { sendAction('forward'); }
function doReload() { sendAction('reload'); }


// ── 双向交互 ──
const MOUSE_MAP: Record<string, string> = {
  mousedown: 'mousePressed', mousemove: 'mouseMoved', mouseup: 'mouseReleased',
};

let lastMouseMove = 0;

function onMouse(e: MouseEvent) {
  if (agentActive.value || !ws || ws.readyState !== WebSocket.OPEN) return;
  // mousemove 节流 ~60fps, click 不节流
  if (e.type === 'mousemove') {
    const now = performance.now();
    if (now - lastMouseMove < 16) return;
    lastMouseMove = now;
  }
  const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
  ws.send(JSON.stringify({
    type: 'input_mouse', eventType: MOUSE_MAP[e.type] || 'mouseMoved',
    x: e.clientX - rect.left, y: e.clientY - rect.top,
    button: e.button === 0 ? 'left' : e.button === 2 ? 'right' : 'none',
    clickCount: e.type === 'mousedown' ? 1 : 0,
  }));
}

function onWheel(e: WheelEvent) {
  if (agentActive.value || !ws || ws.readyState !== WebSocket.OPEN) return;
  const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
  ws.send(JSON.stringify({
    type: 'input_mouse', eventType: 'mouseWheel',
    x: e.clientX - rect.left, y: e.clientY - rect.top,
    deltaX: e.deltaX, deltaY: e.deltaY,
  }));
}

function onKey(e: KeyboardEvent) {
  if (agentActive.value) return;
  if (agentActive.value || !ws || ws.readyState !== WebSocket.OPEN) return;
  ws.send(JSON.stringify({
    type: 'input_keyboard',
    eventType: e.type === 'keydown' ? 'keyDown' : 'keyUp',
    key: e.key, code: e.code,
    text: e.key.length === 1 ? e.key : undefined,
  }));
}

function showAgentCursor(x: number, y: number, action: string) {
  agentCursor.value = { x, y, action };
  setTimeout(() => { agentCursor.value = null; }, 600);
}

defineExpose({ showAgentCursor });
</script>

<style scoped>
.browser-view-page {
  display: flex;
  flex-direction: column;
  position: absolute;
  inset: 0;
  background: var(--bg-app);
  overflow: hidden;
}

/* ── 工具栏 (毛玻璃) ── */
.browser-toolbar {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 12px;
  background: var(--bg-card);
  backdrop-filter: blur(12px);
  -webkit-backdrop-filter: blur(12px);
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
}

.toolbar-left, .toolbar-right {
  display: flex;
  gap: 2px;
}

.tb-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border: none;
  background: transparent;
  border-radius: 6px;
  cursor: pointer;
  color: var(--text-tertiary);
  transition: all 0.15s ease;
}

.tb-btn:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}

.tb-btn.active {
  background: rgba(66, 133, 244, 0.15);
  color: #4285f4;
}

.url-bar {
  flex: 1;
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 5px 12px;
  background: var(--bg-app);
  border-radius: 8px;
  border: 1px solid var(--border-subtle);
  font-size: 12px;
  transition: border-color 0.2s;
}

.url-bar:hover {
  border-color: var(--border);
}

.url-icon { color: var(--text-tertiary); flex-shrink: 0; }

.url-text {
  color: var(--text-secondary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  font-family: 'SF Mono', 'Menlo', monospace;
  font-size: 11px;
}

/* ── 视口 ── */
.browser-viewport {
  position: relative;
  flex: 1;
  background: var(--bg-app);
  overflow: hidden;
  cursor: default;
  outline: none;
  transition: box-shadow 0.4s ease;
}



.frame-image {
  width: 100%;
  height: 100%;
  object-fit: contain;
  user-select: none;
}

/* ── Placeholder (未启动) ── */
.viewport-placeholder {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  height: 100%;
  color: var(--text-quaternary, rgba(255, 255, 255, 0.25));
}

.placeholder-icon {
  opacity: 0.15;
  animation: breathe 4s ease-in-out infinite;
  color: #4285f4;
}

@keyframes breathe {
  0%, 100% { opacity: 0.15; transform: scale(1); }
  50% { opacity: 0.3; transform: scale(1.08); }
}

.placeholder-text {
  font-size: 14px;
  font-weight: 500;
  margin: 0;
  color: var(--text-tertiary);
}

.placeholder-hint {
  font-size: 12px;
  color: var(--text-quaternary, rgba(255, 255, 255, 0.2));
  margin: 0;
}

/* ── Agent 光标涟漪 ── */
.agent-cursor {
  position: absolute;
  width: 24px;
  height: 24px;
  border-radius: 50%;
  border: 2px solid rgba(66, 133, 244, 0.8);
  background: rgba(66, 133, 244, 0.15);
  transform: translate(-50%, -50%);
  pointer-events: none;
  z-index: 10;
  animation: cursor-pop 0.5s ease-out forwards;
}

.agent-cursor.click::after {
  content: '';
  position: absolute;
  inset: -8px;
  border: 1.5px solid rgba(66, 133, 244, 0.3);
  border-radius: 50%;
  animation: cursor-ripple 0.6s ease-out forwards;
}

@keyframes cursor-pop {
  0% { transform: translate(-50%, -50%) scale(0.4); opacity: 1; }
  100% { transform: translate(-50%, -50%) scale(2.2); opacity: 0; }
}

@keyframes cursor-ripple {
  0% { transform: scale(0.6); opacity: 0.9; }
  100% { transform: scale(3); opacity: 0; }
}

.cursor-fade-enter-active { transition: opacity 0.1s; }
.cursor-fade-leave-active { transition: opacity 0.3s; }
.cursor-fade-enter-from, .cursor-fade-leave-to { opacity: 0; }

/* ── AI 操作状态: 仅禁止交互 (蓝色光晕只在 Chrome 页面上) ── */
.browser-viewport.agent-active {
  cursor: not-allowed;
}
</style>
