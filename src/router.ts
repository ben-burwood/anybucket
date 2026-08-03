import {
  createRouter,
  createWebHashHistory,
  type RouteRecordRaw,
} from "vue-router";

import BucketList from "./components/BucketList.vue";
import ConnectionManager from "./components/ConnectionManager.vue";
import ObjectBrowser from "./components/ObjectBrowser.vue";

const routes: RouteRecordRaw[] = [
  { path: "/", name: "buckets", component: BucketList },
  { path: "/connections", name: "connections", component: ConnectionManager },
  {
    // prefix is the object-store path within the bucket; kept as a query param
    // so slashes in the prefix don't fight the route matcher.
    path: "/browse/:bucket",
    name: "browse",
    component: ObjectBrowser,
    props: (route) => ({
      bucket: route.params.bucket as string,
      prefix: (route.query.prefix as string) ?? "",
    }),
  },
];

export const router = createRouter({
  // Hash history avoids the webview trying to hit a dev server for deep links.
  history: createWebHashHistory(),
  routes,
});
