//! Production-only coverage policy validation.
//!
//! `cargo-llvm-cov` can produce one workspace-wide JSON report, but its
//! built-in fail-under switches only validate the aggregate.  This module
//! keeps the policy per package and checks both a percentage floor and an
//! uncovered-count ceiling so a report cannot pass merely because new code
//! was added to a large, already-covered crate.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;

#[derive(Debug, Clone, Copy, Default)]
struct Metric {
    count: u64,
    covered: u64,
}

impl Metric {
    fn from_json(value: &Value, package: &str, kind: &str) -> Result<Self> {
        let count = value
            .get("count")
            .and_then(Value::as_u64)
            .with_context(|| format!("{package}: summary.{kind}.count is missing or invalid"))?;
        let covered = value
            .get("covered")
            .and_then(Value::as_u64)
            .with_context(|| format!("{package}: summary.{kind}.covered is missing or invalid"))?;
        if covered > count {
            bail!("{package}: summary.{kind}.covered exceeds count");
        }
        Ok(Self { count, covered })
    }

    fn uncovered(self) -> u64 {
        self.count - self.covered
    }

    fn percent(self) -> f64 {
        if self.count == 0 {
            100.0
        } else {
            (self.covered as f64 / self.count as f64) * 100.0
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Metrics {
    regions: Metric,
    functions: Metric,
    lines: Metric,
}

impl Metrics {
    fn add(&mut self, other: Self) {
        self.regions.count += other.regions.count;
        self.regions.covered += other.regions.covered;
        self.functions.count += other.functions.count;
        self.functions.covered += other.functions.covered;
        self.lines.count += other.lines.count;
        self.lines.covered += other.lines.covered;
    }
}

#[derive(Debug, Clone, Copy)]
struct Limits {
    min_regions: f64,
    min_functions: f64,
    min_lines: f64,
    max_uncovered_regions: u64,
    max_uncovered_functions: u64,
    max_uncovered_lines: u64,
    target_regions: f64,
    target_functions: f64,
    target_lines: f64,
}

fn number(table: &toml::map::Map<String, toml::Value>, key: &str) -> Result<f64> {
    table
        .get(key)
        .and_then(toml::Value::as_float)
        .or_else(|| {
            table
                .get(key)
                .and_then(toml::Value::as_integer)
                .map(|value| value as f64)
        })
        .with_context(|| format!("coverage policy is missing numeric key `{key}`"))
}

fn nonnegative_integer(table: &toml::map::Map<String, toml::Value>, key: &str) -> Result<u64> {
    let value = table
        .get(key)
        .and_then(toml::Value::as_integer)
        .with_context(|| format!("coverage policy is missing integer key `{key}`"))?;
    u64::try_from(value)
        .with_context(|| format!("coverage policy key `{key}` must be non-negative"))
}

fn limits(policy: &toml::Value, package: &str) -> Result<Limits> {
    let table = policy
        .get("packages")
        .and_then(toml::Value::as_table)
        .and_then(|packages| packages.get(package))
        .and_then(toml::Value::as_table)
        .with_context(|| format!("coverage policy has no [packages.{package}] table"))?;
    Ok(Limits {
        min_regions: number(table, "min_regions")?,
        min_functions: number(table, "min_functions")?,
        min_lines: number(table, "min_lines")?,
        max_uncovered_regions: nonnegative_integer(table, "max_uncovered_regions")?,
        max_uncovered_functions: nonnegative_integer(table, "max_uncovered_functions")?,
        max_uncovered_lines: nonnegative_integer(table, "max_uncovered_lines")?,
        target_regions: number(table, "target_regions")?,
        target_functions: number(table, "target_functions")?,
        target_lines: number(table, "target_lines")?,
    })
}

fn package_for_filename(filename: &str) -> Option<String> {
    let normalized = filename.replace('\\', "/");
    let components = normalized
        .split('/')
        .filter(|component| !component.is_empty() && *component != ".")
        .collect::<Vec<_>>();
    if let Some(index) = components
        .iter()
        .position(|component| *component == "crates")
    {
        return components
            .get(index + 1)
            .map(|package| (*package).to_string());
    }
    if components
        .windows(2)
        .any(|window| window == ["xtask", "src"])
    {
        return Some("xtask".to_string());
    }
    None
}

fn metric_summary(summary: &Value, package: &str) -> Result<Metrics> {
    Ok(Metrics {
        regions: Metric::from_json(
            summary
                .get("regions")
                .with_context(|| format!("{package}: summary.regions is missing"))?,
            package,
            "regions",
        )?,
        functions: Metric::from_json(
            summary
                .get("functions")
                .with_context(|| format!("{package}: summary.functions is missing"))?,
            package,
            "functions",
        )?,
        lines: Metric::from_json(
            summary
                .get("lines")
                .with_context(|| format!("{package}: summary.lines is missing"))?,
            package,
            "lines",
        )?,
    })
}

fn report_metrics(report: &Value) -> Result<BTreeMap<String, Metrics>> {
    let data = report
        .get("data")
        .and_then(Value::as_array)
        .context("coverage report must contain a data array")?;
    if data.is_empty() {
        bail!("coverage report data array is empty");
    }

    let mut by_package = BTreeMap::new();
    let mut unknown_sources = Vec::new();
    for entry in data {
        let files = entry
            .get("files")
            .and_then(Value::as_array)
            .context("coverage report data entry must contain a files array")?;
        for file in files {
            let filename = file
                .get("filename")
                .and_then(Value::as_str)
                .context("coverage file is missing filename")?;
            let Some(package) = package_for_filename(filename) else {
                unknown_sources.push(filename.to_string());
                continue;
            };
            let metrics = metric_summary(
                file.get("summary")
                    .with_context(|| format!("{filename}: summary is missing"))?,
                &package,
            )?;
            by_package
                .entry(package)
                .or_insert_with(Metrics::default)
                .add(metrics);
        }
    }
    if !unknown_sources.is_empty() {
        bail!(
            "coverage report contains unknown source file(s): {}",
            unknown_sources.join(", ")
        );
    }
    if by_package.is_empty() {
        bail!("coverage report contains no Frank workspace source files");
    }
    Ok(by_package)
}

fn check_values(report: &Value, policy: &toml::Value) -> Result<String> {
    let metrics = report_metrics(report)?;
    let packages = policy
        .get("packages")
        .and_then(toml::Value::as_table)
        .context("coverage policy must contain a [packages] table")?;
    if packages.is_empty() {
        bail!("coverage policy contains no packages");
    }

    let mut failures = Vec::new();
    let mut output = String::new();
    for package in packages.keys() {
        let Some(actual) = metrics.get(package) else {
            failures.push(format!("{package}: no source files in report"));
            continue;
        };
        let limit = limits(policy, package)?;
        let checks = [
            (
                "regions",
                actual.regions,
                limit.min_regions,
                limit.max_uncovered_regions,
                limit.target_regions,
            ),
            (
                "functions",
                actual.functions,
                limit.min_functions,
                limit.max_uncovered_functions,
                limit.target_functions,
            ),
            (
                "lines",
                actual.lines,
                limit.min_lines,
                limit.max_uncovered_lines,
                limit.target_lines,
            ),
        ];
        for (kind, metric, min, max_uncovered, target) in checks {
            if metric.percent() + f64::EPSILON < min {
                failures.push(format!(
                    "{package} {kind}: {:.2}% is below minimum {:.2}%",
                    metric.percent(),
                    min
                ));
            }
            if metric.uncovered() > max_uncovered {
                failures.push(format!(
                    "{package} {kind}: {} uncovered exceeds maximum {}",
                    metric.uncovered(),
                    max_uncovered
                ));
            }
            let gap = (target - metric.percent()).max(0.0);
            if !output.is_empty() {
                output.push('\n');
            }
            output.push_str(&format!(
                "  {package} {kind}: {:.2}% ({}/{}) | target {:.2}% (gap {:.2}pp)",
                metric.percent(),
                metric.covered,
                metric.count,
                target,
                gap
            ));
        }
    }

    for package in metrics.keys() {
        if !packages.contains_key(package) {
            failures.push(format!(
                "{package}: report contains an unknown workspace package"
            ));
        }
    }
    if failures.is_empty() {
        Ok(output)
    } else {
        Err(anyhow!(
            "coverage policy failed:\n- {}",
            failures.join("\n- ")
        ))
    }
}

pub fn check(report_path: &Path, policy_path: &Path) -> Result<()> {
    let report: Value = serde_json::from_str(
        &std::fs::read_to_string(report_path)
            .with_context(|| format!("reading coverage report {}", report_path.display()))?,
    )
    .with_context(|| format!("parsing coverage report {}", report_path.display()))?;
    let policy: toml::Value = toml::from_str(
        &std::fs::read_to_string(policy_path)
            .with_context(|| format!("reading coverage policy {}", policy_path.display()))?,
    )
    .with_context(|| format!("parsing coverage policy {}", policy_path.display()))?;
    let summary = check_values(&report, &policy)?;
    println!("xtask coverage-check: passed\n{summary}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{check, check_values};
    use serde_json::json;
    use std::fs;
    use tempfile::tempdir;

    fn policy(min_lines: i64, max_uncovered_lines: i64) -> toml::Value {
        toml::from_str(&format!(
            r#"
                version = 1
                [packages.demo]
                min_regions = 50
                min_functions = 50
                min_lines = {min_lines}
                max_uncovered_regions = 5
                max_uncovered_functions = 5
                max_uncovered_lines = {max_uncovered_lines}
                target_regions = 90
                target_functions = 90
                target_lines = 95
            "#
        ))
        .unwrap()
    }

    fn report(lines_covered: u64, lines_total: u64) -> serde_json::Value {
        json!({
            "data": [{"files": [{
                "filename": "/repo/crates/demo/src/lib.rs",
                "summary": {
                    "regions": {"count": 2, "covered": 1},
                    "functions": {"count": 2, "covered": 1},
                    "lines": {"count": lines_total, "covered": lines_covered}
                }
            }]}]
        })
    }

    #[test]
    fn accepts_report_and_prints_no_error() {
        assert!(check_values(&report(9, 10), &policy(80, 1)).is_ok());
    }

    #[test]
    fn rejects_malformed_report_shape_and_missing_metric() {
        assert!(check_values(&json!({"data": []}), &policy(0, 10)).is_err());
        let missing = json!({
            "data": [{"files": [{
                "filename": "/repo/crates/demo/src/lib.rs",
                "summary": {"regions": {"count": 1, "covered": 1}}
            }]}]
        });
        assert!(check_values(&missing, &policy(0, 10)).is_err());
    }

    #[test]
    fn rejects_floor_and_uncovered_count_regressions() {
        let error = check_values(&report(1, 10), &policy(50, 8)).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("below minimum"));
        assert!(message.contains("uncovered exceeds maximum"));
    }

    #[test]
    fn rejects_unknown_report_package() {
        let report = json!({
            "data": [{"files": [{
                "filename": "/repo/crates/unknown/src/lib.rs",
                "summary": {
                    "regions": {"count": 1, "covered": 1},
                    "functions": {"count": 1, "covered": 1},
                    "lines": {"count": 1, "covered": 1}
                }
            }]}]
        });
        assert!(check_values(&report, &policy(0, 10)).is_err());
    }

    #[test]
    fn rejects_unknown_source_file_and_accepts_relative_workspace_paths() {
        let unknown = json!({
            "data": [{"files": [
                {
                    "filename": "/repo/crates/demo/src/lib.rs",
                    "summary": {
                        "regions": {"count": 1, "covered": 1},
                        "functions": {"count": 1, "covered": 1},
                        "lines": {"count": 1, "covered": 1}
                    }
                },
                {
                    "filename": "/deps/serde/src/lib.rs",
                    "summary": {
                        "regions": {"count": 1, "covered": 1},
                        "functions": {"count": 1, "covered": 1},
                        "lines": {"count": 1, "covered": 1}
                    }
                }
            ]}]
        });
        assert!(check_values(&unknown, &policy(0, 10)).is_err());

        let relative = json!({
            "data": [{"files": [{
                "filename": "./crates/demo/src/lib.rs",
                "summary": {
                    "regions": {"count": 1, "covered": 1},
                    "functions": {"count": 1, "covered": 1},
                    "lines": {"count": 1, "covered": 1}
                }
            }]}]
        });
        assert!(check_values(&relative, &policy(100, 0)).is_ok());
    }

    #[test]
    fn rejects_malformed_json_file() {
        let tmp = tempdir().unwrap();
        let report = tmp.path().join("report.json");
        let policy_path = tmp.path().join("coverage.toml");
        fs::write(&report, "{not valid json").unwrap();
        fs::write(
            &policy_path,
            r#"
version = 1
[packages.demo]
min_regions = 0
min_functions = 0
min_lines = 0
max_uncovered_regions = 10
max_uncovered_functions = 10
max_uncovered_lines = 10
target_regions = 90
target_functions = 90
target_lines = 95
"#,
        )
        .unwrap();
        assert!(check(&report, &policy_path).is_err());
    }
}
