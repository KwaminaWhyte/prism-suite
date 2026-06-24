//! Real document export to **EPS**, **PDF**, **PNG**, and **SVG**.
//!
//! SVG and PNG already have full renderers in [`crate::export`]; this module adds
//! the two PostScript-family writers Illustrator also offers and re-exports the
//! raster/vector pair behind one uniform entry point ([`export_document`]):
//!
//! - **EPS** — Encapsulated PostScript: a `%!PS-Adobe` header + a `BoundingBox`,
//!   then each shape flattened to PostScript path operators (`moveto` / `lineto`
//!   / `closepath`) painted with `fill` / `stroke`.
//! - **PDF** — a minimal single-page PDF 1.4: the standard object table
//!   (catalog / pages / page / content stream) with the artwork as a content
//!   stream of PDF path operators (`m` / `l` / `h` / `f` / `S` / `re`). The page
//!   uses PDF's bottom-up y-axis, so document y is flipped.
//!
//! Both vector writers flatten beziers to polylines (via the document's own
//! flatten) so the output is exact and self-contained. Pure + unit-tested.

use crate::document::{flatten, Document, Shape};

/// The export file formats this module can write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Svg,
    Png,
    Eps,
    Pdf,
}

impl ExportFormat {
    /// Lower-case file extension (no dot).
    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::Svg => "svg",
            ExportFormat::Png => "png",
            ExportFormat::Eps => "eps",
            ExportFormat::Pdf => "pdf",
        }
    }

    /// Guess a format from a file path's extension (case-insensitive). Defaults to
    /// SVG for an unknown / missing extension.
    pub fn from_path(path: &str) -> ExportFormat {
        let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        match ext.as_str() {
            "png" => ExportFormat::Png,
            "eps" | "ps" => ExportFormat::Eps,
            "pdf" => ExportFormat::Pdf,
            _ => ExportFormat::Svg,
        }
    }
}

/// The encoded bytes of an export: text for the vector formats (already UTF-8) or
/// binary for PNG.
#[derive(Clone, Debug)]
pub enum ExportBytes {
    Text(String),
    Binary(Vec<u8>),
}

impl ExportBytes {
    /// The raw bytes regardless of variant.
    pub fn into_bytes(self) -> Vec<u8> {
        match self {
            ExportBytes::Text(s) => s.into_bytes(),
            ExportBytes::Binary(b) => b,
        }
    }
    pub fn as_bytes(&self) -> Vec<u8> {
        match self {
            ExportBytes::Text(s) => s.clone().into_bytes(),
            ExportBytes::Binary(b) => b.clone(),
        }
    }
}

/// Export the document cropped to artboard `ab` `[x, y, w, h]` in `format`.
/// Returns `None` only when a raster encode fails (degenerate artboard size).
pub fn export_document(doc: &Document, ab: [f32; 4], format: ExportFormat) -> Option<ExportBytes> {
    match format {
        ExportFormat::Svg => Some(ExportBytes::Text(crate::export::to_svg_artboard(doc, ab))),
        ExportFormat::Png => crate::export::to_png_artboard(doc, ab).map(ExportBytes::Binary),
        ExportFormat::Eps => Some(ExportBytes::Text(to_eps(doc, ab))),
        ExportFormat::Pdf => Some(ExportBytes::Binary(to_pdf(doc, ab).into_bytes())),
    }
}

// --- Flattened contour extraction --------------------------------------------

/// One closed/open sub-contour of a shape, flattened to a polyline in document
/// space.
struct Contour {
    pts: Vec<(f32, f32)>,
    closed: bool,
}

/// One paintable shape reduced to its contours plus paint attributes.
struct FlatShape {
    contours: Vec<Contour>,
    fill: Option<[f32; 4]>,
    stroke: [f32; 4],
    stroke_w: f32,
}

