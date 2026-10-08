import {
  createRouter,
  createWebHashHistory,
  type RouteRecordRaw,
} from "vue-router";

import BucketList from "./components/BucketList.vue";
import ConnectionManager from "./components/ConnectionManager.vue";
import ObjectBrowser from "./components/ObjectBrowser.vue";
import { useConnections } from "./store/useConnections";

const routes: RouteRecordRaw[] = [
  { path: "/", name: "buckets", component: BucketList, meta: { sidebar: false } },
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

// Single-bucket connections have no bucket list; send the landing route straight
// into the dedicated bucket. Awaiting the connections load avoids a flash of the
// bucket-list page on cold start (the active connection loads asynchronously).
router.beforeEach(async (to) => {
  const conns = useConnections();
  await conns.ensureLoaded();
  // In config mode connections are managed by the server; the management page is disabled.
  if (to.name === "connections" && conns.configMode.value) {
    return { name: "buckets" };
  }
  const bucket = conns.state.active?.bucket;
  if (bucket && to.name === "buckets") {
    return { name: "browse", params: { bucket } };
  }
  return true;
});
