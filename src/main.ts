import { createApp } from "vue";
import { createRouter, createWebHashHistory } from "vue-router";
import App from "./App.vue";
import "./styles/index.css";

// 恢复主题设置
const theme = localStorage.getItem('theme-preference');
if (theme && theme !== 'system') {
  document.documentElement.setAttribute('data-theme', theme);
  document.documentElement.classList.add(theme);
}

const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    {
      path: "/",
      name: "dashboard",
      component: () => import("./views/DashboardView.vue"),
    },
    {
      path: "/security",
      name: "security",
      component: () => import("./views/SecurityView.vue"),
    },
    {
      path: "/logs",
      name: "logs",
      component: () => import("./views/LogsView.vue"),
    },
    {
      path: "/settings",
      name: "settings",
      component: () => import("./views/SettingsView.vue"),
    },
    {
      path: "/browser",
      name: "browser",
      component: () => import("./views/BrowserView.vue"),
    },
    {
      path: "/terminal",
      name: "terminal",
      component: () => import("./views/TerminalView.vue"),
    },
  ],
});

import { createPinia } from 'pinia';

const app = createApp(App);
app.use(createPinia());
app.use(router);
app.mount("#app");
