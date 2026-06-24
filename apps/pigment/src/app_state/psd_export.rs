//! Hand-rolled Photoshop `.psd` binary serializer (8BPS, version 1, RGB/8-bit).
//!
//! Unlike the GPU-host serializer in `canvas_host.rs` (which reads pixels off the
//! wgpu device), this module is a **pure, allocation-only writer**: feed it layer
//! descriptors carrying straight-sRGB RGBA8 buffers plus a merged composite, and
//! it returns a `Vec<u8>` that is a valid PSD file. That makes it fully unit
//! testable in-memory — no GPU adapter required.
//!
//! Format reference: Adobe Photoshop File Format Specification.
//! Sections written, in order:
//!   §2.1 File Header (26 bytes, signature `8BPS`)
//!   §2.2 Color Mode Data (empty for RGB)
//!   §2.3 Image Resources (resolution resource `1005` when requested)
//!   §2.4 Layer & Mask Information (per-layer record + channel image data)
//!   §2.5 Image Data (merged composite, planar)
//!
//! All multi-byte integers are big-endian. Channel data can be written either as
//! uncompressed raw (compression mode 0) or PackBits RLE (compression mode 1).

use super::{App, Action};

/// Channel compression chosen for the written file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PsdCompression {
    /// Uncompressed channel scanlines (compression id 0).
    Raw,
    /// PackBits run-length encoding (compression id 1).
    Rle,
}

/// One layer to serialize. Pixels are straight (non-premultiplied) sRGB RGBA8,
/// row-major, `width * height * 4` bytes, sized to the document.
#[derive(Clone, Debug)]
pub struct PsdLayerInput {
    pub name: String,
    /// Straight sRGB RGBA8, `doc_w * doc_h * 4` bytes.
    pub rgba8: Vec<u8>,
    /// Layer bounds within the document `[top, left, bottom, right]`.
    pub bounds: [u32; 4],
    /// Photoshop 4-char blend key (e.g. `*b"norm"`, `*b"mul "`).
    pub blend_key: [u8; 4],
    /// 0..=255 layer opacity.
    pub opacity: u8,
    /// Whether the layer is visible (clears the hidden flag bit when true).
    pub visible: bool,
}

/// Everything needed to serialize a full PSD document.
#[derive(Clone, Debug)]
pub struct PsdDocument {
    pub width: u32,
    pub height: u32,
    pub layers: Vec<PsdLayerInput>,
    /// Straight sRGB RGBA8 merged composite, `width * height * 4` bytes.
    pub merged_rgba8: Vec<u8>,
    pub compression: PsdCompression,
    /// Document resolution in DPI, written as resource `1005` when `> 0`.
    pub dpi: f32,
}

/// Map an internal blend-shader id (as produced by `BlendMode::shader_id`) to a
/// Photoshop 4-char blend key.
pub fn blend_key_for_shader_id(blend_id: u32) -> [u8; 4] {
    match blend_id {
        1 => *b"diss",  // Dissolve
        2 => *b"dark",  // Darken
        3 => *b"mul ",  // Multiply
        4 => *b"idiv",  // Color Burn
        5 => *b"lbrn",  // Linear Burn
        6 => *b"lite",  // Lighten
        7 => *b"scrn",  // Screen
        8 => *b"div ",  // Color Dodge
        9 => *b"lddg",  // Linear Dodge (Add)
        10 => *b"over", // Overlay
        11 => *b"sLit", // Soft Light
        12 => *b"hLit", // Hard Light
        13 => *b"vLit", // Vivid Light
        14 => *b"lLit", // Linear Light
        15 => *b"pLit", // Pin Light
        16 => *b"diff", // Difference
        17 => *b"smud", // Exclusion
        18 => *b"hue ", // Hue
        19 => *b"sat ", // Saturation
        20 => *b"colr", // Color
        21 => *b"lum ", // Luminosity
        _ => *b"norm",  // Normal
    }
}

