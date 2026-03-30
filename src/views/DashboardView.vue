<template>
  <div class="dashboard">
    <!-- 版本更新提示 -->
    <UpdateBanner />

    <!-- 状态卡片 -->
    <div class="status-grid">
      <!-- 大面积的连接状态主卡片 -->
      <div class="card status-hero-card">
        <div class="status-indicator">
          <div class="status-ring" :class="statusClass"></div>
          <div class="status-core" :class="statusClass"></div>
        </div>
        <div class="status-info">
          <div class="status-label">当前运行状态：</div>
          <div class="status-value" :class="statusClass">{{ statusText }}</div>
          <div v-if="store.userDisplay" class="status-user">{{ store.userDisplay }}</div>
        </div>
        <div class="status-actions">
          <button
            v-if="store.serverReachable && !store.isConnected"
            class="btn btn-primary btn-sm"
            @click="login"
          >
            去授权
          </button>
          <button class="btn btn-ghost btn-sm btn-icon-only" @click="refresh" title="刷新">
            <RefreshCw :size="16" />
          </button>
        </div>
      </div>

      <!-- 数据小指标 -->
      <div class="card metric-card">
        <div class="metric-header">
          <div class="metric-icon bg-blue-subtle">
            <Timer :size="16" class="color-blue" />
          </div>
          <div class="metric-label">持续运行</div>
        </div>
        <div class="metric-value">{{ uptimeText }}</div>
      </div>

      <div class="card metric-card">
        <div class="metric-header">
          <div class="metric-icon bg-green-subtle">
            <Activity :size="16" class="color-green" />
          </div>
          <div class="metric-label">今日操作</div>
        </div>
        <div class="metric-value">{{ store.logs.length }}</div>
      </div>

      <div class="card metric-card">
        <div class="metric-header">
          <div class="metric-icon" :class="guardModeBg">
            <component :is="guardModeIcon" :size="16" :class="guardModeColor" />
          </div>
          <div class="metric-label">安全模式</div>
        </div>
        <div class="metric-value text-capitalize">{{ store.guardModeText }}</div>
      </div>
    </div>

    <!-- 最近操作 -->
    <div class="card recent-card">
      <div class="card-title justify-between">
        <span>最新操作追踪</span>
        <button class="btn btn-ghost btn-sm" @click="$router.push('/logs')">查看全部</button>
      </div>
      
      <div v-if="store.logs.length === 0" class="empty-state">
        <Box :size="32" stroke-width="1.5" />
        <p>暂无操作记录，Agent 尚未执行任何任务</p>
      </div>

      <div v-else class="log-list">
        <div
          v-for="(log, i) in store.logs"
          :key="i"
          class="log-row"
        >
          <span class="log-time mono">{{ formatLogTime(log.timestamp as number) }}</span>
          <span class="log-badge" :class="logBadgeClass(log.tool_type as string)">{{ log.tool_type }}</span>
          <span class="log-action mono">
            <span class="log-action-label">{{ log.action }}</span>
            <span v-if="formatArgs(log.args_json as string)" class="log-action-args">{{ formatArgs(log.args_json as string) }}</span>
          </span>
          <span class="log-decision" :class="log.guard_decision">
            <CheckCircle2 v-if="log.guard_decision === 'allow'" :size="12" />
            <XCircle v-else-if="log.guard_decision === 'deny'" :size="12" />
            <Clock v-else :size="12" />
          </span>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted } from 'vue';
import { usePilotStore } from '../services/pilotStore';
import { GUARD_MODES } from '../constants/guardModes';
import UpdateBanner from '../components/UpdateBanner.vue';
import { 
  RefreshCw, Power, Timer, Activity, ShieldCheck, 
  CheckCircle2, XCircle, Clock, Box 
} from 'lucide-vue-next';

const store = usePilotStore();

const currentModeInfo = computed(() => GUARD_MODES.find(m => m.value === store.guardMode));
const guardModeIcon = computed(() => currentModeInfo.value?.icon);
const guardModeColor = computed(() => currentModeInfo.value?.colorClass || 'color-yellow');
const guardModeBg = computed(() => {
  const c = guardModeColor.value;
  if (c === 'color-red') return 'bg-red-subtle';
  if (c === 'color-blue') return 'bg-blue-subtle';
  if (c === 'color-green') return 'bg-green-subtle';
  return 'bg-yellow-subtle';
});

