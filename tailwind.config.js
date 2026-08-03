/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{vue,ts,tsx}"],
  darkMode: "class",
  theme: {
    extend: {
      colors: {
        surface: {
          DEFAULT: "#ffffff",
          muted: "#f8fafc",
          border: "#e2e8f0",
        },
        // Neutral-grey dark surfaces matching ag-grid's Quartz `colorSchemeDark`
        // (grid background is #2b2b2b) so the chrome and the data grid read as
        // one palette. Neutral grey keeps the green accent as the only colour.
        night: {
          950: "#1f1f1f", // app root (behind panels)
          900: "#2b2b2b", // panels, header, cards (matches grid bg)
          800: "#363636", // inputs, raised/hover surfaces
          700: "#414141", // borders
        },
      },
    },
  },
  plugins: [],
};
