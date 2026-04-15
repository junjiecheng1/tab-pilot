<template>
  <div class="omni-wrapper" data-tauri-drag-region @mousedown="startWindowDrag">
    <div class="omni-panel" :class="{ 'panel-expanded': $route.path !== '/' }" data-tauri-drag-region>
      
      <!-- Mac 原生控件区（提供上方的独立站位和拖拽支持） -->
      <div class="mac-titlebar" data-tauri-drag-region>
        <span class="titlebar-text" data-tauri-drag-region>TabPilot</span>
      </div>

      <!-- 搜索/输入头 -->
      <div class="omni-header" data-tauri-drag-region>
        <img src="/logo.png" class="brand-badge" @click="router.push('/')" data-tauri-no-drag>
        
        <template v-if="store.isConnected">
          <ModelPicker class="omni-model" />
          <input
            ref="inputRef"
            v-model="cmd"
            type="text"
            class="omni-input"
            placeholder="探索 TabPilot、执行指令..."
            @keydown.enter="onEnter"
            @keydown.esc="handleEsc"
            @keydown.up.prevent="handleArrowUp"
            @keydown.down.prevent="handleArrowDown"
            data-tauri-no-drag
          >
        </template>
        <template v-else>
          <div class="omni-auth-trigger" @click="handleAuthClick" data-tauri-no-drag>
            <div class="auth-placeholder-btn">
              <Lock :size="14" />
              <span>尚未连接 Agent，点击进行登录授权状态</span>
            </div>
          </div>
        </template>
        
        <!-- 头部状态区 -->
        <div class="header-actions" data-tauri-no-drag>
          <div class="omni-status" :class="{ connected: store.isConnected }" :title="store.isConnected ? 'Agent 在线' : '未连接/授权'"></div>
        </div>
      </div>

      <!-- 核心路由内容区 (设置、日志面板) -->
      <div class="omni-router-view" v-show="$route.path !== '/'" data-tauri-no-drag>
        <router-view />
      </div>

      <!-- 底部隐式导引条 (起视觉平衡和快捷作用，常驻) -->
      <div class="omni-footer" data-tauri-drag-region>
        <div class="footer-left" data-tauri-no-drag>
          <span class="footer-link" @click="router.push('/logs')" :class="{ active: $route.path === '/logs' }">执行日志 (L)</span>
          <span class="divider">|</span>
          <span class="footer-link" @click="router.push('/settings')" :class="{ active: $route.path === '/settings' }">偏好设置 (S)</span>
        </div>
        <div class="footer-right" data-tauri-drag-region>
          <span class="sys-hint">↵ 送出 · Esc 收起</span>
        </div>
      </div>

    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch, nextTick } from 'vue';
import { useRouter, useRoute } from 'vue-router';
import { usePilotStore } from './services/pilotStore';
import { useChatStore } from './stores/chatStore';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { LogicalSize } from '@tauri-apps/api/dpi';
import { X, Minus, Terminal, Settings, Lock } from 'lucide-vue-next';
import ModelPicker from './components/ModelPicker.vue';

const store = usePilotStore();
const chat = useChatStore();
const router = useRouter();
const route = useRoute();
const cmd = ref('');
const inputRef = ref<HTMLInputElement | null>(null);

const isConnected = computed(() => store.isConnected);