/// Flatten a shape into document-space contours + paint, or `None` for shapes
/// with no drawable geometry. Beziers are flattened via the document's own
/// flatten so curves are honoured. Colours are straight sRGB RGBA.
fn flatten_shape(shape: &Shape) -> Option<FlatShape> {
    if !shape.visible() {
        return None;
    }
    let stroke = shape.stroke_color().unwrap_or([0.0, 0.0, 0.0, 0.0]);
    let stroke_w = shape.stroke_width();
    let fill = shape.fill_color();
    let contours: Vec<Contour> = match shape {
        Shape::Rect { rect, .. } => {
            let (x, y, w, h) = (rect[0], rect[1], rect[2], rect[3]);
            vec![Contour {
                pts: vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h)],
                closed: true,
            }]
        }
        Shape::Ellipse { rect, .. } => {
            let cx = rect[0] + rect[2] * 0.5;
            let cy = rect[1] + rect[3] * 0.5;
            let rx = rect[2] * 0.5;
            let ry = rect[3] * 0.5;
            let pts = (0..64)
                .map(|i| {
                    let t = i as f32 / 64.0 * std::f32::consts::TAU;
                    (cx + rx * t.cos(), cy + ry * t.sin())
                })
                .collect();
            vec![Contour { pts, closed: true }]
        }
        Shape::Line { p0, p1, .. } => vec![Contour {
            pts: vec![*p0, *p1],
            closed: false,
        }],
        Shape::Path {
            points,
            handles,
            closed,
            ..
        } => {
            if points.len() < 2 {
                return None;
            }
            vec![Contour {
                pts: flatten(points, handles, *closed),
                closed: *closed,
            }]
        }
        Shape::Compound { subpaths, .. } => subpaths
            .iter()
            .filter(|sp| sp.points.len() >= 2)
            .map(|sp| Contour {
                pts: sp.flatten(),
                closed: sp.closed,
            })
            .collect(),
        Shape::Text { glyphs, .. } => glyphs
            .iter()
            .filter(|sp| sp.points.len() >= 2)
            .map(|sp| Contour {
                pts: sp.flatten(),
                closed: sp.closed,
            })
            .collect(),
    };
    if contours.is_empty() {
        return None;
    }
    // A fill only paints when it has non-zero alpha.
    let fill = fill.filter(|c| c[3] > 0.0);
    Some(FlatShape {
        contours,
        fill,
        stroke,
        stroke_w,
    })
}

// --- EPS ---------------------------------------------------------------------

/// Serialize the document to an **EPS** (Encapsulated PostScript) string cropped
/// to artboard `ab` `[x, y, w, h]`. The bounding box is `0 0 w h`; artwork is
/// translated by `(-ox, -oy)`. PostScript's y-axis is bottom-up, so document y is
/// flipped within the page height. The output always begins with `%!PS-Adobe`.
pub fn to_eps(doc: &Document, ab: [f32; 4]) -> String {
    let (ox, oy, w, h) = (ab[0], ab[1], ab[2], ab[3]);
    let mut s = String::new();
    s.push_str("%!PS-Adobe-3.0 EPSF-3.0\n");
    s.push_str("%%Creator: Contour\n");
    s.push_str(&format!("%%BoundingBox: 0 0 {} {}\n", w.ceil() as i64, h.ceil() as i64));
    s.push_str("%%LanguageLevel: 2\n");
    s.push_str("%%EndComments\n");
    // Map document space (top-left origin, +y down) → EPS page (bottom-left, +y
    // up): x' = x - ox, y' = h - (y - oy).
    let map = |p: (f32, f32)| (p.0 - ox, h - (p.1 - oy));

    for shape in &doc.shapes {
        let Some(fs) = flatten_shape(shape) else { continue };
        // Emit the path once; fill then stroke (so a stroke sits over its fill).
        let emit_path = |out: &mut String| {
            for c in &fs.contours {
                if c.pts.is_empty() {
                    continue;
                }
                let (x0, y0) = map(c.pts[0]);
                out.push_str(&format!("{:.3} {:.3} moveto\n", x0, y0));
                for &p in &c.pts[1..] {
                    let (x, y) = map(p);
                    out.push_str(&format!("{:.3} {:.3} lineto\n", x, y));
                }
                if c.closed {
                    out.push_str("closepath\n");
                }
            }
        };
        if let Some(fill) = fs.fill {
            s.push_str("newpath\n");
            emit_path(&mut s);
            s.push_str(&format!("{:.4} {:.4} {:.4} setrgbcolor\n", fill[0], fill[1], fill[2]));
            s.push_str("fill\n");
        }
        if fs.stroke[3] > 0.0 && fs.stroke_w > 0.0 {
            s.push_str("newpath\n");
            emit_path(&mut s);
            s.push_str(&format!("{:.3} setlinewidth\n", fs.stroke_w));
            s.push_str(&format!(
                "{:.4} {:.4} {:.4} setrgbcolor\n",
                fs.stroke[0], fs.stroke[1], fs.stroke[2]
            ));
            s.push_str("stroke\n");
        }
    }
    s.push_str("showpage\n");
    s.push_str("%%EOF\n");
    s
}

// --- PDF ---------------------------------------------------------------------

/// A growable PDF byte buffer that tracks each object's byte offset for the
/// cross-reference table.
struct PdfWriter {
    buf: Vec<u8>,
    offsets: Vec<usize>,
}

