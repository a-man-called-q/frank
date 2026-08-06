use std::collections::BTreeMap;

mod generated {
    #![allow(dead_code)]
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../packs/caveman/compiled.rs"
    ));
}

pub const PACK_ID: &str = generated::PACK_ID;
pub const PACK_VERSION: &str = generated::PACK_VERSION;

/// Selects the built-in pack: bare id, or id at the shipped version.
pub(crate) fn selects_builtin(selector: &str) -> bool {
    selector == PACK_ID || selector == format!("{PACK_ID}@{PACK_VERSION}")
}

/// Claims the built-in id at *any* version. Used only to refuse removal, so
/// `caveman@9.9.9` cannot be deleted by inventing a version that isn't the
/// one actually shipped.
pub(crate) fn claims_builtin_id(selector: &str) -> bool {
    selector == PACK_ID || selector.starts_with(&format!("{PACK_ID}@"))
}

pub fn builtin_pack() -> frank_pack::CompiledPack {
    let levels = generated::LEVELS
        .iter()
        .map(|l| {
            (
                l.id.to_string(),
                frank_pack::CompiledLevel {
                    id: l.id.to_string(),
                    title: l.title.map(str::to_string),
                    aliases: l.aliases.iter().map(|a| a.to_string()).collect(),
                    activation_prompt: l.activation_prompt.to_string(),
                    reinforce: l.reinforce.to_string(),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let aliases = generated::ALIASES
        .iter()
        .map(|a| (a.alias.to_string(), a.canonical.to_string()))
        .collect();
    let oneshots = generated::ONESHOTS
        .iter()
        .map(|o| {
            (
                o.id.to_string(),
                frank_pack::CompiledOneshot {
                    id: o.id.to_string(),
                    prompt: o.prompt.to_string(),
                    restores_previous: o.restores_previous,
                },
            )
        })
        .collect();
    frank_pack::CompiledPack {
        id: PACK_ID.to_string(),
        version: PACK_VERSION.to_string(),
        default_level: generated::DEFAULT_LEVEL.to_string(),
        levels,
        aliases,
        oneshots,
        activation: frank_pack::CompiledActivation {
            on: generated::ACTIVATION
                .on
                .iter()
                .map(|s| s.to_string())
                .collect(),
            off: generated::ACTIVATION
                .off
                .iter()
                .map(|s| s.to_string())
                .collect(),
            question_guard: generated::ACTIVATION.question_guard.map(str::to_string),
            command_prefix: generated::ACTIVATION.command_prefix.map(str::to_string),
        },
        benchmark: generated::BENCHMARK
            .iter()
            .map(|r| {
                (
                    r.level.to_string(),
                    frank_pack::ReductionStat {
                        mean: r.mean,
                        p25: r.p25,
                        p75: r.p75,
                        n: r.n,
                        model: r.model.map(str::to_string),
                    },
                )
            })
            .collect(),
    }
}