async function handleAuthClick() {
  const base = store.serverUrl
    .replace(/^wss:\/\//, 'https://')
    .replace(/^ws:\/\//, 'http://')
    .replace(/\/ws\/.*$/, '');

  let challenge = '';
  try {
    const { getAuthChallenge } = await import('./services/bridge');
    challenge = await getAuthChallenge();
  } catch { /* ignore */ }

  const authUrl = `${base}/auth/pilot?challenge=${challenge}`;
  try {
    const { open } = await import('@tauri-apps/plugin-shell');
    await open(authUrl);
  } catch {
    window.open(authUrl, '_blank');
  }

  if (challenge) {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('start_auth_poll', { challenge });
    } catch { /* ignore */ }
  }
}

async function startWindowDrag(e: MouseEvent) {
  // 仅响应鼠标左键拖拽
  if (e.button !== 0) return;
  const target = e.target as HTMLElement;
  // 如果点在了输入框或明确标记了不可拖拽的组件上，不拖拽
  if (target.tagName.toLowerCase() === 'input' || target.closest('[data-tauri-no-drag]')) {
    return;
  }
  const win = getCurrentWindow();
  await win.startDragging();
}

async function minimizeWindow() {
  await getCurrentWindow().minimize();
}

async function closeWindow() {
  await getCurrentWindow().hide();
}

// 动态调整 Tauri 物理窗口大小
async function updateWindowBounds(expanded: boolean) {
  try {
    const win = getCurrentWindow();
    // macOS 上 resizable 为 false 时可能无法调用 setSize，需临时放开限制
    await win.setResizable(true);
    
    if (expanded) {
      await win.setSize(new LogicalSize(720, 508)); 
    } else {
      await win.setSize(new LogicalSize(720, 130)); 
    }
    
    await win.setResizable(false);
  } catch (err) {
    console.warn('[TabPilot] Failed to resize window:', err);
  }
}

watch(() => route.path, (newPath) => {
  updateWindowBounds(newPath !== '/');
});

// IME 合成期 (输入中文/日文/韩文) 按 Enter 不应提交, 等用户敲第二次 Enter 确认候选
function onEnter(e: KeyboardEvent) {
  // isComposing === true: IME 合成中; keyCode 229: 同上 (兼容老浏览器/某些 IME)
  if (e.isComposing || (e as any).keyCode === 229) return;
  handleCommand();
}

async function handleCommand() {
  const val = cmd.value.trim();
  if (!val) return;

  if (val.toLowerCase() === 'settings' || val.toLowerCase() === 's') {
    router.push('/settings');
    cmd.value = '';
    return;
  }

  if (val.toLowerCase() === 'logs' || val.toLowerCase() === 'l') {
    router.push('/logs');
    cmd.value = '';
    return;
  }

  // 清除历史穿梭指针（发送后不再属于"翻旧"状态）
  chat.resetHistory();

  // 跳到执行面板（RunView 挂载后会订阅 chat.steps）
  if (route.path !== '/run') {
    await router.push('/run');
  }

  // 发送到 Agent
  chat.sendMessage(val);
  cmd.value = '';
}

/**
 * Esc 语义（分层）：
 *   1. 输入框非空 → 清空输入
 *   2. 在 /run 且 streaming → 仅 hide 窗口（Agent 后台继续）
 *   3. 其他路由 → 回到 /
 *   4. 已在 / → hide
 */
async function handleEsc() {
  if (cmd.value) {
    cmd.value = '';
    chat.resetHistory();
    return;
  }
  inputRef.value?.blur();

  if (route.path === '/run') {
    // 仅断开 SSE 连接，Agent 后台继续
    chat.detachStream();
    await getCurrentWindow().hide();
    return;
  }

  if (route.path !== '/') {
    router.push('/');
    return;
  }

  await getCurrentWindow().hide();
}

/** Cmd+. / Ctrl+. → 真正终止 Agent */
async function handleStop() {
  await chat.stop();
}

/**
 * ↑ 键穿梭历史会话：仅在输入框为空时生效
 * 首次按 ↑ 会异步拉取会话列表
 */
async function handleArrowUp() {
  if (cmd.value) return; // 有内容时让原生光标移动
  const target = await chat.historyUp();
  if (target) {
    cmd.value = target.last_user_message || target.title || '';
    chat.currentSessionId = target.id;
    // 只预填输入框，不立即发送
    // 用户可以编辑后 Enter 续发
    await nextTick();
    inputRef.value?.focus();
    inputRef.value?.select();
  }
}

async function handleArrowDown() {
  if (cmd.value && chat.historyIndex === -1) return;
  const target = await chat.historyDown();
  if (target) {
    cmd.value = target.last_user_message || target.title || '';
    chat.currentSessionId = target.id;
    await nextTick();
    inputRef.value?.focus();
    inputRef.value?.select();
  } else {
    // 回到"新会话"输入态
    cmd.value = '';
    chat.resetHistory();
  }
}

onMounted(async () => {
  await store.fetchStatus(true);
  store.fetchLogs(10);
  
  updateWindowBounds(route.path !== '/');
  
  if (inputRef.value) {
    inputRef.value.focus();
  }
  
  const handleKeydown = (e: KeyboardEvent) => {
    // 拦截 Cmd+L 或 Ctrl+L 跳转日志
    if (e.key.toLowerCase() === 'l' && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      router.push(route.path === '/logs' ? '/' : '/logs');
    }
    // 拦截 Cmd+S 或 Ctrl+S 跳转设置
    if (e.key.toLowerCase() === 's' && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      router.push(route.path === '/settings' ? '/' : '/settings');
    }
    // 拦截 Esc（当焦点不在 input 上时，补位兜底）
    if (e.key === 'Escape' && document.activeElement?.tagName !== 'INPUT') {
      handleEsc();
    }
    // Cmd+. / Ctrl+. 终止 Agent
    if (e.key === '.' && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      handleStop();
    }
  };
  window.addEventListener('keydown', handleKeydown);
  onUnmounted(() => window.removeEventListener('keydown', handleKeydown));

  try {
    const { listen } = await import('@tauri-apps/api/event');
    listen('pilot-auth-success', async () => {
      await store.refresh();
      router.push('/');
    });
  } catch {}

  // 窗口获得焦点时：若在 /run 且存在未完流，尝试重连
  const onFocus = () => {
    if (route.path === '/run') chat.tryReconnect();
  };
  window.addEventListener('focus', onFocus);
  onUnmounted(() => window.removeEventListener('focus', onFocus));
});
</script>

<style scoped>
/* 全局壳：用来做内边距，给窗口阴影留出空间 */
/* 全局包裹，极简无边距 */
.omni-wrapper {
  width: 100vw;
  height: 100vh;
  display: flex;
  align-items: flex-start;
  justify-content: center;
  padding: 0;
}

/* 核心面板：亚克力玻璃质感 */
.omni-panel {
  width: 100%;
  max-width: 100%;
  height: 100%;
  background: rgba(255, 255, 255, 0.90);
  backdrop-filter: blur(24px) saturate(180%);
  -webkit-backdrop-filter: blur(24px) saturate(180%);
  border-radius: 12px;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.omni-header {
  display: flex;
  align-items: center;
  padding: 12px 16px;
  border-bottom: 1px solid rgba(0, 0, 0, 0.12);
  flex-shrink: 0;
  height: 56px;
}

.mac-titlebar {
  width: 100%;
  height: 28px;
  flex-shrink: 0;
  cursor: grab;
  /* 使其表现出原生标题栏的居中效果 */
  display: flex;
  align-items: center;
  justify-content: center;
}

.titlebar-text {
  font-size: 13px;
  font-weight: 600;
  color: rgba(0, 0, 0, 0.45);
  font-family: -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
  letter-spacing: 0.02em;
  user-select: none;
}

.mac-titlebar:active {
  cursor: grabbing;
}

.omni-header:last-child {
  border-bottom: none;
}

.header-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-left: 16px;
}

.header-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border-radius: 6px;
  color: #6b7280;
  cursor: pointer;
  transition: all 0.2s;
}

