//! Direct2D drawing.
//!
//! The bar is one strip of text and a handful of rounded rectangles, so it
//! draws straight onto an `ID2D1HwndRenderTarget` instead of going anywhere
//! near a browser engine. Everything here works in physical pixels: the render
//! target is pinned to 96 DPI and the caller multiplies by the scale factor of
//! the display it is drawing on.

use windows::core::HSTRING;
use windows::Win32::Foundation::{D2DERR_RECREATE_TARGET, HWND};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_IGNORE, D2D1_COLOR_F, D2D1_PIXEL_FORMAT, D2D_RECT_F, D2D_SIZE_U,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1Factory, ID2D1HwndRenderTarget, ID2D1SolidColorBrush,
    D2D1_DRAW_TEXT_OPTIONS_CLIP, D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_FEATURE_LEVEL_DEFAULT,
    D2D1_HWND_RENDER_TARGET_PROPERTIES, D2D1_PRESENT_OPTIONS_IMMEDIATELY,
    D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT, D2D1_RENDER_TARGET_USAGE_NONE,
    D2D1_ROUNDED_RECT,
};
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat, IDWriteTextLayout,
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
    DWRITE_TEXT_METRICS, DWRITE_TRIMMING, DWRITE_TRIMMING_GRANULARITY_CHARACTER,
    DWRITE_WORD_WRAPPING_NO_WRAP,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows_numerics::Vector2;

use wintilex_core::geometry::Rect;

use crate::segments::{Act, Emphasis, Sections, Segment};
use crate::theme::Palette;

/// Space either side of the text inside a segment, in logical pixels.
const SEGMENT_PADDING: f32 = 9.0;
/// Space between two segments.
const SEGMENT_GAP: f32 = 5.0;
/// Space between the outermost segment and the edge of the screen.
const EDGE_PADDING: f32 = 10.0;
/// How far a pill stops short of the top and bottom of the bar.
const PILL_INSET: f32 = 4.0;
const PILL_RADIUS: f32 = 4.0;

/// The factories, which outlive any individual bar window.
pub struct Painter {
    d2d: ID2D1Factory,
    dwrite: IDWriteFactory,
}

impl Painter {
    pub fn new() -> Option<Painter> {
        let d2d: ID2D1Factory =
            unsafe { D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None) }
                .map_err(|error| log::error!("no Direct2D factory: {error}"))
                .ok()?;
        let dwrite: IDWriteFactory = unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED) }
            .map_err(|error| log::error!("no DirectWrite factory: {error}"))
            .ok()?;
        Some(Painter { d2d, dwrite })
    }

    /// A render target bound to one bar window.
    pub fn target(&self, hwnd: HWND, size: (i32, i32)) -> Option<ID2D1HwndRenderTarget> {
        let properties = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_IGNORE,
            },
            // Pinned, so a segment laid out at 30 pixels is 30 pixels.
            dpiX: 96.0,
            dpiY: 96.0,
            usage: D2D1_RENDER_TARGET_USAGE_NONE,
            minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
        };
        let window = D2D1_HWND_RENDER_TARGET_PROPERTIES {
            hwnd,
            pixelSize: D2D_SIZE_U { width: size.0.max(1) as u32, height: size.1.max(1) as u32 },
            presentOptions: D2D1_PRESENT_OPTIONS_IMMEDIATELY,
        };

        unsafe { self.d2d.CreateHwndRenderTarget(&properties, &window) }
            .map_err(|error| log::error!("no render target: {error}"))
            .ok()
    }

    /// Two weights: the focused window is the one thing on the bar worth
    /// picking out without colour.
    pub fn text_format(&self, family: &str, size: f32, bold: bool) -> Option<IDWriteTextFormat> {
        let weight = if bold { DWRITE_FONT_WEIGHT_SEMI_BOLD } else { DWRITE_FONT_WEIGHT_NORMAL };
        let format = unsafe {
            self.dwrite.CreateTextFormat(
                &HSTRING::from(family),
                None,
                weight,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                size.max(6.0),
                &HSTRING::from("en-us"),
            )
        }
        .ok()?;

        unsafe {
            let _ = format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
            let _ = format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
        }
        Some(format)
    }

    fn layout(
        &self,
        text: &str,
        format: &IDWriteTextFormat,
        width: f32,
        height: f32,
    ) -> Option<Measured> {
        let wide: Vec<u16> = text.encode_utf16().collect();
        let layout =
            unsafe { self.dwrite.CreateTextLayout(&wide, format, width.max(0.0), height) }.ok()?;

        // Long titles are cut with an ellipsis rather than spilling into the
        // next segment.
        if let Ok(sign) = unsafe { self.dwrite.CreateEllipsisTrimmingSign(format) } {
            let trimming = DWRITE_TRIMMING {
                granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                delimiter: 0,
                delimiterCount: 0,
            };
            unsafe {
                let _ = layout.SetTrimming(&trimming, &sign);
            }
        }

        let mut metrics = DWRITE_TEXT_METRICS::default();
        unsafe { layout.GetMetrics(&mut metrics) }.ok()?;
        Some(Measured { layout, width: metrics.width })
    }
}

