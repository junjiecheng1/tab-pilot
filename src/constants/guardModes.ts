/**
 * 安全模式统一数据源
 *
 * Dashboard / SecurityView / SettingsView 共用
 */

import { Shield, ShieldHalf, Unlock } from 'lucide-vue-next';
import type { Component } from 'vue';

export interface GuardModeInfo {
  value: string;
  label: string;
  icon: Component;
  colorClass: string;
  description: string;
}

export const GUARD_MODES: GuardModeInfo[] = [
  {
    value: 'conservative',
    label: '保守模式',
    icon: Shield,
    colorClass: 'color-red',
    description: '读操作放行，一切写操作需确认',
  },
  {
    value: 'standard',
    label: '标准模式',
    icon: ShieldHalf,
    colorClass: 'color-blue',
    description: '智能免疫，对未知或高危修改拦截',
  },
  {
    value: 'trust',
    label: '信任模式',
    icon: Unlock,
    colorClass: 'color-green',
    description: '自主执行，仅硬编码名单被拦截',
  },
];

/** value → label 映射 */
export const GUARD_MODE_LABELS: Record<string, string> = Object.fromEntries(
  GUARD_MODES.map((m) => [m.value, m.label])
);

/** value → colorClass 映射 */
export const GUARD_MODE_COLORS: Record<string, string> = Object.fromEntries(
  GUARD_MODES.map((m) => [m.value, m.colorClass])
);
