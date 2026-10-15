//! Writes Jobfather job output files (label, chips, metrics, reports).
//!
//! Jobfather mounts an emptyDir at `/job-output` and uploads its contents when
//! the job exits. When that directory doesn't exist (e.g. running locally),
//! everything here is a no-op. `JOB_OUTPUT_DIR` overrides the location.

use std::path::PathBuf;

use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
pub struct Chip {
    pub name: String,
    pub value: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

impl Chip {
    pub fn new(name: &str, value: impl Into<Value>, color: Option<&str>) -> Self {
        Chip {
            name: name.to_string(),
            value: value.into(),
            color: color.map(str::to_string),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MetricType {
    Gauge,
    Counter,
}

#[derive(Serialize)]
pub struct Metric {
    pub name: String,
    pub value: f64,
    #[serde(rename = "type")]
    pub metric_type: MetricType,
    pub labels: serde_json::Map<String, Value>,
}

impl Metric {
    pub fn new(name: &str, value: f64, metric_type: MetricType, labels: &[(&str, &str)]) -> Self {
        Metric {
            name: name.to_string(),
            value,
            metric_type,
            labels: labels
                .iter()
                .map(|(k, v)| (k.to_string(), Value::String(v.to_string())))
                .collect(),
        }
    }
}

pub struct JobOutput {
    dir: Option<PathBuf>,
}

impl JobOutput {
    pub fn detect() -> Self {
        let dir = PathBuf::from(
            std::env::var("JOB_OUTPUT_DIR").unwrap_or_else(|_| "/job-output".to_string()),
        );
        JobOutput {
            dir: dir.is_dir().then_some(dir),
        }
    }

    pub fn write(&self, filename: &str, contents: &str) {
        let Some(dir) = &self.dir else {
            return;
        };
        let path = dir.join(filename);
        if let Err(e) = std::fs::write(&path, contents) {
            eprintln!("Failed to write job output {}: {}", path.display(), e);
        }
    }

    pub fn write_json<T: Serialize>(&self, filename: &str, value: &T) {
        match serde_json::to_string_pretty(value) {
            Ok(json) => self.write(filename, &json),
            Err(e) => eprintln!("Failed to serialize job output {}: {}", filename, e),
        }
    }

    pub fn label(&self, label: &str) {
        self.write("label.txt", label);
    }

    pub fn chips(&self, chips: &[Chip]) {
        self.write_json("chips.json", &chips);
    }

    pub fn metrics(&self, metrics: &[Metric]) {
        self.write_json("metrics.json", &metrics);
    }
}

/// Label for a run over one or more sites, e.g. "remux scan: bj +3".
pub fn run_label(command: &str, slugs: &[String]) -> String {
    match slugs {
        [] => command.to_string(),
        [only] => format!("{}: {}", command, only),
        [first, rest @ ..] => format!("{}: {} +{}", command, first, rest.len()),
    }
}

pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} B", bytes)
    } else {
        format!("{:.1} {}", value, UNITS[unit])
    }
}
