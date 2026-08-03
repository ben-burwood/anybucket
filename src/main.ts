import { createApp } from "vue";
import { AllCommunityModule, ModuleRegistry } from "ag-grid-community";
import { router } from "./router";
import App from "./App.vue";
import "./store/useTheme";
import "./style.css";

// ag-grid v34+ requires explicit module registration.
ModuleRegistry.registerModules([AllCommunityModule]);

createApp(App).use(router).mount("#app");
