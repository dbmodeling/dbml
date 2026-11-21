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

/// Largest side accepted, in pixels.
const MAX_SIDE: u32 = 16_384;

/// Pixel budget of one raster (64 Mpx = 256 MiB of RGBA). Larger diagrams are rendered at a
/// smaller scale instead of failing or allocating more than a GiB.
pub const MAX_PIXELS: u64 = 64_000_000;

/// A rasterised image together with the scale that was actually applied.
#[derive(Debug, Clone, PartialEq)]
pub struct Raster {
    /// Encoded image bytes (PNG or JPEG).
    pub data: Vec<u8>,
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// Scale requested by the caller.
    pub requested_scale: f32,
    /// Scale applied; lower than `requested_scale` when the pixel budget forced a reduction.
    pub effective_scale: f32,
}

impl Raster {
    /// Whether the scale had to be reduced to respect the pixel budget.
    #[must_use]
    pub fn was_downscaled(&self) -> bool {
        self.effective_scale < self.requested_scale
    }
}

/// Scale to use for a diagram of `w` x `h` logical pixels so that the raster respects the
/// side cap and `max_pixels`; `requested` is kept when it already fits.
fn fit_scale(w: f32, h: f32, requested: f32, max_pixels: u64) -> f32 {
    let mut scale = requested.min(MAX_SIDE as f32 / w).min(MAX_SIDE as f32 / h);
    let area = f64::from(w) * f64::from(h);
    if area * f64::from(scale) * f64::from(scale) > max_pixels as f64 {
        scale = ((max_pixels as f64 / area).sqrt() * 0.9999) as f32;
    }
    scale
}

fn render(
    svg: &str,
    fonts: &[&[u8]],
    scale: f32,
    max_pixels: u64,
) -> Result<(tiny_skia::Pixmap, f32), RasterError> {
    if !scale.is_finite() || scale <= 0.0 {
        return Err(RasterError::Size);
    }
    let mut opt = usvg::Options::default();
    {
        let db = opt.fontdb_mut();
        for f in fonts {
            db.load_font_data((*f).to_vec());
        }
        db.set_sans_serif_family("Geist");
        db.set_monospace_family("Geist Mono");
    }
    let tree = usvg::Tree::from_str(svg, &opt).map_err(|e| RasterError::Svg(e.to_string()))?;
    let size = tree.size();
    if !(size.width().is_finite() && size.height().is_finite())
        || size.width() <= 0.0
        || size.height() <= 0.0
    {
        return Err(RasterError::Size);
    }
    let mut effective = fit_scale(size.width(), size.height(), scale, max_pixels);
    let dims = |scale: f32| {
        (
            (size.width() * scale).ceil() as u32,
            (size.height() * scale).ceil() as u32,
        )
    };
    let (mut w, mut h) = dims(effective);
    // O arredondamento para cima pode estourar o orçamento por poucos pixels.
    for _ in 0..8 {
        if u64::from(w) * u64::from(h) <= max_pixels {
            break;
        }
        effective *= 0.999;
        (w, h) = dims(effective);
    }
    if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE || u64::from(w) * u64::from(h) > max_pixels
    {
        return Err(RasterError::Size);
    }
    let mut pixmap = tiny_skia::Pixmap::new(w, h).ok_or(RasterError::Size)?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(effective, effective),
        &mut pixmap.as_mut(),
    );
    Ok((pixmap, effective))
}

/// Rasterises `svg` to PNG at `scale`× (2 = retina). `fonts` are TTF/OTF bytes (family `Geist`).
///
/// Diagrams that would exceed [`MAX_PIXELS`] are rendered at a smaller scale; use
/// [`svg_to_png_fit`] to learn the scale that was applied.
///
/// # Errors
/// Returns [`RasterError`] when the SVG is invalid or empty, or encoding fails.
pub fn svg_to_png(svg: &str, fonts: &[&[u8]], scale: f32) -> Result<Vec<u8>, RasterError> {
    svg_to_png_fit(svg, fonts, scale).map(|r| r.data)
}

/// Like [`svg_to_png`], returning the image with its dimensions and effective scale.
///
/// # Errors
/// Returns [`RasterError`] when the SVG is invalid or empty, or encoding fails.
pub fn svg_to_png_fit(svg: &str, fonts: &[&[u8]], scale: f32) -> Result<Raster, RasterError> {
    png_with_budget(svg, fonts, scale, MAX_PIXELS)
}

fn png_with_budget(
    svg: &str,
    fonts: &[&[u8]],
    scale: f32,
    max_pixels: u64,
) -> Result<Raster, RasterError> {
    let (pixmap, effective_scale) = render(svg, fonts, scale, max_pixels)?;
    let data = pixmap
        .encode_png()
        .map_err(|e| RasterError::Encode(e.to_string()))?;
    Ok(Raster {
        data,
        width: pixmap.width(),
        height: pixmap.height(),
        requested_scale: scale,
        effective_scale,
    })
}

/// Rasterises `svg` to JPEG (quality 1–100). The SVG background is opaque, so no alpha is lost.
///
/// Diagrams that would exceed [`MAX_PIXELS`] are rendered at a smaller scale; use
/// [`svg_to_jpeg_fit`] to learn the scale that was applied.
///
/// # Errors
/// Returns [`RasterError`] when the SVG is invalid or empty, or encoding fails.
pub fn svg_to_jpeg(
    svg: &str,
    fonts: &[&[u8]],
    scale: f32,
    quality: u8,
) -> Result<Vec<u8>, RasterError> {
    svg_to_jpeg_fit(svg, fonts, scale, quality).map(|r| r.data)
}

