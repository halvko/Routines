// Runs the prototype's checks against `wrangler dev`, or against a deployed
// Worker when PROTO_URL is set (for example a preview URL).
import { defineConfig } from "@playwright/test";

const url = process.env.PROTO_URL || "http://localhost:8787";

export default defineConfig({
  testDir: "test",
  timeout: 60_000,
  workers: 1,
  reporter: "list",
  use: { baseURL: url, trace: "off" },
  webServer: process.env.PROTO_URL
    ? undefined
    : { command: "npx wrangler dev --port 8787", url: `${url}/ping/up`, reuseExistingServer: true, timeout: 180_000 },
});
