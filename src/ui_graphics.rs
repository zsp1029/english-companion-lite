//! Antialiased shapes via Windows' built-in GDI+; text stays native ClearType.
use std::ptr::null_mut;
use windows::Win32::{
    Foundation::{COLORREF, E_FAIL},
    Graphics::{Gdi::*, GdiPlus::*},
};

pub(crate) struct VectorRenderer(usize);
impl VectorRenderer {
    pub unsafe fn new() -> windows::core::Result<Self> {
        let mut token = 0;
        let input = GdiplusStartupInput {
            GdiplusVersion: 1,
            SuppressExternalCodecs: true.into(),
            ..Default::default()
        };
        if GdiplusStartup(&mut token, &input, null_mut()).0 != 0 {
            return Err(windows::core::Error::from_hresult(E_FAIL));
        }
        std::result::Result::Ok(Self(token))
    }
    pub unsafe fn rounded(
        &self,
        dc: HDC,
        rect: [f32; 4],
        diameter: f32,
        fill: Option<COLORREF>,
        stroke: Option<COLORREF>,
    ) {
        let Some(graphics) = Graphics::new(dc) else {
            return;
        };
        let mut path = null_mut();
        if GdipCreatePath(FillModeAlternate, &mut path).0 != 0 {
            return;
        }
        // Half-pixel inset keeps one physical pixel outlines crisp at any DPI.
        let [x, y, w, h] = rect;
        let (x, y, w, h) = (x + 0.5, y + 0.5, (w - 1.0).max(0.0), (h - 1.0).max(0.0));
        let d = diameter.min(w).min(h).max(1.0);
        for (ax, ay, start) in [
            (x, y, 180.0),
            (x + w - d, y, 270.0),
            (x + w - d, y + h - d, 0.0),
            (x, y + h - d, 90.0),
        ] {
            GdipAddPathArc(path, ax, ay, d, d, start, 90.0);
        }
        GdipClosePathFigure(path);
        if let Some(color) = fill {
            let mut brush = null_mut();
            if GdipCreateSolidFill(argb(color), &mut brush).0 == 0 {
                GdipFillPath(graphics.0, brush.cast(), path);
                GdipDeleteBrush(brush.cast());
            }
        }
        if let Some(color) = stroke {
            let mut pen = null_mut();
            if GdipCreatePen1(argb(color), 1.0, UnitPixel, &mut pen).0 == 0 {
                GdipDrawPath(graphics.0, pen, path);
                GdipDeletePen(pen);
            }
        }
        GdipDeletePath(path);
    }
    // Geometry follows assets/companion.svg, avoiding enlargement of a 32px HICON.
    pub unsafe fn brand_mark(&self, dc: HDC, origin: [f32; 2], size: f32, color: COLORREF) {
        let Some(graphics) = Graphics::new(dc) else {
            return;
        };
        let scale = size / 32.0;
        let x = |v: f32| origin[0] + v * scale;
        let y = |v: f32| origin[1] + v * scale;
        let mut path = null_mut();
        if GdipCreatePath(FillModeAlternate, &mut path).0 != 0 {
            return;
        }
        GdipAddPathArc(
            path,
            x(3.0),
            y(3.0),
            12.0 * scale,
            12.0 * scale,
            180.0,
            90.0,
        );
        GdipAddPathLine(path, x(9.0), y(3.0), x(23.0), y(3.0));
        GdipAddPathArc(
            path,
            x(17.0),
            y(3.0),
            12.0 * scale,
            12.0 * scale,
            270.0,
            90.0,
        );
        GdipAddPathLine(path, x(29.0), y(9.0), x(29.0), y(21.0));
        GdipAddPathArc(
            path,
            x(17.0),
            y(15.0),
            12.0 * scale,
            12.0 * scale,
            0.0,
            90.0,
        );
        GdipAddPathLine(path, x(23.0), y(27.0), x(15.0), y(27.0));
        GdipAddPathLine(path, x(15.0), y(27.0), x(8.0), y(31.0));
        GdipAddPathLine(path, x(8.0), y(31.0), x(8.0), y(27.0));
        GdipAddPathBezier(
            path,
            x(8.0),
            y(27.0),
            x(5.1),
            y(26.5),
            x(3.0),
            y(24.1),
            x(3.0),
            y(21.0),
        );
        GdipAddPathLine(path, x(3.0), y(21.0), x(3.0), y(9.0));
        GdipClosePathFigure(path);
        let mut brush = null_mut();
        if GdipCreateSolidFill(argb(color), &mut brush).0 == 0 {
            GdipFillPath(graphics.0, brush.cast(), path);
            GdipDeleteBrush(brush.cast());
        }
        GdipDeletePath(path);
        let mut pen = null_mut();
        if GdipCreatePen1(0xffffffff, 2.8 * scale, UnitPixel, &mut pen).0 == 0 {
            GdipSetPenStartCap(pen, LineCapRound);
            GdipSetPenEndCap(pen, LineCapRound);
            GdipSetPenLineJoin(pen, LineJoinRound);
            let points = [
                PointF {
                    X: x(21.0),
                    Y: y(10.0),
                },
                PointF {
                    X: x(12.0),
                    Y: y(10.0),
                },
                PointF {
                    X: x(12.0),
                    Y: y(21.0),
                },
                PointF {
                    X: x(21.0),
                    Y: y(21.0),
                },
            ];
            GdipDrawLines(graphics.0, pen, points.as_ptr(), 4);
            let middle = [
                PointF {
                    X: x(12.0),
                    Y: y(15.5),
                },
                PointF {
                    X: x(19.0),
                    Y: y(15.5),
                },
            ];
            GdipDrawLines(graphics.0, pen, middle.as_ptr(), 2);
            GdipDeletePen(pen);
        }
    }
    pub unsafe fn chevron(&self, dc: HDC, points: [PointF; 3], width: f32, color: COLORREF) {
        let Some(graphics) = Graphics::new(dc) else {
            return;
        };
        let mut pen = null_mut();
        if GdipCreatePen1(argb(color), width, UnitPixel, &mut pen).0 == 0 {
            GdipSetPenStartCap(pen, LineCapRound);
            GdipSetPenEndCap(pen, LineCapRound);
            GdipSetPenLineJoin(pen, LineJoinRound);
            GdipDrawLines(graphics.0, pen, points.as_ptr(), 3);
            GdipDeletePen(pen);
        }
    }
}
impl Drop for VectorRenderer {
    fn drop(&mut self) {
        unsafe {
            GdiplusShutdown(self.0);
        }
    }
}
fn argb(color: COLORREF) -> u32 {
    0xff000000 | ((color.0 & 0xff) << 16) | (color.0 & 0xff00) | ((color.0 >> 16) & 0xff)
}
struct Graphics(*mut GpGraphics);
impl Graphics {
    unsafe fn new(dc: HDC) -> Option<Self> {
        let mut graphics = null_mut();
        if GdipCreateFromHDC(dc, &mut graphics).0 != 0 {
            return None;
        }
        GdipSetSmoothingMode(graphics, SmoothingModeAntiAlias8x8);
        Some(Self(graphics))
    }
}
impl Drop for Graphics {
    fn drop(&mut self) {
        unsafe {
            GdipDeleteGraphics(self.0);
        }
    }
}

