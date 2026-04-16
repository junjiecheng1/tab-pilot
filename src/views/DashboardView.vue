<template>
  <div class="dashboard">
    <div class="page-header">
      <h1 class="page-title">系统概览</h1>
      <div class="header-actions">
        <button
          v-if="store.serverReachable && !store.isConnected"
          class="btn btn-primary btn-sm"
          @click="login"
        >
          系统授权
        </button>
        <button class="btn btn-ghost btn-sm" @click="refresh">
          刷新数据
        </button>
      </div>
    </div>

    <!-- 超大核心指标 -->
    <div class="metrics-overview">
      <div class="metric-super-label">核心运行指标</div>
      <div class="metric-super-value">
        {{ store.logs.length }} <span class="metric-super-sub">/ 今日操作</span>
      </div>
    </div>

    <!-- 各维度数据网格 (无框风格) -->
    <div class="metrics-row">
      <div class="metric-group">
        <div class="metric-label">运行时间</div>
        <div class="metric-value">
          <Timer :size="20" class="value-icon" />
          {{ uptimeText }}
        </div>
      </div>

      <div class="metric-group">
        <div class="metric-label">连接状态</div>
        <div class="metric-value" :class="statusClass">
          <Activity :size="20" class="value-icon" />
          {{ statusText }}
        </div>
      </div>

      <div class="metric-group">
        <div class="metric-label">安全模式</div>
        <div class="metric-value">
          <ShieldCheck :size="20" class="value-icon" />
          <span class="text-capitalize">{{ store.guardModeText }}</span>
        </div>
      </div>

      <div class="metric-group">
        <div class="metric-label">当前账户</div>
        <div class="metric-value">
          {{ store.userDisplay || '—' }}
        </div>
      </div>
    </div>

    <!-- 最新操作记录 -->
    <div class="recent-section">
      <div class="section-header">
        <div class="section-title">操作追踪</div>
        <button class="btn btn-ghost btn-sm" @click="$router.push('/logs')">查看完整记录</button>
      </div>

      <div v-if="store.logs.length === 0" class="empty-state">
        <Box :size="32" stroke-width="1.5" />
        <div>Agent 暂无操作活动记录</div>
      </div>

      <div v-else class="log-list">
        <div v-for="(log, i) in store.logs.slice(0, 8)" :key="i" class="log-row">
          <div class="log-icon-wrap">
            <CheckCircle2 v-if="log.guard_decision === 'allow'" :size="16" class="color-green" />
            <XCircle v-else-if="log.guard_decision === 'deny'" :size="16" class="color-red" />
            <Clock v-else :size="16" class="color-yellow" />
          </div>
          <div class="log-content">
            <div class="log-title">{{ log.action }}</div>
            <div class="log-desc">{{ formatArgs(log.args_json as string) }}</div>
          </div>
          <div class="log-meta">
            {{ formatLogTime(log.timestamp as number) }}
          </div>
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
/* 仪表盘主布局 */
.dashboard {
  flex: 1;
  display: flex;
  flex-direction: column;
  height: 100%;
  padding: 40px 48px;
  animation: fadeIn 0.3s ease;
  overflow-y: auto;
}

@keyframes fadeIn {
  from { opacity: 0; transform: translateY(4px); }
  to { opacity: 1; transform: translateY(0); }
}

/* 顶部标题区 */
.page-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 40px;
}

.page-title {
  font-size: 24px;
  font-weight: 600;
  color: var(--text-primary);
  letter-spacing: -0.02em;
  margin: 0;
}

.header-actions {
  display: flex;
  gap: 12px;
}

/* 数据一览 (无边框大幅数字排版) */
.metrics-overview {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-bottom: 48px;
}

.metric-super-label {
  font-size: 13px;
  font-weight: 500;
  color: var(--text-secondary);
}

.metric-super-value {
  font-size: 56px;
  font-weight: 600;
  color: var(--text-primary);
  letter-spacing: -0.03em;
  line-height: 1;
  display: flex;
  align-items:baseline;
  gap: 12px;
}

.metric-super-sub {
  font-size: 14px;
  font-weight: 500;
  color: var(--green);
  letter-spacing: normal;
}

/* 多维状态网络 */
.metrics-row {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 32px;
  padding-top: 32px;
  border-top: 1px solid var(--border);
  margin-bottom: 48px;
}

.metric-group {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.metric-label {
  font-size: 12px;
  color: var(--text-secondary);
  font-weight: 500;
}

.metric-value {
  font-size: 24px;
  font-weight: 600;
  color: var(--text-primary);
  letter-spacing: -0.02em;
  display: flex;
  align-items: center;
  gap: 8px;
}

.value-icon {
  opacity: 0.8;
}

.status-value.connected { color: var(--text-primary); }
.status-value.reachable { color: var(--yellow); }
.status-value.disconnected { color: var(--text-tertiary); }
.status-value.reconnecting { color: var(--yellow); }

/* 最近操作 */
.recent-section {
  flex: 1;
  display: flex;
  flex-direction: column;
}

.section-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 16px;
}

.section-title {
  font-size: 16px;
  font-weight: 600;
  color: var(--text-primary);
}

.log-list {
  display: flex;
  flex-direction: column;
}

.log-row {
  display: flex;
  align-items: center;
  padding: 12px 0;
  border-bottom: 1px solid var(--border-subtle);
  gap: 16px;
}

.log-row:last-child {
  border-bottom: none;
}

.log-icon-wrap {
  width: 32px;
  height: 32px;
  border-radius: 8px;
  background: var(--bg-sidebar);
  border: 1px solid var(--border);
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--text-secondary);
}

.log-content {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.log-title {
  font-size: 14px;
  font-weight: 500;
  color: var(--text-primary);
}

.log-desc {
  font-size: 13px;
  color: var(--text-tertiary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 600px;
}

.log-meta {
  font-size: 12px;
  color: var(--text-tertiary);
  white-space: nowrap;
}

.empty-state {
  padding: 40px 0;
  color: var(--text-tertiary);
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
  font-size: 14px;
}

.color-green { color: var(--green); }
.color-red { color: var(--red); }
.color-yellow { color: var(--yellow); }
</style>