/// PackBits-compress one scanline (Photoshop's per-row RLE). Returns the
/// compressed bytes for the single row. Worst case is slightly larger than the
/// input, which PackBits guarantees stays bounded.
pub fn packbits_row(row: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(row.len() + row.len() / 128 + 1);
    let n = row.len();
    let mut i = 0;
    while i < n {
        // Look for a run of >= 2 identical bytes.
        let mut run = 1;
        while i + run < n && row[i + run] == row[i] && run < 128 {
            run += 1;
        }
        if run >= 2 {
            // Encode a replicate run: header (1-run as i8) then the byte.
            out.push((1i32 - run as i32) as u8); // = 257 - run, i.e. -(run-1)
            out.push(row[i]);
            i += run;
        } else {
            // Gather a literal run of non-repeating bytes (up to 128).
            let start = i;
            let mut lit = 0;
            while i < n && lit < 128 {
                // Stop the literal run if the next two bytes start a replicate.
                if i + 1 < n && row[i] == row[i + 1] {
                    break;
                }
                i += 1;
                lit += 1;
            }
            out.push((lit - 1) as u8); // header: literal count - 1
            out.extend_from_slice(&row[start..start + lit]);
        }
    }
    out
}

/// Decode a single PackBits-compressed buffer back into `expected` bytes. Used
/// by the round-trip unit tests; tolerant of trailing padding.
pub fn packbits_decode(data: &[u8], expected: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(expected);
    let mut i = 0;
    while i < data.len() && out.len() < expected {
        let hdr = data[i] as i8;
        i += 1;
        if hdr >= 0 {
            let count = hdr as usize + 1;
            for _ in 0..count {
                if i < data.len() {
                    out.push(data[i]);
                    i += 1;
                }
            }
        } else if hdr != -128 {
            let count = (1 - hdr as i32) as usize;
            if i < data.len() {
                let b = data[i];
                i += 1;
                for _ in 0..count {
                    out.push(b);
                }
            }
        }
        // hdr == -128 is a no-op per the PackBits spec.
    }
    out.truncate(expected);
    out
}

/// Serialize a PSD document into a freshly allocated byte buffer. Never performs
/// I/O — the caller writes the returned bytes to disk.
pub fn serialize_psd(doc: &PsdDocument) -> Vec<u8> {
    let (w, h) = (doc.width.max(1), doc.height.max(1));
    let npx = (w * h) as usize;
    let nlayers = doc.layers.len();
    let rle = doc.compression == PsdCompression::Rle;

    let mut buf: Vec<u8> = Vec::with_capacity(64 + nlayers * (256 + npx * 4) + npx * 4);

    macro_rules! be2 { ($v:expr) => { buf.extend_from_slice(&($v as u16).to_be_bytes()) } }
    macro_rules! be4 { ($v:expr) => { buf.extend_from_slice(&($v as u32).to_be_bytes()) } }

    // ---- §2.1 File Header Section (26 bytes) --------------------------------
    buf.extend_from_slice(b"8BPS"); // Signature
    be2!(1u16); // Version 1
    buf.extend_from_slice(&[0u8; 6]); // Reserved (must be zero)
    be2!(4u16); // Channel count (RGBA)
    be4!(h); // Rows (height)
    be4!(w); // Columns (width)
    be2!(8u16); // Bit depth per channel
    be2!(3u16); // Color mode: 3 = RGB

    // ---- §2.2 Color Mode Data Section (empty for RGB) -----------------------
    be4!(0u32);

    // ---- §2.3 Image Resources Section ---------------------------------------
    let resources = build_image_resources(doc.dpi);
    be4!(resources.len() as u32);
    buf.extend_from_slice(&resources);

    // ---- §2.4 Layer and Mask Information Section ----------------------------
    let layer_info = build_layer_info(doc, w, h, rle);
    // Length of the full Layer & Mask section.
    be4!(layer_info.len() as u32);
    buf.extend_from_slice(&layer_info);

    // ---- §2.5 Image Data Section (merged composite, planar) -----------------
    if rle {
        be2!(1u16); // Compression mode 1 = RLE
        // PSD RLE: all row byte-counts (per channel, per row) come first, then
        // the compressed scanlines.
        let mut counts: Vec<u8> = Vec::new();
        let mut bodies: Vec<u8> = Vec::new();
        for ch in 0..4usize {
            for row in 0..h as usize {
                let mut scan = Vec::with_capacity(w as usize);
                for col in 0..w as usize {
                    scan.push(doc.merged_rgba8[(row * w as usize + col) * 4 + ch]);
                }
                let packed = packbits_row(&scan);
                counts.extend_from_slice(&(packed.len() as u16).to_be_bytes());
                bodies.extend_from_slice(&packed);
            }
        }
        buf.extend_from_slice(&counts);
        buf.extend_from_slice(&bodies);
    } else {
        be2!(0u16); // Compression mode 0 = raw
        for ch in 0..4usize {
            for px in doc.merged_rgba8.chunks_exact(4) {
                buf.push(px[ch]);
            }
        }
    }

    buf
}

