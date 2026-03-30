<template>
  <div class="app-shell">
    <!-- 侧边栏导航 -->
    <nav class="sidebar">
      <div class="sidebar-brand" title="TabPilot">
        <img src="/logo.png" alt="TabPilot Logo" class="brand-img" />
      </div>

      <div class="nav-items">
        <router-link
          v-for="item in navItems"
          :key="item.path"
          :to="item.path"
          class="nav-item"
          active-class="active"
          :title="item.label"
        >
          <component :is="item.icon" class="nav-icon" :size="20" stroke-width="2" />
        </router-link>
      </div>

      <!-- 底部状态 -->
      <div class="sidebar-footer">
        <div class="status-indicator" :title="statusText">
          <div class="status-dot" :class="statusClass"></div>
        </div>
      </div>
    </nav>

    <!-- 主内容区 -->
    <main class="main-content">
      <router-view />
    </main>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted } from 'vue';
import { usePilotStore } from './services/pilotStore';
import { useRouter } from 'vue-router';
import { LayoutDashboard, ShieldCheck, ScrollText, Settings, Monitor, TerminalSquare } from 'lucide-vue-next';

interface NavItem {
  path: string;
  icon: any;
  label: string;
}

const navItems: NavItem[] = [
  { path: '/', icon: LayoutDashboard, label: '概览' },
  { path: '/security', icon: ShieldCheck, label: '安全' },
  { path: '/browser', icon: Monitor, label: '浏览器' },
  { path: '/terminal', icon: TerminalSquare, label: '终端' },
  { path: '/logs', icon: ScrollText, label: '操作日志' },
  { path: '/settings', icon: Settings, label: '设置' },
];

const store = usePilotStore();
const router = useRouter();

const statusClass = computed(() => ({
  connected: store.isConnected,
  reconnecting: !store.isConnected && store.wsState === 'connecting',
  disconnected: !store.isConnected && store.wsState !== 'connecting',
}));

const statusText = computed(() => {
  if (store.isConnected) return '已连接云端';
  if (store.wsState === 'connecting') return '连接中...';
  return '离线';
});

onMounted(async () => {
  // 首次获取状态
  await store.fetchStatus(true);

  // 监听 Rust 端 deep link 授权成功事件
  try {
    const { listen } = await import('@tauri-apps/api/event');
    listen('pilot-auth-success', async () => {
      console.log('[App] 授权成功，刷新状态');
      await store.refresh();
      router.push('/');  // 跳转到概览页
    });
  } catch {
    // 非 Tauri 环境忽略
  }
});
</script>

<style scoped>
.sidebar {
  width: 64px;
  background: var(--bg-sidebar);
  border-right: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  padding: 20px 0;
  box-shadow: 1px 0 2px rgba(31, 35, 41, 0.02);
  z-index: 10;
  align-items: center;
}

.sidebar-brand {
  display: flex;
  justify-content: center;
  padding: 0 0 24px 0;
}

.brand-img {
  width: 32px;
  height: 32px;
}

.brand-text {
  font-weight: 700;
  font-size: 18px;
  color: var(--text-primary);
  letter-spacing: -0.02em;
}

.nav-items {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 8px;
  width: 100%;
  padding: 0 12px;
}

.nav-item {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 10px;
  border-radius: 8px;
  text-decoration: none;
  transition: all 0.15s ease;
}

.nav-item:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}

.nav-icon {
  color: var(--text-tertiary);
  transition: color 0.15s ease;
}

.nav-item.active {
  background: var(--bg-active);
  color: var(--accent);
}

.nav-item.active .nav-icon {
  color: var(--accent);
}

.sidebar-footer {
  margin-top: auto;
  padding-top: 16px;
}

.status-indicator {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border-radius: 8px;
  background: var(--bg-app);
  border: 1px solid var(--border-subtle);
}

.status-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--text-tertiary);
  position: relative;
}

.status-dot.connected {
  background: var(--green);
  box-shadow: 0 0 0 2px var(--green-dim);
}

.status-dot.reconnecting {
  background: var(--yellow);
  box-shadow: 0 0 0 2px var(--yellow-dim);
  animation: pulse 1.5s infinite;
}

.status-dot.disconnected {
  background: var(--red);
  box-shadow: 0 0 0 2px var(--red-dim);
}



@keyframes pulse {
  0% { opacity: 1; }
  50% { opacity: 0.5; }
  100% { opacity: 1; }
}
</style>
