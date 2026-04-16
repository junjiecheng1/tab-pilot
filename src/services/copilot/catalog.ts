/**
 * Provider / 模型列表
 * 对齐 record-view services/api.ts 的 getProviders()
 */

export interface ProviderInfo {
  id: string;
  name: string;
  icon: string;
  available: boolean;
  model: string | null;
}

export interface ProvidersResponse {
  providers: ProviderInfo[];
  default: string;
}

async function getPilotToken(): Promise<string> {
  try {
    const api = await import('@tauri-apps/api/core');
    return (await api.invoke<string>('get_pilot_token')) || '';
  } catch {
    return '';
  }
}

/** 后端标准信封 */
interface ApiEnvelope<T> {
  success: boolean;
  data: T | null;
  error: { code: string; message: string } | null;
}

export async function getProviders(base: string): Promise<ProvidersResponse> {
  const token = await getPilotToken();
  const headers: Record<string, string> = { 'Content-Type': 'application/json' };
  if (token) headers['Authorization'] = `Bearer ${token}`;

  // TabPilot 用 Bearer token 鉴权，不走 cookie；不要开 credentials，否则跨源失败
  const res = await fetch(`${base}/api/providers`, { headers });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);

  const json = (await res.json()) as ApiEnvelope<ProvidersResponse> | ProvidersResponse;
  // 后端返回 { success, data, error } 信封 → 取 data
  if (json && typeof json === 'object' && 'success' in json) {
    if (!json.success || !json.data) {
      throw new Error(json.error?.message || 'Unknown error');
    }
    return json.data;
  }
  // 兼容非标准直接返回
  return json as ProvidersResponse;
}