/// Build the Image Resources section payload (without its outer length prefix).
/// Currently emits resource id 1005 (Resolution Info) when `dpi > 0`.
fn build_image_resources(dpi: f32) -> Vec<u8> {
    if dpi <= 0.0 {
        return Vec::new();
    }
    let mut r: Vec<u8> = Vec::new();
    r.extend_from_slice(b"8BIM");
    r.extend_from_slice(&1005u16.to_be_bytes()); // Resource ID: ResolutionInfo
    // Pascal-string name: empty → length 0 + pad to even (total 2 bytes).
    r.push(0u8);
    r.push(0u8);
    // Resolution data block is 16 bytes; length must be even (it is).
    r.extend_from_slice(&16u32.to_be_bytes());
    // hRes: fixed-point 16.16 dpi.
    let fixed = ((dpi * 65536.0).round() as u32).to_be_bytes();
    r.extend_from_slice(&fixed); // hRes
    r.extend_from_slice(&1u16.to_be_bytes()); // hResUnit: 1 = px/inch
    r.extend_from_slice(&1u16.to_be_bytes()); // widthUnit: 1 = inches
    r.extend_from_slice(&fixed); // vRes
    r.extend_from_slice(&1u16.to_be_bytes()); // vResUnit
    r.extend_from_slice(&1u16.to_be_bytes()); // heightUnit
    r
}

/// Build the Layer & Mask Information section payload (without its outer length
/// prefix). Contains the layer-info subsection (records + channel data) and a
/// zero-length global layer-mask subsection.
fn build_layer_info(doc: &PsdDocument, w: u32, h: u32, rle: bool) -> Vec<u8> {
    let npx = (w * h) as usize;
    let nlayers = doc.layers.len();

    // ----- Layer records + channel image data (the "layer info" block) -------
    let mut records: Vec<u8> = Vec::new();
    macro_rules! r2 { ($v:expr) => { records.extend_from_slice(&($v as u16).to_be_bytes()) } }
    macro_rules! r4 { ($v:expr) => { records.extend_from_slice(&($v as u32).to_be_bytes()) } }

    // §13.1 Layer count (signed i16; positive here — alpha is a real channel).
    r2!(nlayers as i16);

    // Pre-compute each layer's channel byte payloads so the record's channel
    // length fields are exact.
    let mut layer_channels: Vec<[Vec<u8>; 4]> = Vec::with_capacity(nlayers);
    for layer in &doc.layers {
        let mut chans: [Vec<u8>; 4] = Default::default();
        for (ci, slot) in chans.iter_mut().enumerate() {
            let mut plane = Vec::with_capacity(npx);
            for px in layer.rgba8.chunks_exact(4) {
                plane.push(px[ci]);
            }
            *slot = encode_channel(&plane, w, h, rle);
        }
        layer_channels.push(chans);
    }

    // §13.2 One layer record per layer.
    for (li, layer) in doc.layers.iter().enumerate() {
        let [top, left, bottom, right] = layer.bounds;
        r4!(top);
        r4!(left);
        r4!(bottom);
        r4!(right);

        // Channel count + per-channel (id, length).
        r2!(4u16);
        let ids = [0i16, 1, 2, -1]; // R, G, B, A
        for (ci, id) in ids.iter().enumerate() {
            records.extend_from_slice(&id.to_be_bytes());
            r4!(layer_channels[li][ci].len() as u32);
        }

        // Blend mode signature + key.
        records.extend_from_slice(b"8BIM");
        records.extend_from_slice(&layer.blend_key);
        records.push(layer.opacity);
        records.push(0u8); // clipping: 0 = base
        records.push(if layer.visible { 0u8 } else { 2u8 }); // flags: bit1 = hidden
        records.push(0u8); // filler

        // Extra data: mask (0) + blending ranges (0) + Pascal name (padded to 4).
        let name_bytes = layer.name.as_bytes();
        let name_len = name_bytes.len().min(255);
        let written = 1 + name_len;
        let pad = ((written + 3) & !3) - written;
        let extra_len = 4 + 4 + (written + pad) as u32;
        r4!(extra_len);
        r4!(0u32); // layer mask data length
        r4!(0u32); // layer blending ranges length
        records.push(name_len as u8);
        records.extend_from_slice(&name_bytes[..name_len]);
        for _ in 0..pad {
            records.push(0u8);
        }
    }

    // §13.3 Channel image data: all 4 channels per layer, in layer order.
    for chans in &layer_channels {
        for plane in chans.iter() {
            records.extend_from_slice(plane);
        }
    }

    // The "layer info" length prefix must itself be padded to an even boundary.
    if records.len() % 2 == 1 {
        records.push(0u8);
    }

    // ----- Assemble: layer-info length + layer-info, then global mask (0) ----
    let mut out: Vec<u8> = Vec::with_capacity(records.len() + 8);
    out.extend_from_slice(&(records.len() as u32).to_be_bytes());
    out.extend_from_slice(&records);
    out.extend_from_slice(&0u32.to_be_bytes()); // global layer mask info length = 0
    out
}