struct Measured {
    layout: IDWriteTextLayout,
    width: f32,
}

/// A segment that has been given somewhere to go.
struct Placed {
    layout: IDWriteTextLayout,
    box_rect: D2D_RECT_F,
    text_x: f32,
    emphasis: Emphasis,
    action: Option<Act>,
}

/// Where the clickable segments ended up, in window coordinates.
pub type HitBoxes = Vec<(Rect, Act)>;

/// Returned by [`draw`], because a paint can fail in a way the caller has to
/// act on.
pub struct Painted {
    pub hits: HitBoxes,
    /// The graphics device went away, usually a driver reset or an RDP session
    /// changing hands. The render target has to be built again before the next
    /// paint will show anything.
    pub device_lost: bool,
}

/// Draw one bar and report back what the user can click on.
pub fn draw(
    painter: &Painter,
    target: &ID2D1HwndRenderTarget,
    fonts: (&IDWriteTextFormat, &IDWriteTextFormat),
    palette: &Palette,
    sections: &Sections,
    size: (f32, f32),
    scale: f32,
) -> Painted {
    let (width, height) = size;
    let edge = EDGE_PADDING * scale;
    let gap = SEGMENT_GAP * scale;

    let measure = |segment: &Segment, limit: f32| -> Option<Measured> {
        let format = if segment.emphasis == Emphasis::Focused { fonts.1 } else { fonts.0 };
        painter.layout(&segment.text, format, limit, height)
    };

    let left = prepare(&sections.left, &measure, width, scale);
    let right = prepare(&sections.right, &measure, width, scale);
    let left_span = span(&left, gap, scale);
    let right_span = span(&right, gap, scale);

    // Whatever the two ends did not take, minus a gap on either side so the
    // centre never touches them.
    let taken = left_span + right_span + 2.0 * edge + 2.0 * gap;
    let centre = prepare(&sections.center, &measure, (width - taken).max(0.0), scale);
    let centre_span = span(&centre, gap, scale);

    // Centred against the whole bar where there is room for it, pushed aside
    // rather than overlapped where there is not.
    let right_start = width - edge - right_span;
    let earliest = edge + left_span + gap;
    let latest = (right_start - gap - centre_span).max(earliest);
    let centre_x = ((width - centre_span) / 2.0).clamp(earliest, latest);

    let mut placed = Vec::with_capacity(left.len() + centre.len() + right.len());
    place(&mut placed, left, edge, gap, scale);
    place(&mut placed, centre, centre_x, gap, scale);
    place(&mut placed, right, right_start, gap, scale);

    unsafe {
        target.BeginDraw();
        target.Clear(Some(&palette.background));
    }

    let brush = |colour: D2D1_COLOR_F| -> Option<ID2D1SolidColorBrush> {
        unsafe { target.CreateSolidColorBrush(&colour, None) }.ok()
    };

    let mut hits = Vec::new();
    for item in &placed {
        let (fill, text) = colours(item.emphasis, palette);

        if let Some(fill) = fill {
            if let Some(brush) = brush(fill) {
                let pill = D2D1_ROUNDED_RECT {
                    rect: D2D_RECT_F {
                        left: item.box_rect.left,
                        top: PILL_INSET * scale,
                        right: item.box_rect.right,
                        bottom: height - PILL_INSET * scale,
                    },
                    radiusX: PILL_RADIUS * scale,
                    radiusY: PILL_RADIUS * scale,
                };
                unsafe { target.FillRoundedRectangle(&pill, &brush) };
            }
        }

        if let Some(brush) = brush(text) {
            unsafe {
                target.DrawTextLayout(
                    Vector2 { X: item.text_x, Y: 0.0 },
                    &item.layout,
                    &brush,
                    D2D1_DRAW_TEXT_OPTIONS_CLIP,
                );
            }
        }

        if let Some(action) = item.action {
            hits.push((
                Rect::from_edges(
                    item.box_rect.left as i32,
                    0,
                    item.box_rect.right as i32,
                    height as i32,
                ),
                action,
            ));
        }
    }

    // `EndDraw` is where a device that went away is reported, one paint after
    // the fact. Everything drawn above simply never reached the screen.
    let device_lost = match unsafe { target.EndDraw(None, None) } {
        Ok(()) => false,
        Err(error) if error.code() == D2DERR_RECREATE_TARGET => true,
        Err(error) => {
            log::warn!("bar paint failed: {error}");
            false
        }
    };

    Painted { hits, device_lost }
}