impl PdfWriter {
    fn new() -> Self {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"%PDF-1.4\n");
        // Binary comment marker so tools treat the file as binary.
        buf.extend_from_slice(b"%\xE2\xE3\xCF\xD3\n");
        Self {
            buf,
            offsets: Vec::new(),
        }
    }

    /// Begin object `n` (1-based), recording its byte offset, and write its
    /// `n 0 obj` header.
    fn begin_obj(&mut self, n: usize) {
        // Grow the offset table to hold object n (index n-1).
        while self.offsets.len() < n {
            self.offsets.push(0);
        }
        self.offsets[n - 1] = self.buf.len();
        self.buf.extend_from_slice(format!("{} 0 obj\n", n).as_bytes());
    }

    fn write(&mut self, s: &str) {
        self.buf.extend_from_slice(s.as_bytes());
    }

    fn end_obj(&mut self) {
        self.buf.extend_from_slice(b"endobj\n");
    }

    /// Finish the file: emit the xref table + trailer pointing at `root` (catalog)
    /// object number. Returns the full byte buffer.
    fn finish(mut self, root: usize) -> Vec<u8> {
        let xref_pos = self.buf.len();
        let count = self.offsets.len() + 1; // +1 for the free object 0
        self.buf
            .extend_from_slice(format!("xref\n0 {}\n", count).as_bytes());
        self.buf.extend_from_slice(b"0000000000 65535 f \n");
        for &off in &self.offsets {
            self.buf
                .extend_from_slice(format!("{:010} 00000 n \n", off).as_bytes());
        }
        self.buf.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root {} 0 R >>\nstartxref\n{}\n%%EOF\n",
                count, root, xref_pos
            )
            .as_bytes(),
        );
        self.buf
    }
}

/// Serialize the document to a minimal single-page **PDF 1.4** cropped to
/// artboard `ab` `[x, y, w, h]`. The page MediaBox is `[0 0 w h]`; artwork uses
/// PDF's bottom-up y-axis (document y flipped). Object layout: 1 catalog,
/// 2 pages, 3 page, 4 content stream. The output always begins with `%PDF-`.
pub fn to_pdf(doc: &Document, ab: [f32; 4]) -> ExportBytes {
    let (ox, oy, w, h) = (ab[0], ab[1], ab[2], ab[3]);
    let content = pdf_content_stream(doc, ox, oy, h);
    let mut w_pdf = PdfWriter::new();

    // 1: Catalog.
    w_pdf.begin_obj(1);
    w_pdf.write("<< /Type /Catalog /Pages 2 0 R >>\n");
    w_pdf.end_obj();

    // 2: Pages.
    w_pdf.begin_obj(2);
    w_pdf.write("<< /Type /Pages /Kids [3 0 R] /Count 1 >>\n");
    w_pdf.end_obj();

    // 3: Page.
    w_pdf.begin_obj(3);
    w_pdf.write(&format!(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {:.2} {:.2}] /Contents 4 0 R /Resources << >> >>\n",
        w.max(1.0),
        h.max(1.0)
    ));
    w_pdf.end_obj();

    // 4: Content stream.
    w_pdf.begin_obj(4);
    w_pdf.write(&format!("<< /Length {} >>\nstream\n", content.len()));
    w_pdf.write(&content);
    w_pdf.write("\nendstream\n");
    w_pdf.end_obj();

    ExportBytes::Binary(w_pdf.finish(1))
}