/// Encode a single channel plane (`w*h` bytes) with a 2-byte compression header,
/// matching the per-layer channel-data layout (`§13.3`).
fn encode_channel(plane: &[u8], w: u32, h: u32, rle: bool) -> Vec<u8> {
    if rle {
        let mut counts: Vec<u8> = Vec::with_capacity(h as usize * 2);
        let mut bodies: Vec<u8> = Vec::new();
        for row in 0..h as usize {
            let start = row * w as usize;
            let scan = &plane[start..start + w as usize];
            let packed = packbits_row(scan);
            counts.extend_from_slice(&(packed.len() as u16).to_be_bytes());
            bodies.extend_from_slice(&packed);
        }
        let mut out = Vec::with_capacity(2 + counts.len() + bodies.len());
        out.extend_from_slice(&1u16.to_be_bytes()); // compression: RLE
        out.extend_from_slice(&counts);
        out.extend_from_slice(&bodies);
        out
    } else {
        let mut out = Vec::with_capacity(2 + plane.len());
        out.extend_from_slice(&0u16.to_be_bytes()); // compression: raw
        out.extend_from_slice(plane);
        out
    }
}

// ---- Header reader (for round-trip testing) ---------------------------------

/// The fields parsed out of a PSD file header by [`read_psd_header`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PsdHeader {
    pub version: u16,
    pub channels: u16,
    pub height: u32,
    pub width: u32,
    pub depth: u16,
    pub color_mode: u16,
}

/// Parse and validate a PSD file header from raw bytes. Returns `None` if the
/// `8BPS` signature is missing or the buffer is too short.
pub fn read_psd_header(bytes: &[u8]) -> Option<PsdHeader> {
    if bytes.len() < 26 || &bytes[0..4] != b"8BPS" {
        return None;
    }
    let u16be = |o: usize| u16::from_be_bytes([bytes[o], bytes[o + 1]]);
    let u32be = |o: usize| u32::from_be_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]);
    Some(PsdHeader {
        version: u16be(4),
        channels: u16be(12),
        height: u32be(14),
        width: u32be(18),
        depth: u16be(22),
        color_mode: u16be(24),
    })
}

impl App {
    /// Dispatch for the PSD export domain actions.
    pub(super) fn apply_psd_export(&mut self, action: Action) {
        match action {
            Action::SetPsdExportCompression(rle) => {
                self.psd_export_rle = rle;
            }
            Action::ExportPsdNative(path) => {
                self.do_export_psd_native(&path);
            }
            _ => {}
        }
    }