const statusClass = computed(() => ({
  connected: store.isConnected,
  reachable: !store.isConnected && store.serverReachable,
  reconnecting: !store.isConnected && store.wsState === 'connecting',
  disconnected: !store.isConnected && !store.serverReachable
}));

const statusText = computed(() => {
  if (store.isConnected) return '已连接并就绪';
  if (store.serverReachable) return '服务在线 · 等待授权';
  if (store.wsState === 'connecting') return '正在极速重连...';
  return '服务不可达';
});

const uptimeText = computed(() => {
  if (store.uptime <= 0) return '0 m';
  const h = Math.floor(store.uptime / 3600);
  const m = Math.floor((store.uptime % 3600) / 60);
  return h > 0 ? `${h} h ${m} m` : `${m} m`;
});

function formatLogTime(ts: number): string {
  if (!ts) return '';
  const d = new Date(ts * 1000);
  return `${d.getHours().toString().padStart(2, '0')}:${d.getMinutes().toString().padStart(2, '0')}:${d.getSeconds().toString().padStart(2, '0')}`;
}

const BADGE_CLASSES: Record<string, string> = {
  shell: 'badge-blue',
  browser: 'badge-teal',
  file: 'badge-purple',
  mcp: 'badge-orange',
};
function logBadgeClass(toolType: string): string {
  return BADGE_CLASSES[toolType] || 'badge-purple';
}

function formatArgs(argsJson: string | null | undefined): string {
  if (!argsJson) return '';
  try {
    const parsed = JSON.parse(argsJson);
    if (typeof parsed === 'object' && parsed !== null) {
      if (parsed.command) {
        const cmd = String(parsed.command);
        return cmd.length > 60 ? cmd.slice(0, 60) + '…' : cmd;
      }
      if (parsed.path) return String(parsed.path);
      if (parsed.url) {
        const url = String(parsed.url);
        return url.length > 60 ? url.slice(0, 60) + '…' : url;
      }
      for (const val of Object.values(parsed)) {
        if (typeof val === 'string' && val.length > 0) {
          return val.length > 60 ? val.slice(0, 60) + '…' : val;
        }
      }
    }
  } catch { /* ignore */ }
  return '';
}

async function refresh() {
  await store.refresh();
  await store.fetchLogs(50);
}

async function login() {
  let challenge = '';
  try {
    const { getAuthChallenge } = await import('@/services/bridge');
    challenge = await getAuthChallenge();
  } catch { /* ignore */ }

  const base = store.serverUrl
    .replace(/^wss:\/\//, 'https://')
    .replace(/^ws:\/\//, 'http://')
    .replace(/\/ws\/.*$/, '');

  const authUrl = `${base}/auth/pilot?challenge=${challenge}`;
  try {
    const { open } = await import('@tauri-apps/plugin-shell');
    await open(authUrl);
  } catch {
    window.open(authUrl, '_blank');
  }

  // 通知 Rust 后端开始轮询 auth-poll (deep link 和 poll 双通道)
  if (challenge) {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('start_auth_poll', { challenge });
    } catch { /* ignore */ }
  }
}

// 定时轮询 (5s) 实时更新日志
let pollTimer: ReturnType<typeof setInterval> | null = null;

onMounted(() => {
  store.fetchStatus();
  store.fetchLogs(50);
  pollTimer = setInterval(() => {
    store.fetchLogs(50);
  }, 5000);
});

onUnmounted(() => {
  if (pollTimer) clearInterval(pollTimer);
});
</script>

<style scoped>
.dashboard {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-height: 0;
  animation: fadeIn 0.3s ease;
}

@keyframes fadeIn {
  from { opacity: 0; transform: translateY(4px); }
  to { opacity: 1; transform: translateY(0); }
}

/* 状态网格 */
.status-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  flex-shrink: 0;
  gap: 12px;
  margin-bottom: 12px;
}

/* 核心大卡片 */
.status-hero-card {
  grid-column: span 3;
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 16px 20px;
  background: linear-gradient(145deg, var(--bg-card), var(--bg-card-hover));
}

.status-indicator {
  position: relative;
  width: 44px;
  height: 44px;
  display: flex;
  align-items: center;
  justify-content: center;
}

.status-ring {
  position: absolute;
  inset: 0;
  border-radius: 50%;
  opacity: 0.2;
}

.status-core {
  width: 16px;
  height: 16px;
  border-radius: 50%;
  position: relative;
  z-index: 2;
  box-shadow: 0 0 12px currentColor;
  background: currentColor;
}