/// Build the PDF page content stream: every shape's contours as PDF path
/// operators with fill / stroke. `(ox, oy)` is the artboard origin to subtract;
/// `h` is the page height for the y-flip.
fn pdf_content_stream(doc: &Document, ox: f32, oy: f32, h: f32) -> String {
    let mut s = String::new();
    let map = |p: (f32, f32)| (p.0 - ox, h - (p.1 - oy));
    for shape in &doc.shapes {
        let Some(fs) = flatten_shape(shape) else { continue };
        let has_fill = fs.fill.is_some();
        let has_stroke = fs.stroke[3] > 0.0 && fs.stroke_w > 0.0;
        if !has_fill && !has_stroke {
            continue;
        }
        s.push_str("q\n");
        if let Some(fill) = fs.fill {
            s.push_str(&format!("{:.4} {:.4} {:.4} rg\n", fill[0], fill[1], fill[2]));
        }
        if has_stroke {
            s.push_str(&format!(
                "{:.4} {:.4} {:.4} RG\n",
                fs.stroke[0], fs.stroke[1], fs.stroke[2]
            ));
            s.push_str(&format!("{:.3} w\n", fs.stroke_w));
        }
        for c in &fs.contours {
            if c.pts.is_empty() {
                continue;
            }
            let (x0, y0) = map(c.pts[0]);
            s.push_str(&format!("{:.3} {:.3} m\n", x0, y0));
            for &p in &c.pts[1..] {
                let (x, y) = map(p);
                s.push_str(&format!("{:.3} {:.3} l\n", x, y));
            }
            if c.closed {
                s.push_str("h\n");
            }
        }
        // Paint operator: fill+stroke (B), fill (f), or stroke (S).
        match (has_fill, has_stroke) {
            (true, true) => s.push_str("B\n"),
            (true, false) => s.push_str("f\n"),
            (false, true) => s.push_str("S\n"),
            (false, false) => {}
        }
        s.push_str("Q\n");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Document, Shape};

    fn doc_with_square() -> Document {
        let mut d = Document::new();
        d.shapes.clear();
        d.shapes.push(Shape::rect(
            [10.0, 10.0, 80.0, 60.0],
            [0.8, 0.2, 0.2, 1.0],
            [0.0, 0.0, 0.0, 1.0],
            2.0,
        ));
        d
    }

    #[test]
    fn eps_starts_with_ps_header() {
        let d = doc_with_square();
        let eps = to_eps(&d, [0.0, 0.0, 100.0, 80.0]);
        assert!(eps.starts_with("%!PS"), "EPS begins with PostScript magic");
        assert!(eps.contains("%%BoundingBox: 0 0 100 80"));
        assert!(eps.contains("moveto") && eps.contains("lineto") && eps.contains("closepath"));
        assert!(eps.contains("setrgbcolor") && eps.contains("fill"));
        assert!(eps.contains("stroke"), "stroked square emits a stroke");
        assert!(eps.trim_end().ends_with("%%EOF"));
    }

    #[test]
    fn eps_flips_y_within_page() {
        let d = doc_with_square();
        // Square top edge is at doc y=10; page height 80 → EPS y = 70.
        let eps = to_eps(&d, [0.0, 0.0, 100.0, 80.0]);
        assert!(eps.contains("70.000"), "top edge y-flipped to 70: {eps}");
    }

    #[test]
    fn pdf_starts_with_pdf_magic_and_has_xref() {
        let d = doc_with_square();
        let bytes = to_pdf(&d, [0.0, 0.0, 100.0, 80.0]).into_bytes();
        let head = &bytes[..5];
        assert_eq!(head, b"%PDF-", "PDF begins with %PDF-");
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("/Type /Catalog"));
        assert!(text.contains("/Type /Page "));
        assert!(text.contains("/MediaBox [0 0 100.00 80.00]"));
        assert!(text.contains("xref") && text.contains("trailer"));
        assert!(text.contains("startxref"));
        assert!(text.trim_end().ends_with("%%EOF"));
        // Content stream has fill+stroke ops.
        assert!(text.contains(" rg") && text.contains(" RG") && text.contains("\nB\n"));
    }

    #[test]
    fn pdf_xref_offsets_are_monotonic() {
        let d = doc_with_square();
        let bytes = to_pdf(&d, [0.0, 0.0, 100.0, 80.0]).into_bytes();
        let text = String::from_utf8_lossy(&bytes);
        // The four object headers must appear in order.
        let i1 = text.find("1 0 obj").unwrap();
        let i2 = text.find("2 0 obj").unwrap();
        let i3 = text.find("3 0 obj").unwrap();
        let i4 = text.find("4 0 obj").unwrap();
        assert!(i1 < i2 && i2 < i3 && i3 < i4);
    }

    #[test]
    fn export_format_from_path() {
        assert_eq!(ExportFormat::from_path("a/b.PDF"), ExportFormat::Pdf);
        assert_eq!(ExportFormat::from_path("x.eps"), ExportFormat::Eps);
        assert_eq!(ExportFormat::from_path("x.png"), ExportFormat::Png);
        assert_eq!(ExportFormat::from_path("x.svg"), ExportFormat::Svg);
        assert_eq!(ExportFormat::from_path("noext"), ExportFormat::Svg);
        assert_eq!(ExportFormat::Pdf.extension(), "pdf");
    }

    #[test]
    fn export_document_routes_each_format() {
        let d = doc_with_square();
        let ab = [0.0, 0.0, 100.0, 80.0];
        let svg = export_document(&d, ab, ExportFormat::Svg).unwrap();
        assert!(matches!(svg, ExportBytes::Text(ref s) if s.contains("<svg")));
        let eps = export_document(&d, ab, ExportFormat::Eps).unwrap();
        assert!(matches!(eps, ExportBytes::Text(ref s) if s.starts_with("%!PS")));
        let pdf = export_document(&d, ab, ExportFormat::Pdf).unwrap();
        assert!(matches!(pdf, ExportBytes::Binary(ref b) if &b[..5] == b"%PDF-"));
        // PNG may be Some on a tiny-skia build; just assert it doesn't panic.
        let _ = export_document(&d, ab, ExportFormat::Png);
    }

    #[test]
    fn invisible_shapes_are_skipped() {
        let mut d = doc_with_square();
        d.shapes[0].toggle_visible(); // hide it
        let eps = to_eps(&d, [0.0, 0.0, 100.0, 80.0]);
        // No path ops for a hidden shape.
        assert!(!eps.contains("moveto"), "hidden shape emits no geometry");
    }
}
