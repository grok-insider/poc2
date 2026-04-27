//! Recommendation-emitter dispatch (Phase F.4).
//!
//! Plugins that declared the `emit_recommendations` capability
//! contribute candidates to the advisor's beam search. Each plugin
//! call is bounded by the perf contract (1 ms hard cap; 3 timeouts
//! in 1 minute → auto-disable).

use serde::{Deserialize, Serialize};
use wasmtime::{Linker, Store};

use crate::manifest::Capability;
use crate::predicate::write_memory;
use crate::{LoadedPlugin, PluginError, PluginHost};

/// Ad-hoc recommendation emitted by a plugin. The advisor adapts
/// these into [`poc2_advisor::Candidate`]s by resolving the
/// `action_kind` + `args` fields into one of the AdvisorAction
/// variants.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCandidate {
    /// Free-form discriminant naming the action kind. Adapter maps
    /// these to the advisor's `AdvisorAction` enum:
    /// "apply_currency", "activate_omen", "apply_hinekoras_lock",
    /// "reveal", "stop", "abandon", "guidance".
    pub action: PluginCandidateAction,
    /// Confidence proxy in [0, 1].
    #[serde(default = "default_prior")]
    pub prior: f64,
    /// Higher = more important when ranked. Default 100.
    #[serde(default = "default_priority")]
    pub priority: u32,
    /// Free-form rationale surfaced in the UI.
    #[serde(default)]
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PluginCandidateAction {
    ApplyCurrency {
        currency: String,
        #[serde(default)]
        omens: Vec<String>,
    },
    ActivateOmen {
        omen: String,
    },
    ApplyHinekorasLock,
    Reveal {
        #[serde(default)]
        prefer: Vec<String>,
        #[serde(default)]
        use_abyssal_echoes: bool,
    },
    Stop,
    Abandon {
        reason: String,
    },
    Guidance {
        note: String,
    },
}

const fn default_prior() -> f64 {
    0.5
}
const fn default_priority() -> u32 {
    100
}

impl PluginHost {
    /// Invoke `emit_recommendations(state_json) -> Vec<PluginCandidate>`
    /// on every enabled plugin that declared the
    /// [`Capability::EmitRecommendations`] capability.
    pub fn emit_recommendations_for_state(
        &self,
        state_json: &serde_json::Value,
    ) -> Vec<(String, PluginCandidate)> {
        let mut out = Vec::new();
        for plugin in self.plugins.values() {
            if !plugin.enabled {
                continue;
            }
            if !plugin
                .manifest
                .capabilities
                .contains(&Capability::EmitRecommendations)
            {
                continue;
            }
            match self.dispatch_emit_recommendations(plugin, state_json) {
                Ok(candidates) => {
                    for c in candidates {
                        out.push((plugin.manifest.id.clone(), c));
                    }
                }
                Err(e) => {
                    tracing::warn!(plugin = %plugin.manifest.id, error = %e,
                        "plugin emit_recommendations failed; skipping for this state");
                }
            }
        }
        out
    }

    fn dispatch_emit_recommendations(
        &self,
        plugin: &LoadedPlugin,
        state: &serde_json::Value,
    ) -> Result<Vec<PluginCandidate>, PluginError> {
        let mut store = Store::new(&self.engine, ());
        store
            .set_fuel(1_000_000) // tighter than predicates — emitter perf budget is 1ms
            .map_err(|_| PluginError::FuelExhausted)?;
        let linker: Linker<()> = Linker::new(&self.engine);
        let instance = linker
            .instantiate(&mut store, &plugin.module)
            .map_err(PluginError::Module)?;

        let alloc = instance
            .get_typed_func::<i32, i32>(&mut store, "alloc")
            .map_err(|e| PluginError::Trap(format!("missing alloc export: {e}")))?;
        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or(PluginError::MissingMemory)?;
        let state_json = serde_json::to_vec(state).map_err(PluginError::DeserializeOutput)?;
        let state_ptr = alloc
            .call(&mut store, state_json.len() as i32)
            .map_err(|e| PluginError::Trap(e.to_string()))?;
        write_memory(&memory, &mut store, state_ptr, &state_json)?;

        let emit = instance
            .get_typed_func::<(i32, i32), (i32, i32)>(&mut store, "emit_recommendations")
            .map_err(|e| PluginError::Trap(format!("missing emit_recommendations export: {e}")))?;
        let (out_ptr, out_len) = emit
            .call(&mut store, (state_ptr, state_json.len() as i32))
            .map_err(|e| PluginError::Trap(e.to_string()))?;

        let bytes = crate::read_memory(&memory, &mut store, out_ptr, out_len)?;
        let candidates: Vec<PluginCandidate> =
            serde_json::from_slice(&bytes).map_err(PluginError::DeserializeOutput)?;
        Ok(candidates)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_candidate_apply_currency_round_trip() {
        let c = PluginCandidate {
            action: PluginCandidateAction::ApplyCurrency {
                currency: "ChaosOrb".into(),
                omens: vec!["OmenOfWhittling".into()],
            },
            prior: 0.7,
            priority: 150,
            rationale: "Cleanup chain".into(),
        };
        let s = serde_json::to_string(&c).unwrap();
        let back: PluginCandidate = serde_json::from_str(&s).unwrap();
        if let PluginCandidateAction::ApplyCurrency { currency, omens } = &back.action {
            assert_eq!(currency, "ChaosOrb");
            assert_eq!(omens, &["OmenOfWhittling"]);
        } else {
            panic!("expected ApplyCurrency");
        }
        assert_eq!(back.priority, 150);
    }

    #[test]
    fn plugin_candidate_guidance_round_trip() {
        let c = PluginCandidate {
            action: PluginCandidateAction::Guidance {
                note: "Wait for league day 7".into(),
            },
            prior: 0.5,
            priority: 50,
            rationale: "League cycle".into(),
        };
        let s = serde_json::to_string(&c).unwrap();
        let back: PluginCandidate = serde_json::from_str(&s).unwrap();
        if let PluginCandidateAction::Guidance { note } = &back.action {
            assert!(note.contains("league"));
        } else {
            panic!("expected Guidance");
        }
    }
}
