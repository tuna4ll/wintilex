//! Direct2D drawing.
//!
//! The bar is drawn into an off-screen bitmap and handed to the desktop with
//! `UpdateLayeredWindow`, rather than painted into its window. That is what
//! buys the look: every pixel carries its own alpha, so the groups can be
//! rounded, translucent and separated by gaps the wallpaper shows through, with
//! the corners antialiased instead of stepped.
//!
//! Everything here works in physical pixels: the render target is pinned to 96
//! DPI and the caller multiplies the sizes by the scale of the display.

use std::ffi::c_void;

use windows::core::HSTRING;
use windows::Win32::Foundation::{COLORREF, HWND, POINT, SIZE};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT, D2D_RECT_F,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1Factory, ID2D1RenderTarget, ID2D1SolidColorBrush,
    D2D1_DRAW_TEXT_OPTIONS_CLIP, D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_FEATURE_LEVEL_DEFAULT,
    D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT, D2D1_RENDER_TARGET_USAGE_NONE,
    D2D1_ROUNDED_RECT, D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE,
};
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat, IDWriteTextLayout,
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
    DWRITE_TEXT_METRICS, DWRITE_TRIMMING, DWRITE_TRIMMING_GRANULARITY_CHARACTER,
    DWRITE_WORD_WRAPPING_NO_WRAP,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject,
    AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS,
    HBITMAP, HDC, HGDIOBJ,
};
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICBitmap, IWICImagingFactory,
    WICBitmapCacheOnLoad,
};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::UI::WindowsAndMessaging::{UpdateLayeredWindow, ULW_ALPHA};
use windows_numerics::Vector2;

use wintilex_core::geometry::Rect;

use crate::segments::{Act, Emphasis, Sections, Segment};
use crate::theme::{fade, Palette};

/// Space either side of the segments inside a group.
const GROUP_PADDING: f32 = 11.0;
/// Space between two segments in the same group.
const SEGMENT_GAP: f32 = 9.0;
/// Space between two groups, kept clear so the centre never touches the ends.
const GROUP_GAP: f32 = 10.0;
/// Space between an icon and the text after it.
const ICON_GAP: f32 = 7.0;
/// Space either side of the text inside a pill.
const PILL_PADDING: f32 = 9.0;
/// How far a pill stops short of the top and bottom of its group.
const PILL_INSET: f32 = 5.0;
/// Icons read large next to text of the same nominal size.
const ICON_SCALE: f32 = 0.95;

/// The factories, which outlive any individual bar window.
pub struct Painter {
    d2d: ID2D1Factory,
    dwrite: IDWriteFactory,
    wic: IWICImagingFactory,
}

impl Painter {
    /// COM has to have been started on this thread already; the bar thread does
    /// that before anything else.
    pub fn new() -> Option<Painter> {
        let d2d: ID2D1Factory =
            unsafe { D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None) }
                .map_err(|error| log::error!("no Direct2D factory: {error}"))
                .ok()?;
        let dwrite: IDWriteFactory = unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED) }
            .map_err(|error| log::error!("no DirectWrite factory: {error}"))
            .ok()?;
        let wic: IWICImagingFactory =
            unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER) }
                .map_err(|error| log::error!("no imaging factory: {error}"))
                .ok()?;

        Some(Painter { d2d, dwrite, wic })
    }

    /// An off-screen surface the size of one bar.
    pub fn surface(&self, size: (i32, i32)) -> Option<Surface> {
        let (width, height) = (size.0.max(1), size.1.max(1));

        let bitmap = unsafe {
            self.wic.CreateBitmap(
                width as u32,
                height as u32,
                &GUID_WICPixelFormat32bppPBGRA,
                WICBitmapCacheOnLoad,
            )
        }
        .map_err(|error| log::error!("no bitmap: {error}"))
        .ok()?;

        let properties = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                // Every pixel keeps its own alpha, which is what lets the
                // layered window show the wallpaper through the gaps.
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            // Pinned, so a segment laid out at 30 pixels is 30 pixels.
            dpiX: 96.0,
            dpiY: 96.0,
            usage: D2D1_RENDER_TARGET_USAGE_NONE,
            minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
        };

        let target = unsafe { self.d2d.CreateWicBitmapRenderTarget(&bitmap, &properties) }
            .map_err(|error| log::error!("no render target: {error}"))
            .ok()?;

        // ClearType assumes it knows what is behind the glyph, which on a
        // transparent surface it does not; grey antialiasing is the one that
        // composites correctly.
        unsafe { target.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE) };

        Surface::new(bitmap, target, width, height)
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

        // Long titles are cut with an ellipsis rather than spilling out of
        // their group.
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

