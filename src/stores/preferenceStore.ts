/**
 * 用户偏好：provider 选择等
 * - localStorage 持久化
 * - providers 列表启动拉一次，缓存 15 分钟
 */

import { defineStore } from 'pinia';
import { computed, ref } from 'vue';
import { usePilotStore } from '../services/pilotStore';
import { getProviders, type ProviderInfo } from '../services/copilot/catalog';
import { deriveHttpBase } from '../services/copilot/api';

const LS_KEY = 'tabpilot.provider';
const CACHE_TTL = 15 * 60 * 1000;

export const usePreferenceStore = defineStore('preference', () => {
  const pilot = usePilotStore();

  const providers = ref<ProviderInfo[]>([]);
  const defaultProvider = ref<string>('');
  const lastFetchedAt = ref(0);
  const loading = ref(false);

  // 用户选择（localStorage 持久化）
  const selectedProviderId = ref<string>(
    typeof localStorage !== 'undefined' ? localStorage.getItem(LS_KEY) || '' : '',
  );

  const httpBase = computed(() => deriveHttpBase(pilot.serverUrl));

  /** 有效的 provider id：localStorage → defaultProvider → ''(不带) */
  const activeProviderId = computed(() => {
    if (selectedProviderId.value) {
      const found = providers.value.find((p) => p.id === selectedProviderId.value);
      if (found && found.available) return selectedProviderId.value;
    }
    if (defaultProvider.value) return defaultProvider.value;
    return '';
  });

  const activeProvider = computed(() =>
    providers.value.find((p) => p.id === activeProviderId.value) || null,
  );

  async function fetchProviders(force = false) {
    if (!force && providers.value.length && Date.now() - lastFetchedAt.value < CACHE_TTL) {
      return;
    }
    if (!httpBase.value) return;
    loading.value = true;
    try {
      const res = await getProviders(httpBase.value);
      providers.value = res.providers || [];
      defaultProvider.value = res.default || '';
      lastFetchedAt.value = Date.now();
    } catch (e) {
      console.warn('[TabPilot] fetchProviders failed:', e);
    } finally {
      loading.value = false;
    }
  }

  function setProvider(id: string) {
    selectedProviderId.value = id;
    try {
      localStorage.setItem(LS_KEY, id);
    } catch {
      /* ignore */
    }
  }

  function clearProvider() {
    selectedProviderId.value = '';
    try {
      localStorage.removeItem(LS_KEY);
    } catch {
      /* ignore */
    }
  }

  return {
    providers,
    defaultProvider,
    loading,
    selectedProviderId,
    activeProviderId,
    activeProvider,
    fetchProviders,
    setProvider,
    clearProvider,
  };
});
