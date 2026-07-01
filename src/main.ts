import { createApp } from "vue";
import ArcoVue, { Message, Modal } from "@arco-design/web-vue";
import ArcoVueIcon from "@arco-design/web-vue/es/icon";
import "@arco-design/web-vue/dist/arco.css";
import "./styles.css";
import App from "./App.vue";
import { installArcoI18n, installDomI18n } from "./i18n";

installArcoI18n(Message, Modal);

createApp(App).use(ArcoVue).use(ArcoVueIcon).mount("#app");
installDomI18n();