    /// Build a [`PsdDocument`] from the live GPU host + document and write it to
    /// `path` using the native serializer. Reads each layer's pixels and the
    /// composite off the host, converting to straight sRGB RGBA8.
    pub fn do_export_psd_native(&mut self, path: &std::path::Path) {
        let (w, h) = (self.host.doc_w, self.host.doc_h);
        let npx = (w * h) as usize;

        let to_rgba8 = |f: &[f32]| -> Vec<u8> {
            let mut out = Vec::with_capacity(npx * 4);
            for px in f.chunks_exact(4) {
                let a = px[3];
                let inv = if a > 1e-5 { 1.0 / a } else { 0.0 };
                let to8 = |v: f32| {
                    (prism_core::color::linear_to_srgb((v * inv).clamp(0.0, 1.0)) * 255.0).round()
                        as u8
                };
                out.push(to8(px[0]));
                out.push(to8(px[1]));
                out.push(to8(px[2]));
                out.push((a.clamp(0.0, 1.0) * 255.0).round() as u8);
            }
            out
        };

        let mut layers: Vec<PsdLayerInput> = Vec::new();
        // Snapshot ids/metadata first to avoid borrowing `self.doc` across the
        // mutable `self.host` reads.
        let meta: Vec<(prism_core::LayerId, String, [u8; 4], u8, bool)> = self
            .doc
            .layers
            .layers
            .iter()
            .filter(|l| !matches!(l.kind, prism_core::LayerKind::Adjustment(_)))
            .map(|l| {
                (
                    l.id,
                    l.name.clone(),
                    blend_key_for_shader_id(l.blend.shader_id()),
                    (l.opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
                    l.visible,
                )
            })
            .collect();
        for (id, name, blend_key, opacity, visible) in meta {
            let rgba8 = match self.host.read_layer_f32(id) {
                Some(f) => to_rgba8(&f),
                None => vec![0u8; npx * 4],
            };
            layers.push(PsdLayerInput {
                name,
                rgba8,
                bounds: [0, 0, h, w],
                blend_key,
                opacity,
                visible,
            });
        }

        let merged_rgba8 = match self.host.read_composite_f32() {
            Some(f) => to_rgba8(&f),
            None => vec![0u8; npx * 4],
        };

        let doc = PsdDocument {
            width: w,
            height: h,
            layers,
            merged_rgba8,
            compression: if self.psd_export_rle {
                PsdCompression::Rle
            } else {
                PsdCompression::Raw
            },
            dpi: 72.0,
        };
        let bytes = serialize_psd(&doc);
        match std::fs::write(path, &bytes) {
            Ok(()) => {
                self.last_psd_native_path = Some(path.display().to_string());
                self.status_message = Some(format!("Saved PSD (native): {}", path.display()));
            }
            Err(e) => {
                self.status_message = Some(format!("Native PSD export failed: {e}"));
                log::error!("native PSD export failed: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid_layer(name: &str, w: u32, h: u32, rgba: [u8; 4]) -> PsdLayerInput {
        let mut px = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..(w * h) {
            px.extend_from_slice(&rgba);
        }
        PsdLayerInput {
            name: name.to_string(),
            rgba8: px,
            bounds: [0, 0, h, w],
            blend_key: *b"norm",
            opacity: 255,
            visible: true,
        }
    }

    fn sample_doc(compression: PsdCompression) -> PsdDocument {
        let (w, h) = (4u32, 3u32);
        let merged: Vec<u8> = (0..(w * h))
            .flat_map(|i| [(i * 7) as u8, (i * 3) as u8, (i * 5) as u8, 255u8])
            .collect();
        PsdDocument {
            width: w,
            height: h,
            layers: vec![
                solid_layer("Background", w, h, [10, 20, 30, 255]),
                solid_layer("Top", w, h, [200, 100, 50, 128]),
            ],
            merged_rgba8: merged,
            compression,
            dpi: 72.0,
        }
    }

    #[test]
    fn psd_signature_and_header_roundtrip() {
        let doc = sample_doc(PsdCompression::Raw);
        let bytes = serialize_psd(&doc);
        // 8BPS signature.
        assert_eq!(&bytes[0..4], b"8BPS");
        let hdr = read_psd_header(&bytes).expect("valid header");
        assert_eq!(hdr.version, 1);
        assert_eq!(hdr.channels, 4);
        assert_eq!(hdr.width, 4);
        assert_eq!(hdr.height, 3);
        assert_eq!(hdr.depth, 8);
        assert_eq!(hdr.color_mode, 3); // RGB
    }

    #[test]
    fn psd_rle_header_roundtrip() {
        let doc = sample_doc(PsdCompression::Rle);
        let bytes = serialize_psd(&doc);
        assert_eq!(&bytes[0..4], b"8BPS");
        let hdr = read_psd_header(&bytes).unwrap();
        assert_eq!(hdr.width, 4);
        assert_eq!(hdr.height, 3);
        assert_eq!(hdr.channels, 4);
    }

    #[test]
    fn read_header_rejects_bad_signature() {
        let mut bytes = serialize_psd(&sample_doc(PsdCompression::Raw));
        bytes[0] = b'X';
        assert!(read_psd_header(&bytes).is_none());
        assert!(read_psd_header(&[0u8; 4]).is_none());
    }

    #[test]
    fn packbits_roundtrips_arbitrary_row() {
        let row: Vec<u8> = vec![
            5, 5, 5, 5, 1, 2, 3, 4, 9, 9, 9, 0, 0, 0, 0, 0, 7, 8, 8, 8, 8,
        ];
        let packed = packbits_row(&row);
        // RLE should not blow up the size unreasonably.
        assert!(packed.len() <= row.len() + row.len() / 128 + 2);
        let decoded = packbits_decode(&packed, row.len());
        assert_eq!(decoded, row);
    }

    #[test]
    fn packbits_roundtrips_uniform_run() {
        let row = vec![42u8; 300];
        let packed = packbits_row(&row);
        assert!(packed.len() < row.len()); // big win for uniform data
        let decoded = packbits_decode(&packed, row.len());
        assert_eq!(decoded, row);
    }

    #[test]
    fn rle_channel_data_decodes_back() {
        // Build a channel plane and verify encode_channel → decode round-trips.
        let (w, h) = (8u32, 4u32);
        let plane: Vec<u8> = (0..(w * h))
            .map(|i| ((i / 3) % 200) as u8)
            .collect();
        let encoded = encode_channel(&plane, w, h, true);
        // First 2 bytes are the compression marker.
        assert_eq!(&encoded[0..2], &1u16.to_be_bytes());
        // Parse row counts then decode each scanline.
        let row_count = h as usize;
        let counts_end = 2 + row_count * 2;
        let mut decoded: Vec<u8> = Vec::new();
        let mut body = counts_end;
        for r in 0..row_count {
            let len = u16::from_be_bytes([encoded[2 + r * 2], encoded[2 + r * 2 + 1]]) as usize;
            let scan = packbits_decode(&encoded[body..body + len], w as usize);
            decoded.extend_from_slice(&scan);
            body += len;
        }
        assert_eq!(decoded, plane);
    }

    #[test]
    fn blend_key_mapping() {
        assert_eq!(blend_key_for_shader_id(3), *b"mul ");
        assert_eq!(blend_key_for_shader_id(0), *b"norm");
        assert_eq!(blend_key_for_shader_id(999), *b"norm");
    }

    #[test]
    fn layer_count_field_matches() {
        let doc = sample_doc(PsdCompression::Raw);
        let bytes = serialize_psd(&doc);
        // Walk to the layer & mask section to confirm the layer count i16.
        // Header(26) + colormode(4 + len) + resources(4 + len) + lm len(4) + li len(4).
        let cm_len = u32::from_be_bytes([bytes[26], bytes[27], bytes[28], bytes[29]]) as usize;
        let res_off = 30 + cm_len;
        let res_len = u32::from_be_bytes([
            bytes[res_off],
            bytes[res_off + 1],
            bytes[res_off + 2],
            bytes[res_off + 3],
        ]) as usize;
        let lm_off = res_off + 4 + res_len;
        // lm_off points at the 4-byte Layer&Mask length, then 4-byte layer-info
        // length, then the i16 layer count.
        let count_off = lm_off + 4 + 4;
        let count = i16::from_be_bytes([bytes[count_off], bytes[count_off + 1]]);
        assert_eq!(count, 2);
    }

    #[test]
    fn resource_resolution_present_when_dpi_set() {
        let res = build_image_resources(72.0);
        assert!(!res.is_empty());
        assert_eq!(&res[0..4], b"8BIM");
        assert_eq!(u16::from_be_bytes([res[4], res[5]]), 1005);
        // Empty when dpi is zero.
        assert!(build_image_resources(0.0).is_empty());
    }
}
