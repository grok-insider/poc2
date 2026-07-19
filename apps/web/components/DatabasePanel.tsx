"use client";

import { useEffect, useState, type ReactNode } from "react";
import { Database, Search } from "lucide-react";
import { useCraft } from "@/lib/store";
import { engine } from "@/lib/engine/client";
import { BaseIcon } from "@/components/BaseIcon";
import type {
  DatabaseEntryDetail,
  DatabaseEntrySummary,
  DatabaseSection,
  DatabaseStatLine,
} from "@/lib/types";
import styles from "./DatabasePanel.module.css";

const SECTIONS: { id: DatabaseSection; label: string }[] = [
  { id: "bases", label: "Bases" },
  { id: "materials", label: "Materials" },
];

const MAX_ROWS = 300;

/** label: value rows shared by base + material detail cards. */
function StatLines({ lines }: { lines: DatabaseStatLine[] }) {
  if (lines.length === 0) return null;
  return (
    <div className={styles.statLines}>
      {lines.map((s, i) => (
        <div key={i} className={styles.statLine} title={s.help ?? undefined}>
          <span className={styles.statLabel}>{s.label}</span>
          <span className={`${styles.statValue} num`}>{s.value}</span>
        </div>
      ))}
    </div>
  );
}

function Tags({ tags, cap = 8 }: { tags: string[]; cap?: number }) {
  if (tags.length === 0) return null;
  const shown = tags.slice(0, cap);
  const extra = tags.length - shown.length;
  return (
    <div className={styles.tags}>
      {shown.map((t, i) => (
        <span key={`${t}-${i}`} className="tag">
          {t}
        </span>
      ))}
      {extra > 0 && <span className="tag faint">+{extra}</span>}
    </div>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className={styles.detailSection}>
      <div className="section-title">{title}</div>
      {children}
    </div>
  );
}

/* ---------- Detail card ------------------------------------------------- */

function DetailCard({
  detail,
  section,
}: {
  detail: DatabaseEntryDetail;
  section: DatabaseSection;
}) {
  const item = useCraft((s) => s.item);
  const setItem = useCraft((s) => s.setItem);
  const summary = detail.summary;
  const base = detail.base ?? null;
  const material = detail.material ?? null;

  function useAsBase() {
    if (!base) return;
    setItem({
      ...item,
      base: summary.base?.class_pascal ?? base.class_display,
      base_type_id: summary.id,
      base_display_name: summary.name,
      ilvl: item.ilvl,
    });
  }

  return (
    <div className={`card ${styles.detail}`}>
      <div className={styles.detailHead}>
        <div className={styles.detailTitle}>{summary.name}</div>
        <div className={styles.detailSub}>
          <span className="muted">{summary.category}</span>
          {summary.kind && (
            <>
              <span className="faint"> · </span>
              <span className="tag">{summary.kind}</span>
            </>
          )}
        </div>
        {summary.description && (
          <p className={styles.detailDesc}>{summary.description}</p>
        )}
      </div>

      {section === "bases" && base && (
        <>
          <Section title="Overview">
            <div className={styles.metaGrid}>
              <div className={styles.metaCell}>
                <span className="faint">Class</span>
                <span className="muted">{base.class_display}</span>
              </div>
              <div className={styles.metaCell}>
                <span className="faint">Drop level</span>
                <span className="num gold">{base.drop_level}</span>
              </div>
              <div className={styles.metaCell}>
                <span className="faint">Attributes</span>
                <span className="muted">{base.attribute_pool}</span>
              </div>
              <div className={styles.metaCell}>
                <span className="faint">Inventory</span>
                <span className="num muted">
                  {base.inventory_width}×{base.inventory_height}
                </span>
              </div>
            </div>
            <div className={styles.metaType + " mono faint"}>{base.metadata_type}</div>
          </Section>

          {base.tags.length > 0 && (
            <Section title="Tags">
              <Tags tags={base.tags} cap={20} />
            </Section>
          )}
          {base.requirements.length > 0 && (
            <Section title="Requirements">
              <ul className={styles.bullets}>
                {base.requirements.map((r, i) => (
                  <li key={i} className="muted">
                    {r}
                  </li>
                ))}
              </ul>
            </Section>
          )}
          {base.derived_stats.length > 0 && (
            <Section title="Derived stats">
              <StatLines lines={base.derived_stats} />
            </Section>
          )}
          {base.granted_effects.length > 0 && (
            <Section title="Granted effects">
              <StatLines lines={base.granted_effects} />
            </Section>
          )}
          {base.class_notes.length > 0 && (
            <Section title="Class notes">
              <ul className={styles.bullets}>
                {base.class_notes.map((n, i) => (
                  <li key={i} className="muted">
                    {n}
                  </li>
                ))}
              </ul>
            </Section>
          )}

          <div className={styles.detailActions}>
            <button className="btn btn-primary" onClick={useAsBase}>
              Use as base ▸
            </button>
          </div>
        </>
      )}

      {section === "materials" && material && (
        <>
          {material.description && (
            <Section title="Description">
              <p className="muted">{material.description}</p>
            </Section>
          )}
          <Section title="Source">
            <span className="muted">{material.source_section}</span>
          </Section>
          {material.applies_to.length > 0 && (
            <Section title="Applies to">
              <Tags tags={material.applies_to} cap={20} />
            </Section>
          )}
          {material.tags.length > 0 && (
            <Section title="Tags">
              <Tags tags={material.tags} cap={20} />
            </Section>
          )}
          {material.raw_fields.length > 0 && (
            <Section title="Fields">
              <StatLines lines={material.raw_fields} />
            </Section>
          )}
        </>
      )}
    </div>
  );
}

