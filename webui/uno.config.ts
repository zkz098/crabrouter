import { defineConfig } from "unocss";
import presetWind3 from "@unocss/preset-wind3";
import presetIcons from "@unocss/preset-icons";

export default defineConfig({
  presets: [
    presetWind3({
      dark: "class",
    }),
    presetIcons({
      scale: 1.2,
      extraProperties: {
        display: "inline-block",
        "vertical-align": "middle",
      },
    }),
  ],
  theme: {
    fontFamily: {
      sans: '"Inter", -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif',
      mono: '"JetBrains Mono", ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace',
    },
  },
});
