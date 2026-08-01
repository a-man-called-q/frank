import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  test: {
    include: ["src/**/*.test.{ts,tsx}"],
    environment: "jsdom",
    setupFiles: ["./src/test-setup.ts"],
    coverage: {
      provider: "v8",
      include: ["src/**/*.{ts,tsx}"],
      thresholds: { lines: 90, functions: 90, statements: 90, branches: 85 },
      // The entrypoint only mounts React and has no decision logic to exercise.
      exclude: ["src/test-setup.ts", "src/types.ts", "src/main.tsx"],
    },
  },
});
