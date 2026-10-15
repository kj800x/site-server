//! Detect MP4s that aren't laid out for progressive web playback and losslessly
//! remux them (`ffmpeg -c copy -movflags +faststart`) in place.
//!
//! Two layouts are fixed:
//! - `moov` after `mdat` (classic non-faststart file): players must fetch the
//!   end of the file before playback can start.
//! - Fragmented MP4 (`moof` atoms) with no `sidx` in front of them: FFmpeg's
//!   demuxer (and so Chrome) walks every fragment, i.e. reads the whole file,
//!   before playback can start.

use std::{
    fmt::Write as _,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::Command,
};

use serde::Serialize;
use serde_json::Value;

use crate::{
    job_output::{human_bytes, run_label, Chip, JobOutput, Metric, MetricType},
    site::FileCrawlType,
    workdir::WorkDir,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Layout {
    Ok,
    MoovAtEnd,
    FragmentedUnindexed,
    NotMp4,
    Unparseable,
    Missing,
}

impl Layout {
    pub fn needs_remux(&self) -> bool {
        matches!(self, Layout::MoovAtEnd | Layout::FragmentedUnindexed)
    }
}

/// Classify a file by walking its top-level ISO BMFF atoms. Only atom headers
/// are read, so this is cheap even for large files.
pub fn classify(path: &Path) -> Result<Layout, String> {
    let mut file = File::open(path).map_err(|e| format!("open: {}", e))?;
    let file_len = file.metadata().map_err(|e| format!("stat: {}", e))?.len();

    let mut offset = 0u64;
    let mut first = true;
    let mut seen_moov = false;
    let mut seen_mdat_before_moov = false;
    let mut seen_sidx = false;

    while offset + 8 <= file_len {
        file.seek(SeekFrom::Start(offset))
            .map_err(|e| format!("seek: {}", e))?;
        let mut header = [0u8; 8];
        file.read_exact(&mut header)
            .map_err(|e| format!("read: {}", e))?;
        let size32 = u32::from_be_bytes(header[0..4].try_into().unwrap()) as u64;
        let atom_type = &header[4..8];

        if first {
            // QuickTime files may lack ftyp, but always start with one of these.
            let known = [b"ftyp", b"moov", b"mdat", b"wide", b"free", b"skip"];
            if !known.iter().any(|t| t.as_slice() == atom_type) {
                return Ok(Layout::NotMp4);
            }
            first = false;
        }

        let size = match size32 {
            0 => file_len - offset,
            1 => {
                let mut large = [0u8; 8];
                file.read_exact(&mut large)
                    .map_err(|e| format!("read: {}", e))?;
                u64::from_be_bytes(large)
            }
            n => n,
        };
        if size < 8 {
            return Err(format!("invalid atom size {} at offset {}", size, offset));
        }

        match atom_type {
            b"moov" => seen_moov = true,
            b"mdat" if !seen_moov => seen_mdat_before_moov = true,
            b"sidx" => seen_sidx = true,
            b"moof" => {
                return Ok(if seen_sidx {
                    Layout::Ok
                } else {
                    Layout::FragmentedUnindexed
                })
            }
            _ => {}
        }

        offset = offset
            .checked_add(size)
            .ok_or_else(|| "atom size overflow".to_string())?;
    }

    if offset > file_len {
        return Err("truncated (last atom extends past end of file)".to_string());
    }
    if !seen_moov {
        return Err("no moov atom".to_string());
    }
    Ok(if seen_mdat_before_moov {
        Layout::MoovAtEnd
    } else {
        Layout::Ok
    })
}

struct ProbeInfo {
    duration: f64,
    streams: usize,
}

fn probe(path: &Path) -> Result<ProbeInfo, String> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration:stream=index",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .map_err(|e| format!("failed to run ffprobe: {}", e))?;
    if !output.status.success() {
        return Err(format!(
            "ffprobe failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let json: Value =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("bad ffprobe output: {}", e))?;
    let duration = json["format"]["duration"]
        .as_str()
        .and_then(|d| d.parse::<f64>().ok())
        .ok_or_else(|| "ffprobe reported no duration".to_string())?;
    let streams = json["streams"].as_array().map(|s| s.len()).unwrap_or(0);
    Ok(ProbeInfo { duration, streams })
}

fn temp_path_for(path: &Path) -> PathBuf {
    let filename = path.file_name().unwrap().to_string_lossy();
    path.with_file_name(format!(".{}.remux-tmp", filename))
}

/// Remux `path` in place. The original is only replaced once the output has
/// been verified; on any failure the original is left untouched.
fn remux_in_place(path: &Path) -> Result<(), String> {
    let temp = temp_path_for(path);
    let result = remux_to_temp_and_verify(path, &temp).and_then(|()| {
        // Keep the original's permissions; ownership stays whoever runs this.
        if let Ok(meta) = std::fs::metadata(path) {
            let _ = std::fs::set_permissions(&temp, meta.permissions());
        }
        std::fs::rename(&temp, path).map_err(|e| format!("rename: {}", e))
    });
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

fn remux_to_temp_and_verify(path: &Path, temp: &Path) -> Result<(), String> {
    let before = probe(path)?;

    let is_mov = path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("mov"));
    let output = Command::new("ffmpeg")
        .args(["-nostdin", "-v", "error", "-y", "-i"])
        .arg(path)
        .args(["-map", "0", "-c", "copy", "-movflags", "+faststart"])
        .args(["-f", if is_mov { "mov" } else { "mp4" }])
        .arg(temp)
        .output()
        .map_err(|e| format!("failed to run ffmpeg: {}", e))?;
    if !output.status.success() {
        return Err(format!(
            "ffmpeg failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let layout = classify(temp).map_err(|e| format!("output unparseable: {}", e))?;
    if layout != Layout::Ok {
        return Err(format!("output still has layout {:?}", layout));
    }

    let after = probe(temp)?;
    if after.streams != before.streams {
        return Err(format!(
            "stream count changed ({} -> {})",
            before.streams, after.streams
        ));
    }
    let tolerance = (before.duration * 0.005).max(1.0);
    if (after.duration - before.duration).abs() > tolerance {
        return Err(format!(
            "duration changed ({:.3}s -> {:.3}s)",
            before.duration, after.duration
        ));
    }

    // A stream copy should be within a few percent of the original size.
    let size_before = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let size_after = std::fs::metadata(temp).map(|m| m.len()).unwrap_or(0);
    if (size_after as f64) < (size_before as f64) * 0.9 {
        return Err(format!(
            "output much smaller than input ({} -> {})",
            size_before, size_after
        ));
    }

    Ok(())
}

#[derive(Serialize)]
struct FileReport {
    filename: String,
    layout: Layout,
    size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    remuxed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Serialize, Default)]
struct SiteReport {
    slug: String,
    path: String,
    ok: usize,
    needs_remux: usize,
    remuxed: usize,
    failed: usize,
    not_mp4: usize,
    unparseable: usize,
    missing: usize,
    rewrite_bytes: u64,
    /// Every file that isn't `ok`.
    files: Vec<FileReport>,
}

impl SiteReport {
    fn remaining(&self) -> usize {
        self.needs_remux - self.remuxed
    }
}

fn video_filenames(work_dir: &WorkDir) -> Vec<String> {
    let mut filenames = vec![];
    for item in work_dir.crawled.values() {
        for file in item.flat_files().values() {
            if let FileCrawlType::Video {
                filename,
                downloaded: true,
                ..
            } = file
            {
                if !filenames.contains(filename) {
                    filenames.push(filename.clone());
                }
            }
        }
    }
    filenames
}

fn process_site(work_dir: &WorkDir, apply: bool) -> SiteReport {
    let mut report = SiteReport {
        slug: work_dir.config.slug.clone(),
        path: work_dir.path.display().to_string(),
        ..Default::default()
    };

    for filename in video_filenames(work_dir) {
        let path = work_dir.path.join(&filename);
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);

        let (layout, error) = if !path.exists() {
            (Layout::Missing, None)
        } else {
            match classify(&path) {
                Ok(layout) => (layout, None),
                Err(e) => (Layout::Unparseable, Some(e)),
            }
        };

        match layout {
            Layout::Ok => {
                report.ok += 1;
                continue;
            }
            Layout::NotMp4 => report.not_mp4 += 1,
            Layout::Missing => report.missing += 1,
            Layout::Unparseable => {
                report.unparseable += 1;
                println!(
                    "{}: unparseable: {}",
                    filename,
                    error.as_deref().unwrap_or("")
                );
            }
            Layout::MoovAtEnd | Layout::FragmentedUnindexed => {
                report.needs_remux += 1;
                report.rewrite_bytes += size;
            }
        }

        let mut file_report = FileReport {
            filename: filename.clone(),
            layout,
            size,
            remuxed: None,
            error,
        };

        if layout.needs_remux() {
            if apply {
                println!(
                    "{}: {:?}, remuxing ({})",
                    filename,
                    layout,
                    human_bytes(size)
                );
                match remux_in_place(&path) {
                    Ok(()) => {
                        report.remuxed += 1;
                        file_report.remuxed = Some(true);
                        println!("{}: remuxed", filename);
                    }
                    Err(e) => {
                        report.failed += 1;
                        report.rewrite_bytes -= size;
                        file_report.remuxed = Some(false);
                        println!("{}: remux failed: {}", filename, e);
                        file_report.error = Some(e);
                    }
                }
            } else {
                println!("{}: {:?} ({})", filename, layout, human_bytes(size));
            }
        }

        report.files.push(file_report);
    }

    report
}

fn markdown_report(reports: &[SiteReport], apply: bool) -> String {
    let mut md = String::new();
    let _ = writeln!(
        md,
        "# Remux {}\n",
        if apply { "apply" } else { "scan (dry run)" }
    );
    let _ = writeln!(
        md,
        "| Site | OK | Needs remux | Remuxed | Failed | Not MP4 | Unparseable | Missing | Rewrite size |"
    );
    let _ = writeln!(md, "|---|---|---|---|---|---|---|---|---|");
    for r in reports {
        let _ = writeln!(
            md,
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            r.slug,
            r.ok,
            r.needs_remux,
            r.remuxed,
            r.failed,
            r.not_mp4,
            r.unparseable,
            r.missing,
            human_bytes(r.rewrite_bytes)
        );
    }

    for r in reports {
        if r.files.is_empty() {
            continue;
        }
        let _ = writeln!(md, "\n## {} (`{}`)\n", r.slug, r.path);
        let _ = writeln!(md, "| File | Layout | Size | Result |");
        let _ = writeln!(md, "|---|---|---|---|");
        for f in &r.files {
            let result = match (f.remuxed, &f.error) {
                (Some(true), _) => "remuxed".to_string(),
                (_, Some(e)) => e.replace('|', "\\|").replace('\n', " "),
                (None, None) if f.layout.needs_remux() => "would remux".to_string(),
                _ => "skipped".to_string(),
            };
            let layout = serde_json::to_value(f.layout)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_default();
            let _ = writeln!(
                md,
                "| `{}` | {} | {} | {} |",
                f.filename,
                layout,
                human_bytes(f.size),
                result
            );
        }
    }
    md
}

pub fn remux_all(work_dirs: &[WorkDir], apply: bool) {
    let job_output = JobOutput::detect();
    let command = if apply { "remux apply" } else { "remux scan" };
    let slugs: Vec<String> = work_dirs.iter().map(|w| w.config.slug.clone()).collect();
    job_output.label(&run_label(command, &slugs));

    let mut reports = vec![];
    for work_dir in work_dirs {
        if work_dir.remote_url.is_some() {
            println!("Skipping remote site: {}", work_dir.config.label);
            continue;
        }
        println!(
            "Remux {}: {}",
            if apply { "applying" } else { "scanning" },
            work_dir.config.label
        );
        reports.push(process_site(work_dir, apply));
    }

    let sum = |f: fn(&SiteReport) -> usize| reports.iter().map(f).sum::<usize>();
    let ok = sum(|r| r.ok);
    let needs_remux = sum(|r| r.needs_remux);
    let remuxed = sum(|r| r.remuxed);
    let failed = sum(|r| r.failed);
    let not_mp4 = sum(|r| r.not_mp4);
    let unparseable = sum(|r| r.unparseable);
    let missing = sum(|r| r.missing);
    let remaining = sum(|r| r.remaining());
    let rewrite_bytes: u64 = reports.iter().map(|r| r.rewrite_bytes).sum();

    println!(
        "Done. ok={} needs_remux={} remuxed={} failed={} not_mp4={} unparseable={} missing={} rewrite={}",
        ok,
        needs_remux,
        remuxed,
        failed,
        not_mp4,
        unparseable,
        missing,
        human_bytes(rewrite_bytes)
    );

    let mut chips = vec![
        Chip::new("ok", ok, Some("success")),
        Chip::new(
            "needs remux",
            needs_remux,
            Some(if remaining > 0 { "warning" } else { "success" }),
        ),
    ];
    if apply {
        chips.push(Chip::new("remuxed", remuxed, Some("success")));
    }
    if failed > 0 {
        chips.push(Chip::new("failed", failed, Some("danger")));
    }
    if not_mp4 > 0 {
        chips.push(Chip::new("not mp4", not_mp4, None));
    }
    if unparseable > 0 {
        chips.push(Chip::new("unparseable", unparseable, Some("danger")));
    }
    if missing > 0 {
        chips.push(Chip::new("missing", missing, None));
    }
    chips.push(Chip::new("rewrite size", human_bytes(rewrite_bytes), None));
    job_output.chips(&chips);

    let mut metrics = vec![];
    for r in &reports {
        let site = [("site", r.slug.as_str())];
        metrics.push(Metric::new(
            "site_server_remux_needs_remux",
            r.remaining() as f64,
            MetricType::Gauge,
            &site,
        ));
        if apply {
            metrics.push(Metric::new(
                "site_server_remux_files_total",
                r.remuxed as f64,
                MetricType::Counter,
                &[("site", r.slug.as_str()), ("result", "remuxed")],
            ));
            metrics.push(Metric::new(
                "site_server_remux_files_total",
                r.failed as f64,
                MetricType::Counter,
                &[("site", r.slug.as_str()), ("result", "failed")],
            ));
            metrics.push(Metric::new(
                "site_server_remux_bytes_total",
                r.rewrite_bytes as f64,
                MetricType::Counter,
                &site,
            ));
        }
    }
    job_output.metrics(&metrics);

    job_output.write("report.md", &markdown_report(&reports, apply));
    job_output.write_json(
        "result.json",
        &serde_json::json!({
            "apply": apply,
            "sites": reports,
        }),
    );
}