/* ---------- Panel ------------------------------------------------------- */

export function DatabasePanel() {
  const [section, setSection] = useState<DatabaseSection>("bases");
  const [search, setSearch] = useState("");
  const [debounced, setDebounced] = useState("");
  const [entries, setEntries] = useState<DatabaseEntrySummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [detail, setDetail] = useState<DatabaseEntryDetail | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);

  // Debounce the search input (~200ms).
  useEffect(() => {
    const t = setTimeout(() => setDebounced(search.trim()), 200);
    return () => clearTimeout(t);
  }, [search]);

  // List entries on (section, debounced search).
  useEffect(() => {
    let live = true;
    setLoading(true);
    setError(null);
    engine
      .listDatabaseEntries(section, debounced || undefined)
      .then((rows) => {
        if (!live) return;
        setEntries(rows.slice(0, MAX_ROWS));
        setLoading(false);
      })
      .catch((e) => {
        if (!live) return;
        setError(String(e));
        setEntries([]);
        setLoading(false);
      });
    return () => {
      live = false;
    };
  }, [section, debounced]);

  // Reset the selection whenever the section changes.
  useEffect(() => {
    setSelectedId(null);
    setDetail(null);
  }, [section]);

  // Resolve the detail for the selected entry.
  useEffect(() => {
    if (!selectedId) {
      setDetail(null);
      return;
    }
    let live = true;
    setDetailLoading(true);
    engine
      .databaseEntryDetail(section, selectedId)
      .then((d) => {
        if (!live) return;
        setDetail(d);
        setDetailLoading(false);
      })
      .catch(() => {
        if (!live) return;
        setDetail(null);
        setDetailLoading(false);
      });
    return () => {
      live = false;
    };
  }, [section, selectedId]);

  return (
    <div className="pane">
      <div className="pane-head">
        <div className="pane-title">Database</div>
        <div className={styles.headActions}>
          <div className="seg">
            {SECTIONS.map((s) => (
              <button
                key={s.id}
                className={section === s.id ? "on" : ""}
                onClick={() => setSection(s.id)}
              >
                {s.label}
              </button>
            ))}
          </div>
          <div className={styles.search}>
            <Search size={13} className="faint" />
            <input
              className="field"
              placeholder={`Search ${section}…`}
              value={search}
              onChange={(e) => setSearch(e.target.value)}
            />
          </div>
        </div>
      </div>

      <div className={`pane-scroll ${styles.layout}`}>
        <div className={styles.list}>
          {loading ? (
            <div className={styles.skeletons}>
              {Array.from({ length: 6 }, (_, i) => (
                <div key={i} className="skeleton" style={{ height: 56 }} />
              ))}
            </div>
          ) : error ? (
            <div className="empty-state">
              <span className="eyebrow danger">Database error</span>
              <span className="faint mono" style={{ fontSize: 11 }}>
                {error}
              </span>
            </div>
          ) : entries.length === 0 ? (
            <div className="empty-state">
              <Database size={20} className="faint" />
              <span className="muted">
                {debounced ? `No ${section} match “${debounced}”.` : `No ${section} found.`}
              </span>
            </div>
          ) : (
            entries.map((e, ei) => (
              <button
                key={`${e.id}-${ei}`}
                className={`${styles.row} ${selectedId === e.id ? styles.rowActive : ""}`}
                onClick={() => setSelectedId(e.id)}
                title={e.description ?? undefined}
              >
                {section === "bases" && (
                  <BaseIcon baseId={e.id} name={e.name} size={34} />
                )}
                <div className={styles.rowBody}>
                  <div className={styles.rowTop}>
                    <span className={styles.rowName}>{e.name}</span>
                    {e.kind && <span className="tag">{e.kind}</span>}
                  </div>
                  <div className={styles.rowMeta}>
                    <span className="faint">{e.category}</span>
                    {e.tags.slice(0, 3).map((t, ti) => (
                      <span key={`${t}-${ti}`} className="chip">
                        {t}
                      </span>
                    ))}
                  </div>
                  {e.description && (
                    <div className={styles.rowDesc}>{e.description}</div>
                  )}
                </div>
              </button>
            ))
          )}
          {!loading && entries.length >= MAX_ROWS && (
            <div className={styles.capNote + " faint"}>
              Showing first {MAX_ROWS} — refine your search to narrow.
            </div>
          )}
        </div>

        <div className={styles.detailCol}>
          {detailLoading ? (
            <div className={`card ${styles.detail}`}>
              <div className="skeleton" style={{ height: 18, width: "60%" }} />
              <div className="skeleton" style={{ height: 10, width: "40%", marginTop: 10 }} />
              <div className="skeleton" style={{ height: 80, width: "100%", marginTop: 14 }} />
            </div>
          ) : detail ? (
            <DetailCard detail={detail} section={section} />
          ) : (
            <div className={`card ${styles.detailEmpty}`}>
              <Database size={22} className="faint" />
              <span className="muted">Select an entry to inspect its details.</span>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