.header-btn:hover {
  background: rgba(0, 0, 0, 0.05);
  color: #111827;
}

.header-btn.active {
  background: rgba(0, 0, 0, 0.08);
  color: #111827;
}

.brand-badge {
  width: 24px;
  height: 24px;
  margin-right: 12px;
  transition: opacity 0.2s;
  cursor: pointer;
}

.omni-model {
  margin-right: 8px;
  flex-shrink: 0;
}

.brand-badge:hover {
  opacity: 0.8;
}

.omni-input {
  flex: 1;
  background: transparent;
  border: none;
  outline: none;
  font-size: 18px;
  font-weight: 500;
  color: #111827;
  font-family: -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
  letter-spacing: -0.01em;
}

.omni-input::placeholder {
  color: #9ca3af;
  font-weight: 400;
}

/* 授权提示状态 */
.omni-auth-trigger {
  flex: 1;
  display: flex;
  align-items: center;
  height: 100%;
  cursor: pointer;
}

.auth-placeholder-btn {
  display: flex;
  align-items: center;
  gap: 8px;
  background: rgba(0, 0, 0, 0.04);
  color: #4b5563;
  padding: 8px 14px;
  border-radius: 8px;
  font-size: 14px;
  font-weight: 500;
  border: 1px solid rgba(0, 0, 0, 0.06);
  transition: all 0.2s;
}

