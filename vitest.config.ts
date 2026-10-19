import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["tests-web/**/*.test.ts"],
    coverage: {
      provider: "v8",
      include: ["frontend/selection.ts", "frontend/hotkey.ts"],
      thresholds: { statements: 90, lines: 90, functions: 90, branches: 85 },
    },
  },
});
