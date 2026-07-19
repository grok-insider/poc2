"use client";

import { useEffect } from "react";
import { Console } from "@/components/Console";
import { bootDesktop } from "@/lib/bootDesktop";
import { useCraft } from "@/lib/store";

export default function Page() {
  const boot = useCraft((s) => s.boot);
  const engineReady = useCraft((s) => s.engineReady);
  const engineError = useCraft((s) => s.engineError);

  useEffect(() => {
    void boot();
  }, [boot]);

  // Desktop shell only: captured item text → ingestExternalItemText.
  useEffect(() => bootDesktop(), []);

  if (engineError) {
    return (
      <div className="boot boot-error">
        <div className="card boot-card">
          <div className="eyebrow danger">Engine failed to load</div>
          <pre className="mono">{engineError}</pre>
        </div>
      </div>
    );
  }

  if (!engineReady) {
    return (
      <div className="boot">
        <div className="card boot-card">
          <div className="display-mark">⬡ PATH OF CRAFTING</div>
          <div className="muted">Loading the engine…</div>
          <div className="skeleton" style={{ height: 8, width: 220, marginTop: 14 }} />
        </div>
      </div>
    );
  }

  return <Console />;
}