/// The bitmap one bar is drawn into, and the GDI objects that carry it to the
/// screen.
pub struct Surface {
    bitmap: IWICBitmap,
    target: ID2D1RenderTarget,
    width: i32,
    height: i32,
    dc: HDC,
    dib: HBITMAP,
    previous: HGDIOBJ,
    bits: *mut c_void,
}

impl Surface {
    fn new(
        bitmap: IWICBitmap,
        target: ID2D1RenderTarget,
        width: i32,
        height: i32,
    ) -> Option<Surface> {
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                // Negative means the rows run top to bottom, which is the order
                // the bitmap hands its pixels over in.
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        unsafe {
            let screen = GetDC(None);
            let mut bits: *mut c_void = std::ptr::null_mut();
            let dib = CreateDIBSection(Some(screen), &info, DIB_RGB_COLORS, &mut bits, None, 0);
            let dc = CreateCompatibleDC(Some(screen));
            ReleaseDC(None, screen);

            let Ok(dib) = dib else {
                log::error!("could not create the bar bitmap");
                let _ = DeleteDC(dc);
                return None;
            };

            let previous = SelectObject(dc, dib.into());
            Some(Surface { bitmap, target, width, height, dc, dib, previous, bits })
        }
    }

    pub fn matches(&self, size: (i32, i32)) -> bool {
        self.width == size.0.max(1) && self.height == size.1.max(1)
    }

    pub fn size(&self) -> (f32, f32) {
        (self.width as f32, self.height as f32)
    }

    /// Hand the drawn bitmap to the desktop.
    ///
    /// This also moves and sizes the window, so a layered bar never needs a
    /// `WM_PAINT` at all.
    pub fn present(&self, hwnd: HWND, at: Rect, opacity: f32) -> bool {
        let stride = (self.width * 4) as u32;
        let length = (stride as usize) * (self.height as usize);

        let copied = unsafe {
            let buffer = std::slice::from_raw_parts_mut(self.bits as *mut u8, length);
            self.bitmap.CopyPixels(std::ptr::null(), stride, buffer)
        };
        if let Err(error) = copied {
            log::warn!("could not read the bar bitmap: {error}");
            return false;
        }

        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: (opacity.clamp(0.0, 1.0) * 255.0) as u8,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };

        unsafe {
            let screen = GetDC(None);
            let result = UpdateLayeredWindow(
                hwnd,
                Some(screen),
                Some(&POINT { x: at.x, y: at.y }),
                Some(&SIZE { cx: self.width, cy: self.height }),
                Some(self.dc),
                Some(&POINT { x: 0, y: 0 }),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );
            ReleaseDC(None, screen);

            if let Err(error) = result {
                log::warn!("could not show the bar: {error}");
                return false;
            }
        }
        true
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.previous);
            let _ = DeleteObject(self.dib.into());
            let _ = DeleteDC(self.dc);
        }
    }
}

struct Measured {
    layout: IDWriteTextLayout,
    width: f32,
}

/// A segment that has been measured, ready to be given a position.
struct Piece {
    icon: Option<Measured>,
    text: Measured,
    width: f32,
    emphasis: Emphasis,
    tone: D2D1_COLOR_F,
    action: Option<Act>,
}

impl Piece {
    fn filled(&self) -> bool {
        matches!(self.emphasis, Emphasis::Pill | Emphasis::Focused | Emphasis::Urgent)
    }
}

/// Where the clickable segments ended up, in window coordinates.
pub type HitBoxes = Vec<(Rect, Act)>;

/// The fonts one bar draws with.
pub struct Fonts<'a> {
    pub text: &'a IDWriteTextFormat,
    pub bold: &'a IDWriteTextFormat,
    pub icon: Option<&'a IDWriteTextFormat>,
}

