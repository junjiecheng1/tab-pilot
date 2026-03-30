<template>
  <div class="terminal-view">
    <!-- Tab 栏 -->
    <div class="term-tabs">
      <div class="tab-list">
        <button
          v-for="s in sessions"
          :key="s.id"
          class="tab-item"
          :class="{ active: activeId === s.id }"
          @click="activeId = s.id"
          :title="s.label"
        >
          <span class="tab-dot" :class="{ main: s.is_main }"></span>
          <span class="tab-label">{{ s.label }}</span>
          <span class="tab-age">{{ formatAge(s.age_seconds) }}</span>
          <button
            v-if="!s.is_main"
            class="tab-close"
            @click.stop="killSession(s.id)"
            title="终止"
          >
            <X :size="12" />
          </button>
        </button>
      </div>
      <div class="tab-right">
        <span class="session-count" v-if="sessions.length">
          {{ sessions.length }} 个终端
        </span>
      </div>
    </div>

    <!-- 终端显示区 -->
    <div class="term-body" v-if="sessions.length > 0">
      <div ref="termContainer" class="xterm-container"></div>
    </div>

    <!-- 空状态 -->
    <div class="term-empty" v-else>
      <TerminalSquare :size="48" stroke-width="1" class="empty-icon" />
      <p class="empty-text">无活跃终端</p>
      <p class="empty-hint">Agent 执行 Shell 命令时，终端将显示在此处</p>
    </div>

    <!-- 命令测试栏 -->
    <div class="cmd-bar">
      <div class="cmd-input-wrap">
        <span class="cmd-prompt">$</span>
        <input
          v-model="cmdInput"
          class="cmd-input"
          placeholder="输入命令测试... (如: which markitdown)"
          @keydown.enter="runCommand"
          :disabled="cmdRunning"
        />
        <button class="cmd-run" @click="runCommand" :disabled="cmdRunning || !cmdInput.trim()">
          {{ cmdRunning ? '运行中...' : '执行' }}
        </button>
      </div>
      <pre v-if="cmdOutput" class="cmd-output">{{ cmdOutput }}</pre>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch, nextTick } from 'vue';
import { X, TerminalSquare } from 'lucide-vue-next';
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';
import {
  listShellSessions,
  readShellOutput,
  killShellSession,
  execShellCommand,
  type ShellSessionInfo,
} from '../services/bridge';

const sessions = ref<ShellSessionInfo[]>([]);
const activeId = ref('');
const termContainer = ref<HTMLElement | null>(null);

// 命令测试
const cmdInput = ref('');
const cmdOutput = ref('');
const cmdRunning = ref(false);

async function runCommand() {
  const cmd = cmdInput.value.trim();
  if (!cmd || cmdRunning.value) return;
  cmdRunning.value = true;
  cmdOutput.value = `$ ${cmd}\n...\n`;
  try {
    const result = await execShellCommand(cmd, 120);
    if (result) {
      cmdOutput.value = `$ ${cmd}\n`
        + `exit_code: ${result.exit_code}  active: ${result.active}  sid: ${result.session_id}\n`
        + `─────────────────────────────\n`
        + (result.output || '(无输出)');
    } else {
      cmdOutput.value = `$ ${cmd}\n(invoke 返回 null)`;
    }
  } catch (e) {
    cmdOutput.value = `$ ${cmd}\n❌ ${e}`;
  } finally {
    cmdRunning.value = false;
  }
}

let term: Terminal | null = null;
let fitAddon: FitAddon | null = null;
let pollTimer: ReturnType<typeof setInterval> | null = null;
let outputTimer: ReturnType<typeof setInterval> | null = null;

// 每个 session 的历史输出缓存 (切 tab 时保留)
const outputCache = new Map<string, string>();

onMounted(async () => {
  initTerminal();
  await fetchSessions(); // 立即拉一次, 不等 interval
  startPolling();
});

onUnmounted(() => {
  if (pollTimer) clearInterval(pollTimer);
  if (outputTimer) clearInterval(outputTimer);
  term?.dispose();
});

// 切换 tab 时重新渲染
watch(activeId, (newId, oldId) => {
  if (!term) return;
  term.clear();
  // 写入该 session 的历史输出
  const cached = outputCache.get(newId);
  if (cached) {
    term.write(cached);
  }
});

function initTerminal() {
  if (!termContainer.value) return;

  term = new Terminal({
    disableStdin: true,
    cursorBlink: false,
    fontSize: 12,
    fontFamily: 'SF Mono, Fira Code, JetBrains Mono, Consolas, monospace',
    lineHeight: 1.4,
    scrollback: 5000,
    theme: {
      background: '#0a0a0c',
      foreground: '#d4d4d8',
      cursor: 'transparent',
      selectionBackground: 'rgba(51, 112, 255, 0.3)',
    },
  });

  fitAddon = new FitAddon();
  term.loadAddon(fitAddon);
  term.open(termContainer.value);
  fitAddon.fit();

  // 监听窗口大小变化
  const ro = new ResizeObserver(() => fitAddon?.fit());
  ro.observe(termContainer.value);
}

async function fetchSessions() {
  const list = await listShellSessions();
  // 主终端永远在第一个
  list.sort((a, b) => (a.is_main === b.is_main ? 0 : a.is_main ? -1 : 1));
  sessions.value = list;

  // 清理已消失 session 的缓存
  const ids = new Set(list.map(s => s.id));
  for (const key of outputCache.keys()) {
    if (!ids.has(key)) outputCache.delete(key);
  }

  // 自动选中第一个 (或新出现的)
  if (list.length > 0 && (!activeId.value || !ids.has(activeId.value))) {
    activeId.value = list[list.length - 1].id;
    await nextTick();
    if (!term && termContainer.value) {
      initTerminal();
    }
  }
  if (list.length === 0) {
    activeId.value = '';
  }
}

