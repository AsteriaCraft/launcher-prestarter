// Theme Configuration
// Centralized colors, fonts, and spacing for the launcher

export const theme = {
  // Colors
  colors: {
    base: "#0f1424",
    textPrimary: "#f3f4fb",
    textSecondary: "#9aa5c4",
    textDescription: "#7f8bad",
    shadow: "rgba(0, 0, 0, 0.55)",
    error: "#ff6b6b",

    // Gradients
    activeGradient:
      "linear-gradient(135deg, #ff6cb8 0%, #9b4dff 55%, #5cc8ff 100%)",
    progressGradient:
      "linear-gradient(90deg, #c240ff 0%, #8247ff 48%, #5bc0ff 100%)",

    // Backgrounds
    overlayDark: "rgba(0, 0, 0, 0.9)",
    overlayLight: "rgba(0, 0, 0, 0.75)",
    progressBg: "#111727",

    // UI Elements
    border: "rgba(255, 255, 255, 0.08)",
    borderLight: "rgba(255, 255, 255, 0.03)",
  },

  // Typography
  fonts: {
    primary: '"Inter", "Kharkiv", sans-serif',
    weights: {
      thin: 100,
      regular: 400,
      semibold: 600,
      bold: 700,
    },
    sizes: {
      xs: "0.6rem", // Top bar metadata
      sm: "0.7rem", // Small text, tips
      md: "0.75rem", // Status text
      lg: "0.85rem", // Percentage
      xl: "0.9rem", // Welcome heading
    },
  },

  // Spacing scale
  spacing: {
    xs: "0.25rem", // 4px
    sm: "0.6rem", // ~10px
    md: "1rem", // 16px
    lg: "1.25rem", // 20px
    xl: "1.5rem", // 24px
  },

  // Transitions
  transitions: {
    fast: "200ms",
    normal: "300ms",
    slow: "500ms",
  },
} as const;

export type Theme = typeof theme;
