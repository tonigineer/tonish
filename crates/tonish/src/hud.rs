//! Figures on how tonish runs, in the top left corner of every monitor:
//! frames a second, how long a frame takes and where it is drawn, CPU and
//! memory. `tonish msg hud` hides and shows them.

use std::collections::VecDeque;
use std::fs;
use std::process::Command;
use std::time::{Duration, Instant};

use engine::{Anchor, Drawn, Layer, On, Output, SurfaceSpec};
use renderer::{Font, Point, RoundedRect, Scene};
use tracing::{debug, warn};

use crate::theme::Theme;

/// Logical pixels.
const SIZE: (f32, f32) = (300.0, 92.0);
const MARGIN: i32 = 8;
const PADDING: f32 = 8.0;
const TEXT: f32 = 12.0;
/// Often enough to follow a figure, seldom enough to cost nothing.
pub const FPS: f32 = 2.0;

pub struct Hud {
    pub shown: bool,
    font: Option<Font>,
    /// Asked for once, so that a system without a font warns once.
    looked_for_font: bool,
    /// The frames of every surface but these figures' own, of the last
    /// second.
    frames: VecDeque<(Instant, On, Drawn)>,
    gpu: Option<String>,
    cpu: Cpu,
}

/// The process's CPU time, in nanoseconds, at the last update, to tell how
/// much it used since.
struct Cpu {
    last: Option<(Instant, u64)>,
    percent: f32,
}

impl Default for Hud {
    fn default() -> Self {
        Self {
            shown: true,
            font: None,
            looked_for_font: false,
            frames: VecDeque::new(),
            gpu: None,
            cpu: Cpu {
                last: None,
                percent: 0.0,
            },
        }
    }
}

impl Hud {
    /// Below anything that keeps room at the top, such as the bar, with an
    /// exclusive zone of 0, and letting every click through to the windows
    /// under it.
    pub fn surface() -> SurfaceSpec {
        SurfaceSpec {
            name: "hud",
            layer: Layer::Overlay,
            anchor: Anchor::TOP | Anchor::LEFT,
            size: (SIZE.0 as u32, SIZE.1 as u32),
            exclusive: 0,
            margin: (MARGIN, 0, 0, MARGIN),
            input: false,
        }
    }

    pub fn drew(&mut self, on: On, drawn: Drawn, now: Instant) {
        self.frames.push_back((now, on, drawn));
        self.forget(now);
    }

    pub fn gpu_started(&mut self, device: &str) {
        self.gpu = Some(device.to_owned());
    }

    fn forget(&mut self, now: Instant) {
        while self
            .frames
            .front()
            .is_some_and(|(t, _, _)| now.duration_since(*t) > Duration::from_secs(1))
        {
            self.frames.pop_front();
        }
    }

    pub fn draw(&mut self, scene: &mut Scene, out: &Output, scale: f32, now: Instant, t: &Theme) {
        self.forget(now);
        self.cpu.update(now);
        scene.push(RoundedRect {
            p0: Point::new(0.0, 0.0),
            p1: Point::new(SIZE.0, SIZE.1),
            radii: [8.0; 4],
            color: t.surface.alpha(0.85),
        });
        let lines = self.lines(out);
        let Some(font) = self.font() else {
            return;
        };
        let line = font.line_height(TEXT);
        for (i, text) in lines.iter().enumerate() {
            let at = Point::new(PADDING, PADDING + i as f32 * line);
            let color = if i == 0 { t.primary } else { t.on_surface };
            scene.push(font.text(text, at, TEXT, scale, color));
        }
    }

    fn lines(&self, out: &Output) -> Vec<String> {
        let fps = self.fps();
        let mut took: Vec<Duration> = self.frames.iter().map(|(_, _, d)| d.took).collect();
        took.sort_unstable();
        let ms = |d: Option<&Duration>| d.map_or(0.0, |d| d.as_secs_f64() * 1000.0);
        let on = match self.frames.back() {
            Some((_, _, d)) if d.gpu => "gpu",
            Some(_) => "cpu",
            None => "-",
        };
        // NVIDIA's names end in the bus and the instruction set, as in
        // "NVIDIA GeForce RTX 4090/PCIe/SSE2".
        let device = self
            .gpu
            .as_deref()
            .map_or("not started", |d| d.split('/').next().unwrap_or(d));
        vec![
            format!("tonish {}", env!("CARGO_PKG_VERSION")),
            format!(
                "fps {fps:<4} frame {:.2} ms  max {:.2} ms",
                ms(took.get(took.len() / 2)),
                ms(took.last())
            ),
            format!(
                "cpu {:.1} %  mem {:.1} MiB  on {on}",
                self.cpu.percent,
                rss_mib()
            ),
            format!("gpu {device}"),
            format!(
                "{} {}x{} @{}",
                out.name.as_deref().unwrap_or("unnamed"),
                out.size.0,
                out.size.1,
                out.scale
            ),
        ]
    }

