//! The cases bracket what the shell actually draws. A bar is redrawn only
//! when something on it changes, so its cost is paid rarely; a full-screen
//! surface is what an animation would redraw every frame, and it is where
//! optimisations such as filling solid interiors directly pay off. Many
//! small shapes measure the per-shape overhead that large ones hide.
//! Throughput is reported in pixels so that cases of different sizes compare
//! directly.

use std::hint::black_box;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use renderer::cpu;
use renderer::{Circle, Point, Rect, Rgba, Scene, Shape};

/// Sized in logical pixels, as the shell lays it out, so that the same scene
/// can be measured at different output scales.
struct Case {
    name: &'static str,
    size: (f32, f32),
    scale: f32,
    scene: Scene,
}

fn background(width: f32, height: f32) -> Rect {
    Rect {
        p0: Point::new(0.0, 0.0),
        p1: Point::new(width, height),
        color: Rgba::from_hex(0x111318, 1.0),
    }
}

/// The shapes the shell draws today, so that this case tracks its real cost.
fn bar(width: f32) -> Scene {
    let gold = Rgba::from_hex(0xc9a96e, 1.0);
    let logo = Circle {
        center: Point::new(22.0, 22.0),
        r: 8.0,
        color: gold.alpha(0.0),
    };
    let mut scene = Scene::new();
    scene.push(background(width, 44.0));
    scene.push(logo.outline(2.2, gold));
    scene.push(Circle {
        r: 2.6,
        color: gold,
        ..logo
    });
    scene
}

fn icons(width: f32) -> Scene {
    let mut scene = Scene::new();
    scene.push(background(width, 44.0));
    for i in 0..64 {
        let dot = Circle {
            center: Point::new(22.0 + i as f32 * 36.0, 22.0),
            r: 10.0,
            color: Rgba::from_hex(0x9fb4c7, 0.6),
        };
        scene.push(dot);
        scene.push(dot.outline(1.5, Rgba::from_hex(0xe8e3d8, 1.0)));
    }
    scene
}

fn fullscreen(width: f32, height: f32) -> Scene {
    let mut scene = Scene::new();
    scene.push(background(width, height));
    scene
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "bar/2560x44@1",
            size: (2560.0, 44.0),
            scale: 1.0,
            scene: bar(2560.0),
        },
        Case {
            name: "bar/1920x44@2",
            size: (1920.0, 44.0),
            scale: 2.0,
            scene: bar(1920.0),
        },
        Case {
            name: "icons/2560x44@1",
            size: (2560.0, 44.0),
            scale: 1.0,
            scene: icons(2560.0),
        },
        Case {
            name: "fullscreen/1920x1080@2",
            size: (1920.0, 1080.0),
            scale: 2.0,
            scene: fullscreen(1920.0, 1080.0),
        },
    ]
}

fn draw(c: &mut Criterion) {
    let mut group = c.benchmark_group("draw");
    for case in cases() {
        let w = (case.size.0 * case.scale) as usize;
        let h = (case.size.1 * case.scale) as usize;
        let mut px = vec![0u8; w * h * 4];
        group.throughput(Throughput::Elements((w * h) as u64));
        // Full-screen frames take tens of milliseconds; the default 100
        // samples would make a run take minutes without adding precision.
        group.sample_size(if w * h > 1_000_000 { 20 } else { 100 });
        group.bench_function(case.name, |b| {
            b.iter(|| {
                cpu::draw(
                    black_box(px.as_mut_slice()),
                    w,
                    h,
                    case.scale,
                    &case.scene.shapes,
                )
            })
        });
    }
    group.finish();
}

criterion_group!(benches, draw);
criterion_main!(benches);