/// How far in from the edge of the strip the groups sit, and how round they
/// are.
#[derive(Debug, Clone, Copy)]
pub struct Metrics {
    pub margin: f32,
    pub radius: f32,
    pub scale: f32,
    /// The least a group can be and still hold a line of text.
    pub line: f32,
}

/// Draw one bar and report back what the user can click on.
pub fn draw(
    painter: &Painter,
    surface: &Surface,
    fonts: &Fonts,
    palette: &Palette,
    sections: &Sections,
    metrics: Metrics,
) -> HitBoxes {
    let (width, height) = surface.size();
    let scale = metrics.scale;
    let target = &surface.target;

    // A margin that would squeeze the groups thinner than their own text is
    // trimmed rather than obeyed: a bar too short to read is worse than one
    // sitting closer to the edge than asked.
    let metrics =
        Metrics { margin: metrics.margin.min(((height - metrics.line) / 2.0).max(0.0)), ..metrics };
    let top = metrics.margin;
    let inner = (height - 2.0 * metrics.margin).max(1.0);
    let padding = GROUP_PADDING * scale;
    let group_gap = GROUP_GAP * scale;

    let measure = |segment: &Segment, limit: f32| {
        measure_segment(painter, fonts, palette, segment, limit, inner, scale)
    };

    let left = prepare(&sections.left, &measure, width, scale);
    let right = prepare(&sections.right, &measure, width, scale);
    let left_group = group_width(&left, padding, scale);
    let right_group = group_width(&right, padding, scale);

    // The centre gets whatever the two ends left behind.
    let taken = left_group + right_group + 2.0 * metrics.margin + 2.0 * group_gap;
    let room = (width - taken - 2.0 * padding).max(0.0);
    let centre = prepare(&sections.center, &measure, room, scale);
    let centre_group = group_width(&centre, padding, scale);

    let right_start = width - metrics.margin - right_group;
    let earliest = metrics.margin + left_group + group_gap;
    let latest = (right_start - group_gap - centre_group).max(earliest);
    let centre_start = ((width - centre_group) / 2.0).clamp(earliest, latest);

    unsafe {
        target.BeginDraw();
        target.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));
    }

    let mut hits = Vec::new();
    for (pieces, start) in [(left, metrics.margin), (centre, centre_start), (right, right_start)] {
        paint_group(target, palette, &pieces, start, top, inner, metrics, &mut hits);
    }

    unsafe {
        let _ = target.EndDraw(None, None);
    }

    hits
}

/// Draw the rounded island and everything inside it.
#[allow(clippy::too_many_arguments)]
fn paint_group(
    target: &ID2D1RenderTarget,
    palette: &Palette,
    pieces: &[Piece],
    start: f32,
    top: f32,
    inner: f32,
    metrics: Metrics,
    hits: &mut HitBoxes,
) {
    if pieces.is_empty() {
        return;
    }

    let scale = metrics.scale;
    let padding = GROUP_PADDING * scale;
    let gap = SEGMENT_GAP * scale;
    let width = group_width(pieces, padding, scale);

    fill_round(
        target,
        D2D_RECT_F { left: start, top, right: start + width, bottom: top + inner },
        metrics.radius,
        palette.background,
    );

    let mut x = start + padding;
    for piece in pieces {
        let pill_padding = if piece.filled() { PILL_PADDING * scale } else { 0.0 };
        let box_width = piece.width + 2.0 * pill_padding;

        if piece.filled() {
            let fill = match piece.emphasis {
                Emphasis::Focused => palette.accent,
                Emphasis::Urgent => palette.urgent,
                _ => fade(palette.surface, 0.9),
            };
            // The inset gives way on a short bar so the pill keeps its text.
            let inset = (PILL_INSET * scale).min(inner * 0.18);
            let pill = D2D_RECT_F {
                left: x,
                top: top + inset,
                right: x + box_width,
                bottom: top + inner - inset,
            };
            fill_round(target, pill, (inner - 2.0 * inset) / 2.0, fill);
        }

        let text_colour = match piece.emphasis {
            Emphasis::Focused | Emphasis::Urgent => palette.accent_text,
            Emphasis::Muted => palette.muted,
            _ => palette.foreground,
        };
        // Inside a filled pill the accent would fight the fill, so the icon
        // joins the text instead.
        let icon_colour = if piece.filled() { text_colour } else { piece.tone };

        let mut cursor = x + pill_padding;
        if let Some(icon) = &piece.icon {
            draw_text(target, &icon.layout, cursor, top, icon_colour);
            cursor += icon.width + ICON_GAP * scale;
        }
        draw_text(target, &piece.text.layout, cursor, top, text_colour);

        if let Some(action) = piece.action {
            hits.push((
                Rect::from_edges(
                    x as i32,
                    top as i32,
                    (x + box_width) as i32,
                    (top + inner) as i32,
                ),
                action,
            ));
        }

        x += box_width + gap;
    }
}