    /// Of the surface drawn most often, so that an animation on one monitor
    /// isn't counted twice by the same frame on another.
    fn fps(&self) -> usize {
        let mut counts: Vec<(On, usize)> = Vec::new();
        for (_, on, _) in &self.frames {
            match counts.iter_mut().find(|(o, _)| o == on) {
                Some((_, n)) => *n += 1,
                None => counts.push((*on, 1)),
            }
        }
        counts.iter().map(|(_, n)| *n).max().unwrap_or(0)
    }

    /// The system's monospace font, as fontconfig picks it, loaded at the
    /// first draw rather than at start, so that tonish starts as fast
    /// without the figures.
    fn font(&mut self) -> Option<&mut Font> {
        if !self.looked_for_font {
            self.looked_for_font = true;
            match monospace() {
                Ok(font) => self.font = Some(font),
                Err(e) => warn!("the figures show without text, for want of a font: {e}"),
            }
        }
        self.font.as_mut()
    }
}

fn monospace() -> Result<Font, String> {
    let started = Instant::now();
    let out = Command::new("fc-match")
        .args(["--format=%{file}", "monospace"])
        .output()
        .map_err(|e| format!("cannot run fc-match: {e}"))?;
    let path = String::from_utf8_lossy(&out.stdout).into_owned();
    if !out.status.success() || path.is_empty() {
        return Err("fc-match names no monospace font".into());
    }
    let bytes = fs::read(&path).map_err(|e| format!("{path}: {e}"))?;
    let font = Font::from_bytes(&bytes).map_err(|e| format!("{path}: {e}"))?;
    debug!(file = %path, took = ?started.elapsed(), "font loaded");
    Ok(font)
}

impl Cpu {
    /// As a share of one core, the way top shows a process.
    fn update(&mut self, now: Instant) {
        let Some(ns) = cpu_ns() else {
            return;
        };
        if let Some((then, before)) = self.last {
            let wall = now.duration_since(then).as_nanos() as f32;
            if wall > 0.0 {
                self.percent = ns.saturating_sub(before) as f32 / wall * 100.0;
            }
        }
        self.last = Some((now, ns));
    }
}

/// The time every thread, the GPU's included, has spent on a CPU, from the
/// scheduler in nanoseconds. /proc/self/stat counts it in ticks of 10 ms,
/// too coarse for an update twice a second, where one tick is 2 %.
fn cpu_ns() -> Option<u64> {
    let mut total = 0u64;
    for task in fs::read_dir("/proc/self/task").ok()?.flatten() {
        // A thread that ended since the folder was listed has no file left.
        if let Ok(stat) = fs::read_to_string(task.path().join("schedstat"))
            && let Some(ns) = stat
                .split_whitespace()
                .next()
                .and_then(|n| n.parse::<u64>().ok())
        {
            total += ns;
        }
    }
    Some(total)
}

fn rss_mib() -> f32 {
    fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            let line = s.lines().find(|l| l.starts_with("VmRSS:"))?;
            line.split_whitespace().nth(1)?.parse::<f32>().ok()
        })
        .map_or(0.0, |kib| kib / 1024.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine::{OutputId, SurfaceId};

    fn drawn(ms: u64) -> Drawn {
        Drawn {
            took: Duration::from_millis(ms),
            waited: Duration::ZERO,
            gpu: true,
        }
    }

    #[test]
    fn frames_a_second_count_the_busiest_surface_of_the_last_second() {
        let mut hud = Hud::default();
        let start = Instant::now();
        let a = On::Surface(OutputId(1), SurfaceId(0));
        for i in 0..30 {
            let t = start + Duration::from_millis(i * 33);
            hud.drew(On::Popup, drawn(1), t);
            if i % 3 == 0 {
                hud.drew(a, drawn(1), t);
            }
        }
        assert_eq!(hud.fps(), 30);
        hud.forget(start + Duration::from_millis(2000));
        assert_eq!(hud.fps(), 0);
    }

    #[test]
    fn the_lines_name_the_figures() {
        let mut hud = Hud::default();
        hud.gpu_started("NVIDIA GeForce RTX 4090/PCIe/SSE2");
        hud.drew(On::Popup, drawn(2), Instant::now());
        let out = Output {
            id: OutputId(1),
            name: Some("DP-1".into()),
            size: (1920.0, 1080.0),
            scale: 2,
        };
        let lines = hud.lines(&out);
        assert!(lines[1].starts_with("fps 1 "), "{lines:?}");
        assert!(lines[1].contains("frame 2.00 ms"), "{lines:?}");
        assert!(lines[2].ends_with("on gpu"), "{lines:?}");
        assert_eq!(lines[3], "gpu NVIDIA GeForce RTX 4090");
        assert_eq!(lines[4], "DP-1 1920x1080 @2");
    }

    #[test]
    fn this_process_has_cpu_time_and_memory() {
        assert!(cpu_ns().is_some_and(|ns| ns > 0));
        assert!(rss_mib() > 0.0);
    }
}
