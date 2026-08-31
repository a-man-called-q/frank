//! Anthropic public output-token pricing, USD per million. Ported verbatim
//! from `archive/src/hooks/caveman-stats.js`'s `MODEL_OUTPUT_PRICE_PER_M`.
//! Matched by model id prefix so this stays correct across point releases;
//! most-specific prefixes must come first since `price_for_model` returns
//! the first match. Update from <https://www.anthropic.com/pricing> if a
//! release changes the tier.

pub const MODEL_OUTPUT_PRICE_PER_M: &[(&str, f64)] = &[
    ("claude-opus-4-0", 75.00),
    ("claude-opus-4-1", 75.00),
    ("claude-opus-4-2025", 75.00),
    ("claude-opus-4", 25.00),
    ("claude-sonnet-4", 15.00),
    ("claude-haiku-4", 5.00),
    ("claude-3-5-sonnet", 15.00),
    ("claude-3-5-haiku", 4.00),
    ("claude-3-opus", 75.00),
];

pub fn price_for_model(model: Option<&str>) -> Option<f64> {
    let model = model?;
    MODEL_OUTPUT_PRICE_PER_M
        .iter()
        .find(|(prefix, _)| model.starts_with(prefix))
        .map(|(_, price)| *price)
}

pub fn format_usd(amount: f64) -> String {
    if amount >= 1.0 {
        format!("${amount:.2}")
    } else if amount >= 0.01 {
        format!("${amount:.3}")
    } else {
        format!("${amount:.4}")
    }
}