/// Like [`svg_to_jpeg`], returning the image with its dimensions and effective scale.
///
/// # Errors
/// Returns [`RasterError`] when the SVG is invalid or empty, or encoding fails.
pub fn svg_to_jpeg_fit(
    svg: &str,
    fonts: &[&[u8]],
    scale: f32,
    quality: u8,
) -> Result<Raster, RasterError> {
    jpeg_with_budget(svg, fonts, scale, quality, MAX_PIXELS)
}

fn jpeg_with_budget(
    svg: &str,
    fonts: &[&[u8]],
    scale: f32,
    quality: u8,
    max_pixels: u64,
) -> Result<Raster, RasterError> {
    let (pixmap, effective_scale) = render(svg, fonts, scale, max_pixels)?;
    let (width, height) = (pixmap.width(), pixmap.height());
    let (w16, h16) = (
        u16::try_from(width).map_err(|_| RasterError::Size)?,
        u16::try_from(height).map_err(|_| RasterError::Size)?,
    );
    // RGBA -> RGB no próprio buffer: sem segunda cópia do raster inteiro.
    let mut rgb = pixmap.take();
    let pixels = rgb.len() / 4;
    for i in 0..pixels {
        rgb.copy_within(i * 4..i * 4 + 3, i * 3);
    }
    rgb.truncate(pixels * 3);
    let mut data = Vec::new();
    jpeg_encoder::Encoder::new(&mut data, quality)
        .encode(&rgb, w16, h16, jpeg_encoder::ColorType::Rgb)
        .map_err(|e| RasterError::Encode(e.to_string()))?;
    Ok(Raster {
        data,
        width,
        height,
        requested_scale: scale,
        effective_scale,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Theme, to_svg};
    use std::collections::HashMap;

    const INTER: &[u8] = include_bytes!("../../app/assets/fonts/Geist-Regular.ttf");

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
            &HashMap::new(),
            &Theme::dark(),
        );
        assert!(svg.contains("a&lt;b") && !svg.contains("a<b"));
        assert!(to_svg(&sample().0, &p, &HashMap::new(), &Theme::light()).contains("viewBox"));
    }

    #[test]
    fn png_should_have_signature_and_diagram_size() {
        let (s, p) = sample();
        let svg = to_svg(&s, &p, &HashMap::new(), &Theme::dark());
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
        let jpg = svg_to_jpeg(
            &to_svg(&s, &p, &HashMap::new(), &Theme::light()),
            &[INTER],
            1.0,
            90,
        )
        .unwrap();
        assert_eq!(&jpg[..3], &[0xFF, 0xD8, 0xFF]);
    }

    const BIG_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="4000" height="3000"><rect width="4000" height="3000" fill="#123456"/></svg>"##;

    #[test]
    fn png_reduces_scale_to_fit_the_pixel_budget() {
        // 4000x3000 a 2x = 48 Mpx; com orçamento de 1 Mpx a escala efetiva cai.
        let r = png_with_budget(BIG_SVG, &[], 2.0, 1_000_000).unwrap();
        assert!(r.was_downscaled(), "{r:?}");
        assert!(
            u64::from(r.width) * u64::from(r.height) <= 1_000_000,
            "{}x{}",
            r.width,
            r.height
        );
        assert!(r.effective_scale < 0.5, "{}", r.effective_scale);
        let img = resvg::tiny_skia::Pixmap::decode_png(&r.data).unwrap();
        assert_eq!((img.width(), img.height()), (r.width, r.height));
    }

    #[test]
    fn jpeg_reduces_scale_to_fit_the_pixel_budget() {
        let r = jpeg_with_budget(BIG_SVG, &[], 2.0, 80, 1_000_000).unwrap();
        assert!(r.was_downscaled());
        assert!(u64::from(r.width) * u64::from(r.height) <= 1_000_000);
        assert_eq!(&r.data[..3], &[0xFF, 0xD8, 0xFF]);
    }

    #[test]
    fn scale_is_kept_when_it_fits_the_budget() {
        let r = png_with_budget(BIG_SVG, &[], 1.0, MAX_PIXELS).unwrap();
        assert!(!r.was_downscaled());
        assert_eq!((r.width, r.height), (4000, 3000));
    }

    #[test]
    fn huge_diagram_is_downscaled_instead_of_allocating_over_the_budget() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="40000" height="40000"><rect width="1" height="1"/></svg>"#;
        let r = png_with_budget(svg, &[], 1.0, 4_000_000).unwrap();
        assert!(u64::from(r.width) * u64::from(r.height) <= 4_000_000);
        assert!(r.was_downscaled());
    }

    #[test]
    fn invalid_scale_is_rejected() {
        assert!(matches!(
            png_with_budget(BIG_SVG, &[], 0.0, MAX_PIXELS),
            Err(RasterError::Size)
        ));
        assert!(matches!(
            png_with_budget(BIG_SVG, &[], f32::NAN, MAX_PIXELS),
            Err(RasterError::Size)
        ));
    }

    #[test]
    fn raster_should_reject_invalid_svg() {
        assert!(matches!(
            svg_to_png("not svg", &[], 1.0),
            Err(RasterError::Svg(_))
        ));
    }
}