/// Measure a section, giving the flexible segments whatever is left over.
fn prepare(
    segments: &[Segment],
    measure: &dyn Fn(&Segment, f32) -> Option<Measured>,
    limit: f32,
    scale: f32,
) -> Vec<(Measured, Emphasis, Option<Act>)> {
    let padding = SEGMENT_PADDING * scale;
    let fixed: f32 = segments
        .iter()
        .filter(|segment| !segment.flexible)
        .filter_map(|segment| measure(segment, limit).map(|m| m.width + 2.0 * padding))
        .sum();

    let spare = (limit - fixed).max(2.0 * padding);

    segments
        .iter()
        .filter_map(|segment| {
            let room = if segment.flexible { spare - 2.0 * padding } else { limit };
            let measured = measure(segment, room)?;
            (measured.width > 0.5).then_some((measured, segment.emphasis, segment.action))
        })
        .collect()
}

/// Total width of a prepared section, padding and gaps included.
fn span(section: &[(Measured, Emphasis, Option<Act>)], gap: f32, scale: f32) -> f32 {
    let text: f32 = section.iter().map(|(measured, _, _)| measured.width).sum();
    let count = section.len() as f32;
    text + count * 2.0 * SEGMENT_PADDING * scale + (count - 1.0).max(0.0) * gap
}

fn place(
    out: &mut Vec<Placed>,
    section: Vec<(Measured, Emphasis, Option<Act>)>,
    start: f32,
    gap: f32,
    scale: f32,
) {
    let padding = SEGMENT_PADDING * scale;
    let mut x = start;

    for (measured, emphasis, action) in section {
        let width = measured.width + 2.0 * padding;
        out.push(Placed {
            layout: measured.layout,
            box_rect: D2D_RECT_F { left: x, top: 0.0, right: x + width, bottom: 0.0 },
            text_x: x + padding,
            emphasis,
            action,
        });
        x += width + gap;
    }
}

/// The fill behind a segment, if it has one, and the colour of its text.
fn colours(emphasis: Emphasis, palette: &Palette) -> (Option<D2D1_COLOR_F>, D2D1_COLOR_F) {
    match emphasis {
        Emphasis::Normal => (None, palette.foreground),
        Emphasis::Muted => (None, palette.muted),
        Emphasis::Pill => (Some(palette.surface), palette.foreground),
        Emphasis::Focused => (Some(palette.accent), palette.accent_text),
        Emphasis::Urgent => (Some(palette.urgent), palette.accent_text),
    }
}