fn fill_round(target: &ID2D1RenderTarget, rect: D2D_RECT_F, radius: f32, colour: D2D1_COLOR_F) {
    let Ok(brush) = (unsafe { target.CreateSolidColorBrush(&colour, None) }) else {
        return;
    };
    let limit = ((rect.bottom - rect.top) / 2.0).min((rect.right - rect.left) / 2.0);
    let rounded =
        D2D1_ROUNDED_RECT { rect, radiusX: radius.min(limit), radiusY: radius.min(limit) };
    unsafe { target.FillRoundedRectangle(&rounded, &brush) };
}

fn draw_text(
    target: &ID2D1RenderTarget,
    layout: &IDWriteTextLayout,
    x: f32,
    y: f32,
    colour: D2D1_COLOR_F,
) {
    let brush: ID2D1SolidColorBrush = match unsafe { target.CreateSolidColorBrush(&colour, None) } {
        Ok(brush) => brush,
        Err(_) => return,
    };
    unsafe {
        target.DrawTextLayout(Vector2 { X: x, Y: y }, layout, &brush, D2D1_DRAW_TEXT_OPTIONS_CLIP);
    }
}

fn measure_segment(
    painter: &Painter,
    fonts: &Fonts,
    palette: &Palette,
    segment: &Segment,
    limit: f32,
    height: f32,
    scale: f32,
) -> Option<Piece> {
    let format = if segment.emphasis == Emphasis::Focused { fonts.bold } else { fonts.text };

    let icon = segment
        .icon
        .as_ref()
        .zip(fonts.icon)
        .and_then(|(glyph, font)| painter.layout(glyph, font, limit, height));

    let used = icon.as_ref().map(|icon| icon.width + ICON_GAP * scale).unwrap_or(0.0);
    let text = painter.layout(&segment.text, format, (limit - used).max(0.0), height)?;

    Some(Piece {
        width: used + text.width,
        icon,
        text,
        emphasis: segment.emphasis,
        tone: palette.tone(segment.tone),
        action: segment.action,
    })
}

/// Measure a group, giving the flexible segments whatever is left over.
fn prepare(
    segments: &[Segment],
    measure: &dyn Fn(&Segment, f32) -> Option<Piece>,
    limit: f32,
    scale: f32,
) -> Vec<Piece> {
    let pill = 2.0 * PILL_PADDING * scale;
    let fixed: f32 = segments
        .iter()
        .filter(|segment| !segment.flexible)
        .filter_map(|segment| measure(segment, limit))
        .map(|piece| piece.width + if piece.filled() { pill } else { 0.0 })
        .sum();

    let spare = (limit - fixed).max(0.0);

    segments
        .iter()
        .filter_map(|segment| {
            let room = if segment.flexible { spare } else { limit };
            let piece = measure(segment, room)?;
            (piece.width > 0.5).then_some(piece)
        })
        .collect()
}

/// Total width of a group, padding and gaps included.
fn group_width(pieces: &[Piece], padding: f32, scale: f32) -> f32 {
    if pieces.is_empty() {
        return 0.0;
    }
    let gap = SEGMENT_GAP * scale;
    let pill = 2.0 * PILL_PADDING * scale;

    let content: f32 =
        pieces.iter().map(|piece| piece.width + if piece.filled() { pill } else { 0.0 }).sum();

    content + 2.0 * padding + (pieces.len() as f32 - 1.0) * gap
}

/// Icons are given their own size so they sit with the text rather than
/// towering over it.
pub fn icon_size(font_size: f32) -> f32 {
    font_size * ICON_SCALE
}
