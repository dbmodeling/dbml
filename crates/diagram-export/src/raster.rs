use resvg::{tiny_skia, usvg};
use std::fmt;

/// Failure while rasterising an SVG.
#[derive(Debug)]
pub enum RasterError {
    /// The SVG could not be parsed.
    Svg(String),
    /// The image is empty or too large to allocate.
    Size,
    /// Encoding the raster failed.
    Encode(String),
}

impl fmt::Display for RasterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Svg(e) => write!(f, "invalid SVG: {e}"),
            Self::Size => f.write_str("diagram size is invalid or too large"),
            Self::Encode(e) => write!(f, "image encoding failed: {e}"),
        }
    }
}

impl std::error::Error for RasterError {}

/// Largest side accepted, in pixels, to keep memory bounded.
const MAX_SIDE: u32 = 16_384;

fn render(svg: &str, fonts: &[&[u8]], scale: f32) -> Result<tiny_skia::Pixmap, RasterError> {
    let mut opt = usvg::Options::default();
    {
        let db = opt.fontdb_mut();
        for f in fonts {
            db.load_font_data((*f).to_vec());
        }
        db.set_sans_serif_family("Inter");
    }
    let tree = usvg::Tree::from_str(svg, &opt).map_err(|e| RasterError::Svg(e.to_string()))?;
    let size = tree.size();
    let (w, h) = (
        (size.width() * scale).ceil() as u32,
        (size.height() * scale).ceil() as u32,
    );
    if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE {
        return Err(RasterError::Size);
    }
    let mut pixmap = tiny_skia::Pixmap::new(w, h).ok_or(RasterError::Size)?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Ok(pixmap)
}

/// Rasterises `svg` to PNG at `scale`× (2 = retina). `fonts` are TTF/OTF bytes (family `Inter`).
///
/// # Errors
/// Returns [`RasterError`] when the SVG is invalid, empty or too large, or encoding fails.
pub fn svg_to_png(svg: &str, fonts: &[&[u8]], scale: f32) -> Result<Vec<u8>, RasterError> {
    render(svg, fonts, scale)?
        .encode_png()
        .map_err(|e| RasterError::Encode(e.to_string()))
}

/// Rasterises `svg` to JPEG (quality 1–100). The SVG background is opaque, so no alpha is lost.
///
/// # Errors
/// Returns [`RasterError`] when the SVG is invalid, empty or too large, or encoding fails.
pub fn svg_to_jpeg(
    svg: &str,
    fonts: &[&[u8]],
    scale: f32,
    quality: u8,
) -> Result<Vec<u8>, RasterError> {
    let pixmap = render(svg, fonts, scale)?;
    let rgb: Vec<u8> = pixmap
        .data()
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect();
    let mut out = Vec::new();
    let (w, h) = (
        u16::try_from(pixmap.width()).map_err(|_| RasterError::Size)?,
        u16::try_from(pixmap.height()).map_err(|_| RasterError::Size)?,
    );
    jpeg_encoder::Encoder::new(&mut out, quality)
        .encode(&rgb, w, h, jpeg_encoder::ColorType::Rgb)
        .map_err(|e| RasterError::Encode(e.to_string()))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Theme, to_svg};
    use std::collections::HashMap;

    const INTER: &[u8] = include_bytes!("../../app/assets/fonts/Inter-Regular.ttf");

    fn sample() -> (schema::Schema, HashMap<String, (f32, f32)>) {
        let col = |n: &str, pk| schema::Column {
            name: n.into(),
            ty: "int".into(),
            pk,
            not_null: pk,
            ..Default::default()
        };
        let t = |n: &str| schema::Table {
            name: n.into(),
            columns: vec![col("id", true), col("v", false)],
            ..Default::default()
        };
        let s = schema::Schema {
            tables: vec![t("a"), t("b")],
            ..Default::default()
        };
        let p = HashMap::from([("a".into(), (0.0, 0.0)), ("b".into(), (400.0, 40.0))]);
        (s, p)
    }

    #[test]
    fn svg_should_contain_cards_and_escape_names() {
        let (mut s, p) = sample();
        s.tables[0].name = "a<b".into();
        let svg = to_svg(
            &s,
            &HashMap::from([("a<b".into(), (0.0, 0.0))]),
            &Theme::dark(),
        );
        assert!(svg.contains("a&lt;b") && !svg.contains("a<b"));
        assert!(to_svg(&sample().0, &p, &Theme::light()).contains("viewBox"));
    }

    #[test]
    fn png_should_have_signature_and_diagram_size() {
        let (s, p) = sample();
        let svg = to_svg(&s, &p, &Theme::dark());
        let png = svg_to_png(&svg, &[INTER], 2.0).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        let img = resvg::tiny_skia::Pixmap::decode_png(&png).unwrap();
        // 2 cartões: 260+400+260 = 660 + 96 de margem = ~756 → ×2
        assert!(
            img.width() > 1400 && img.height() > 200,
            "{}x{}",
            img.width(),
            img.height()
        );
        // o fundo é opaco e não é transparente
        assert_eq!(
            img.pixel(0, 0)
                .map(resvg::tiny_skia::PremultipliedColorU8::alpha),
            Some(255)
        );
    }

    #[test]
    fn jpeg_should_have_magic_bytes() {
        let (s, p) = sample();
        let jpg = svg_to_jpeg(&to_svg(&s, &p, &Theme::light()), &[INTER], 1.0, 90).unwrap();
        assert_eq!(&jpg[..3], &[0xFF, 0xD8, 0xFF]);
    }

    #[test]
    fn raster_should_reject_invalid_svg() {
        assert!(matches!(
            svg_to_png("not svg", &[], 1.0),
            Err(RasterError::Svg(_))
        ));
    }
}
