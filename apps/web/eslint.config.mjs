// Flat ESLint config (ESLint 9). Next.js 16 removed the `next lint` command,
// so we run ESLint directly (`eslint .`). `eslint-config-next` v16 ships a
// flat-config array; we spread it, ignore generated output, and soften one
// React-compiler rule that over-fires on idiomatic data loading.
import next from "eslint-config-next";

const config = [
  ...next,
  {
    // `set-state-in-effect` flags the standard async-fetch effect pattern
    // (`setLoading(true)` before an awaited engine call). That's intentional
    // here — the worker round-trips are async — so keep it as a non-blocking
    // hint rather than a hard error.
    rules: {
      "react-hooks/set-state-in-effect": "warn",
    },
  },
  {
    ignores: ["lib/wasm/**", "out/**", ".next/**", "next-env.d.ts"],
  },
];

export default config;