.connected.status-ring { background: var(--green); animation: pulse-ring 2s infinite cubic-bezier(0.4, 0, 0.6, 1); }
.connected.status-core { color: var(--green); }

.reachable.status-ring { background: var(--yellow); animation: pulse-ring 2s infinite cubic-bezier(0.4, 0, 0.6, 1); }
.reachable.status-core { color: var(--yellow); }

.disconnected.status-ring { background: var(--red); opacity: 0.1; }
.disconnected.status-core { color: var(--red); opacity: 0.8; box-shadow: none; }

.reconnecting.status-ring { background: var(--yellow); animation: spin 1.5s linear infinite; border: 2px dashed var(--yellow); border-radius: 50%; }
.reconnecting.status-core { background: var(--yellow); color: var(--yellow); }

@keyframes pulse-ring {
  0% { transform: scale(0.8); opacity: 0.4; }
  100% { transform: scale(1.5); opacity: 0; }
}

@keyframes spin {
  100% { transform: rotate(360deg); }
}

.status-info {
  flex: 1;
  display: flex;
  align-items: center;
  gap: 8px;
}

.status-label {
  font-size: 14px;
  font-weight: 500;
  color: var(--text-secondary);
}

.status-value {
  font-size: 18px;
  font-weight: 700;
  letter-spacing: -0.01em;
}

.status-value.connected { color: var(--text-primary); }
.status-value.reachable { color: var(--yellow); }
.status-value.disconnected { color: var(--text-tertiary); }
.status-value.reconnecting { color: var(--yellow); }

.status-user {
  font-size: 12px;
  color: var(--text-tertiary);
  margin-top: 2px;
  font-family: 'SF Mono', 'Menlo', monospace;
  letter-spacing: 0.5px;
}

.status-actions {
  display: flex;
  gap: 12px;
}

.btn-icon-only {
  padding: 4px;
  width: 28px;
  height: 28px;
  display: flex;
  align-items: center;
  justify-content: center;
}

/* 小指标卡片 */
.metric-card {
  padding: 12px 16px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.metric-header {
  display: flex;
  align-items: center;
  gap: 10px;
}

.metric-icon {
  width: 24px;
  height: 24px;
  border-radius: 6px;
  display: flex;
  align-items: center;
  justify-content: center;
}

.bg-blue-subtle { background: rgba(51, 112, 255, 0.1); }
.bg-green-subtle { background: var(--green-dim); }
.bg-yellow-subtle { background: var(--yellow-dim); }

.color-blue { color: var(--blue); }
.color-green { color: var(--green); }
.color-yellow { color: var(--yellow); }

.metric-label {
  font-size: 13px;
  color: var(--text-secondary);
  font-weight: 500;
}

.metric-value {
  font-size: 18px;
  font-weight: 700;
  color: var(--text-primary);
  font-family: var(--font-mono);
}

.text-capitalize { text-transform: capitalize; }

/* 最近操作 — 紧凑一行 */
.recent-card {
  padding: 16px;
  flex: 1;
  display: flex;
  flex-direction: column;
  min-height: 280px;
  margin-bottom: 0px;
}

.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 24px 0;
  color: var(--text-tertiary);
  gap: 12px;
  font-size: 13px;
}

.log-list {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  gap: 1px;
}

.log-row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 6px 8px;
  border-radius: 6px;
  transition: background 0.15s;
}

.log-row:hover {
  background: var(--bg-hover);
}

.log-time {
  font-size: 11px;
  color: var(--text-tertiary);
  flex-shrink: 0;
  width: 60px;
}

.log-badge {
  display: inline-flex;
  align-items: center;
  padding: 1px 6px;
  border-radius: 3px;
  font-size: 10px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  flex-shrink: 0;
}

.badge-blue { background: rgba(51, 112, 255, 0.1); color: var(--blue); }
.badge-purple { background: rgba(147, 51, 234, 0.1); color: #9333ea; }
.badge-teal { background: rgba(20, 184, 166, 0.1); color: #14b8a6; }
.badge-orange { background: rgba(249, 115, 22, 0.1); color: #f97316; }

.log-action {
  flex: 1;
  font-size: 12px;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
}

.log-action-label {
  font-weight: 600;
  margin-right: 6px;
}

.log-action-args {
  color: var(--text-tertiary);
  font-weight: 400;
}

.log-decision {
  flex-shrink: 0;
  display: flex;
  align-items: center;
}

.log-decision.allow { color: var(--green); }
.log-decision.deny { color: var(--red); }
.log-decision.confirm { color: var(--yellow); }
</style>
