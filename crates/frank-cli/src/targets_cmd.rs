//! `frank targets` is a presentation adapter over the shared application
//! service. Discovery, path precedence, probing, and manifest parsing live in
//! `frank-app` so the CLI and GUI cannot drift apart.

use frank_app::{FrankPaths, FrankService};

pub fn run(detected_only: bool, json: bool) -> i32 {
    let discovery = FrankService::new(FrankPaths::from_process()).discover_targets();

    #[derive(serde::Serialize)]
    struct Row {
        id: String,
        label: String,
        kind: String,
        verified: bool,
        soft: bool,
        detected: bool,
        source: String,
    }

    let mut rows = discovery
        .targets
        .into_iter()
        .map(|target| Row {
            id: target.id,
            label: target.label,
            kind: target.kind,
            verified: target.verified,
            soft: target.soft,
            detected: target.detected,
            source: target.source,
        })
        .collect::<Vec<_>>();

    if detected_only {
        rows.retain(|row| row.detected);
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&rows).unwrap());
    } else {
        for row in &rows {
            let mark = if row.detected { "\u{2713}" } else { " " };
            let ver = if row.verified {
                "verified"
            } else {
                "unverified"
            };
            let soft = if row.soft { ", soft" } else { "" };
            println!(
                "[{mark}] {:<14} {:<20} {} ({ver}{soft})",
                row.id, row.label, row.kind
            );
        }
    }

    for error in &discovery.errors {
        eprintln!("frank: failed to parse target manifest {error}");
    }

    if discovery.errors.is_empty() { 0 } else { 1 }
}
