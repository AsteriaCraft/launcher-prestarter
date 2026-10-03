import { sveltekit } from "@sveltejs/kit/vite";
import { defineConfig } from "vitest/config";

// Unit tests of the front end's pure code (state reducer, formatting, messages); no webview needed.
export default defineConfig({
  plugins: [sveltekit()],
  test: {
    include: ["src/**/*.test.ts"],
    environment: "node",
  },
});