/// Compose a complete frame before touching the visible DC. The caller must
/// present before EndPaint; clipping remains owned by the destination DC.
pub(crate) struct PaintBuffer {
    target: HDC,
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    width: i32,
    height: i32,
}
impl PaintBuffer {
    pub unsafe fn new(target: HDC, width: i32, height: i32) -> Option<Self> {
        if width <= 0 || height <= 0 {
            return None;
        }
        let dc = CreateCompatibleDC(target);
        if dc.is_invalid() {
            return None;
        }
        let bitmap = CreateCompatibleBitmap(target, width, height);
        if bitmap.is_invalid() {
            let _ = DeleteDC(dc);
            return None;
        }
        let previous = SelectObject(dc, bitmap);
        Some(Self {
            target,
            dc,
            bitmap,
            previous,
            width,
            height,
        })
    }
    pub fn dc(&self) -> HDC {
        self.dc
    }
    pub unsafe fn present(self) {
        let _ = BitBlt(
            self.target,
            0,
            0,
            self.width,
            self.height,
            self.dc,
            0,
            0,
            SRCCOPY,
        );
    }
}
impl Drop for PaintBuffer {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.previous);
            let _ = DeleteObject(self.bitmap);
            let _ = DeleteDC(self.dc);
        }
    }
}