.omni-auth-trigger:hover .auth-placeholder-btn {
  background: rgba(0, 0, 0, 0.08);
  color: #111827;
}

/* 状态灯 */
.omni-status {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: #d1d5db;
  margin-left: 16px;
  flex-shrink: 0;
}

.omni-status.connected {
  background: #10b981;
  box-shadow: 0 0 8px rgba(16, 185, 129, 0.6);
}

.omni-status.reconnecting {
  background: #f59e0b;
  box-shadow: 0 0 8px rgba(245, 158, 11, 0.6);
  animation: pulse 1.5s infinite;
}

/* 独立路由区 */
.omni-router-view {
  flex: 1;
  display: flex;
  overflow: auto;
  background: rgba(255, 255, 255, 0.7);
  /* 确保内部滚动平滑 */
}

/* 简化的内容执行区 */
.omni-content {
  padding: 16px 20px;
  background: rgba(249, 250, 251, 0.6);
  display: flex;
  flex-direction: column;
  flex-shrink: 0;
}

.section-title {
  font-size: 10px;
  font-weight: 700;
  color: #6b7280;
  letter-spacing: 0.1em;
  margin-bottom: 12px;
}

.log-stream {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.stream-item {
  display: flex;
  align-items: center;
  gap: 12px;
  font-family: "JetBrains Mono", "SF Mono", monospace;
  font-size: 12px;
  color: #374151;
  background: #ffffff;
  padding: 8px 12px;
  border-radius: 6px;
  border: 1px solid rgba(0, 0, 0, 0.1);
  box-shadow: 0 1px 2px rgba(0,0,0,0.02);
}

.stream-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  flex-shrink: 0;
}
.stream-dot.green { background: #10b981; }
.stream-dot.red { background: #ef4444; }
.stream-dot.yellow { background: #f59e0b; }

.stream-agent {
  font-weight: 700;
  color: #111827;
}

.stream-args {
  color: #6b7280;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 400px;
}

.stream-empty {
  font-size: 13px;
  color: #6b7280;
  padding: 12px 0;
  text-align: center;
}

.divider-vertical {
  width: 1px;
  height: 16px;
  background: var(--border);
  margin: 0 4px;
}

/* 底部状态条 (常驻) */
.omni-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 20px 16px 20px;
  background: rgba(249, 250, 251, 0.6);
  border-top: 1px solid rgba(0, 0, 0, 0.08);
  font-size: 13px;
  color: #6b7280;
  flex-shrink: 0;
}

.footer-left {
  display: flex;
  align-items: center;
  gap: 12px;
}

.footer-link {
  color: #4b5563;
  cursor: pointer;
  transition: color 0.2s;
  font-weight: 500;
}

.footer-link:hover {
  color: #111827;
}

.footer-right {
  display: flex;
  align-items: center;
  gap: 12px;
}

.divider {
  color: #d1d5db;
}

.sys-hint {
  font-size: 12px;
  color: #9ca3af;
}

.color-red {
  color: #ef4444;
}
.color-accent {
  color: var(--accent);
  font-weight: 600;
}

@keyframes pulse {
  0% { transform: scale(1); opacity: 1; }
  50% { transform: scale(1.1); opacity: 0.7; }
  100% { transform: scale(1); opacity: 1; }
}
</style>