function startPolling() {
  // 每 3s 刷新 session 列表
  pollTimer = setInterval(fetchSessions, 3000);

  // 每 500ms 读当前 tab 的输出
  outputTimer = setInterval(async () => {
    if (!activeId.value || !term) return;
    try {
      const output = await readShellOutput(activeId.value);
      if (output) {
        term.write(output);
        // 追加到缓存
        const prev = outputCache.get(activeId.value) ?? '';
        outputCache.set(activeId.value, prev + output);
      }
    } catch {
      // session 可能已结束
    }
  }, 500);
}

async function killSession(id: string) {
  await killShellSession(id);
  outputCache.delete(id);
  sessions.value = sessions.value.filter(s => s.id !== id);
  if (activeId.value === id) {
    activeId.value = sessions.value[0]?.id ?? '';
  }
}

function formatAge(seconds: number): string {
  if (seconds < 60) return `${seconds}s`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m`;
  return `${Math.floor(seconds / 3600)}h`;
}
</script>

<style scoped>
.terminal-view {
  display: flex;
  flex-direction: column;
  position: absolute;
  inset: 0;
  background: var(--bg-app);
  overflow: hidden;
}

/* ── Tab 栏 ── */
.term-tabs {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 4px 12px;
  background: var(--bg-card);
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
  min-height: 36px;
}

.tab-list {
  display: flex;
  gap: 2px;
  overflow-x: auto;
  flex: 1;
}

.tab-list::-webkit-scrollbar { height: 0; }

.tab-item {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 10px;
  border: none;
  background: transparent;
  border-radius: 6px;
  cursor: pointer;
  font-size: 11px;
  font-family: var(--font-mono);
  color: var(--text-tertiary);
  white-space: nowrap;
  transition: all 0.15s;
}

.tab-item:hover {
  background: var(--bg-hover);
  color: var(--text-secondary);
}

.tab-item.active {
  background: var(--bg-active);
  color: var(--accent);
}

.tab-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--green);
  flex-shrink: 0;
  box-shadow: 0 0 4px var(--green);
}

.tab-dot.main {
  background: var(--accent);
  box-shadow: 0 0 4px var(--accent);
}

.tab-label {
  max-width: 160px;
  overflow: hidden;
  text-overflow: ellipsis;
}

.tab-age {
  color: var(--text-tertiary);
  font-size: 10px;
  opacity: 0.7;
}

.tab-close {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 16px;
  height: 16px;
  border: none;
  background: transparent;
  border-radius: 3px;
  cursor: pointer;
  color: var(--text-tertiary);
  opacity: 0;
  transition: all 0.15s;
}

.tab-item:hover .tab-close { opacity: 1; }
.tab-close:hover {
  background: var(--red-dim);
  color: var(--red);
}

.tab-right {
  flex-shrink: 0;
  padding-left: 8px;
}

.session-count {
  font-size: 11px;
  color: var(--text-tertiary);
}

/* ── 终端区 ── */
.term-body {
  flex: 1;
  background: var(--bg-terminal);
  overflow: hidden;
}

.xterm-container {
  width: 100%;
  height: 100%;
  padding: 8px;
}

/* ── 空状态 ── */
.term-empty {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
}

.empty-icon {
  color: var(--text-tertiary);
  opacity: 0.15;
  animation: breathe 4s ease-in-out infinite;
}

@keyframes breathe {
  0%, 100% { opacity: 0.15; transform: scale(1); }
  50% { opacity: 0.3; transform: scale(1.08); }
}

.empty-text {
  font-size: 14px;
  font-weight: 500;
  color: var(--text-tertiary);
}

.empty-hint {
  font-size: 12px;
  color: var(--text-tertiary);
  opacity: 0.6;
}

/* ── 命令测试栏 ── */
.cmd-bar {
  flex-shrink: 0;
  border-top: 1px solid var(--border);
  background: var(--bg-card);
  padding: 8px 12px;
}

.cmd-input-wrap {
  display: flex;
  align-items: center;
  gap: 8px;
}

.cmd-prompt {
  color: var(--accent);
  font-family: var(--font-mono);
  font-size: 13px;
  font-weight: 600;
}

.cmd-input {
  flex: 1;
  background: var(--bg-app);
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 6px 10px;
  font-family: var(--font-mono);
  font-size: 12px;
  color: var(--text-primary);
  outline: none;
  transition: border-color 0.15s;
}

.cmd-input:focus {
  border-color: var(--accent);
}

.cmd-input:disabled {
  opacity: 0.5;
}

.cmd-run {
  padding: 5px 14px;
  background: var(--accent);
  color: #fff;
  border: none;
  border-radius: 6px;
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  transition: opacity 0.15s;
}

.cmd-run:hover { opacity: 0.85; }
.cmd-run:disabled { opacity: 0.4; cursor: not-allowed; }

.cmd-output {
  margin-top: 8px;
  padding: 8px 10px;
  background: #0a0a0c;
  border-radius: 6px;
  font-family: var(--font-mono);
  font-size: 11px;
  line-height: 1.5;
  color: var(--text-secondary);
  max-height: 200px;
  overflow-y: auto;
  white-space: pre-wrap;
  word-break: break-all;
}
</style>
