/**
 * TabPilot 状态 Store
 *
 * 全局缓存 Pilot 状态，避免切页面重复请求
 * - 默认 10s 内复用缓存
 * - 手动 refresh 强制刷新
 */

import { defineStore } from 'pinia';
import { ref, computed } from 'vue';
import { getStatus, getLogs, type StatusResponse, type LogEntry } from './bridge';
import { GUARD_MODE_LABELS, GUARD_MODES } from '../constants/guardModes';

const CACHE_TTL = 10_000; // 10s

export const usePilotStore = defineStore('pilot', () => {
  // ── 状态 ──
  const status = ref<StatusResponse | null>(null);
  const logs = ref<LogEntry[]>([]);
  const lastFetchAt = ref(0);
  const loading = ref(false);

  // ── 计算属性 ──
  const isConnected = computed(() => status.value?.connected ?? false);
  const serverReachable = computed(() => status.value?.server_reachable ?? false);
  const wsState = computed(() => status.value?.ws_state ?? 'disconnected');
  const uptime = computed(() => status.value?.uptime ?? 0);
  // guard_mode 由后端 get_status 接口权威返回; 未加载时空串, UI 走"加载中"态.
  const guardMode = computed(() => status.value?.guard_mode ?? '');
  const guardModeText = computed(() => GUARD_MODE_LABELS[guardMode.value] || guardMode.value);
  const guardModeInfo = computed(() => GUARD_MODES.find(m => m.value === guardMode.value));
  const workspace = computed(() => status.value?.workspace ?? '');
  const serverUrl = computed(() => status.value?.server_url ?? '');
  const version = computed(() => status.value?.version ?? 'unknown');
  const browserEnabled = computed(() => status.value?.browser_enabled ?? true);
  const auditEnabled = computed(() => status.value?.audit_enabled ?? true);
  const userDisplay = computed(() => status.value?.user_display ?? '');
  const userId = computed(() => status.value?.user_id ?? '');
  const toolsReady = computed(() => status.value?.tools_ready ?? false);
  const toolNames = computed(() => status.value?.tool_names ?? []);

  // ── 方法 ──

  /** 获取状态 (带缓存) */
  async function fetchStatus(force = false) {
    const now = Date.now();
    if (!force && status.value && now - lastFetchAt.value < CACHE_TTL) {
      return; // 缓存有效
    }

    loading.value = true;
    try {
      status.value = await getStatus();
      lastFetchAt.value = Date.now();
    } catch (e) {
      console.error('[PilotStore] 获取状态失败:', e);
    } finally {
      loading.value = false;
    }
  }

  /** 强制刷新 */
  async function refresh() {
    await fetchStatus(true);
  }

  /** 统一设置: 执行 action → 立即刷新 status */
  async function setSetting(action: () => Promise<unknown>) {
    try {
      await action();
      await fetchStatus(true);  // 强制刷新
    } catch (e) {
      console.error('[PilotStore] 设置失败:', e);
    }
  }

  /** 获取日志 */
  async function fetchLogs(limit = 10) {
    try {
      logs.value = await getLogs(limit);
    } catch (e) {
      console.error('[PilotStore] 获取日志失败:', e);
    }
  }

  /** 重置缓存 (登出时调用) */
  function reset() {
    status.value = null;
    logs.value = [];
    lastFetchAt.value = 0;
  }

  // ── 自动刷新 (15s, 仅 app 可见时) ──
  let autoRefreshTimer: ReturnType<typeof setInterval> | null = null;

  function startAutoRefresh() {
    if (autoRefreshTimer) return;
    autoRefreshTimer = setInterval(() => {
      if (!document.hidden) {
        fetchStatus(true);
      }
    }, 15_000);

    // 初次加载 2s 后重试 (等 Connector health check 完成)
    setTimeout(() => fetchStatus(true), 2000);

    // focus 时立即刷新
    window.addEventListener('focus', refresh);
    // 页面可见时刷新
    document.addEventListener('visibilitychange', () => {
      if (!document.hidden) refresh();
    });
  }

  function stopAutoRefresh() {
    if (autoRefreshTimer) {
      clearInterval(autoRefreshTimer);
      autoRefreshTimer = null;
    }
    window.removeEventListener('focus', refresh);
  }

  // 首次使用 store 时自动启动
  startAutoRefresh();

  return {
    // 状态
    status, logs, loading,
    // 计算
    isConnected, serverReachable, wsState, uptime,
    guardMode, guardModeText, guardModeInfo,
    workspace, serverUrl, version,
    browserEnabled, auditEnabled,
    userDisplay, userId, toolsReady, toolNames,
    // 方法
    fetchStatus, refresh, fetchLogs, reset, setSetting,
    startAutoRefresh, stopAutoRefresh,
  };
});
