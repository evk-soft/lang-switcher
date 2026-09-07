//! Badge rasterization and bounded, content/DPI-keyed cache (ADR-0006).

use std::collections::HashMap;

use ab_glyph::{Font, FontRef, GlyphId, OutlinedGlyph, Rect, ScaleFont, point};
use switcher_core::content::{BadgeContent, BadgeStyle};
use switcher_platform::events::BadgeImage;
use tiny_skia::{FillRule, Mask, Paint, Path, PathBuilder, Pixmap, Transform};

pub const FONT: &[u8] = include_bytes!("../assets/fonts/Inter-SemiBold-subset.ttf");
pub const MAX_ENTRIES: usize = 16;
const MAX_EDGE: f32 = 4096.0;
const MAX_PIXELS: u64 = 4 * 1024 * 1024;

#[derive(Clone, Copy, Debug)]
pub struct BadgeMetrics {
    pub height_dip: f32,
    pub min_width_dip: f32,
    pub pad_x_dip: f32,
    pub radius_dip: f32,
    pub text_px_dip: f32,
}

impl Default for BadgeMetrics {
    fn default() -> Self {
        // Proposed design values; verify visually at 100/150/200% in task 21.
        Self {
            height_dip: 26.0,
            min_width_dip: 40.0,
            pad_x_dip: 9.0,
            radius_dip: 7.0,
            text_px_dip: 16.0,
        }
    }
}

impl BadgeMetrics {
    fn valid(self) -> bool {
        [self.height_dip, self.min_width_dip, self.text_px_dip]
            .into_iter()
            .all(|v| v.is_finite() && v > 0.0)
            && [self.pad_x_dip, self.radius_dip]
                .into_iter()
                .all(|v| v.is_finite() && v >= 0.0)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("invalid embedded font")]
    InvalidFont(#[from] ab_glyph::InvalidFont),
    #[error("badge metrics or DPI are invalid")]
    InvalidMetrics,
    #[error("badge exceeds the raster size limit")]
    TooLarge,
    #[error("could not allocate badge surface or mask")]
    Allocation,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct BadgeKey {
    content: BadgeContent,
    dpi: u32,
}

#[derive(Debug)]
pub struct BadgeCache {
    font: FontRef<'static>,
    metrics: BadgeMetrics,
    entries: HashMap<BadgeKey, BadgeImage>,
}

impl BadgeCache {
    pub fn new(font_bytes: &'static [u8], metrics: BadgeMetrics) -> Result<Self, RenderError> {
        if !metrics.valid() {
            return Err(RenderError::InvalidMetrics);
        }
        Ok(Self {
            font: FontRef::try_from_slice(font_bytes)?,
            metrics,
            entries: HashMap::new(),
        })
    }

    pub fn image(&mut self, content: &BadgeContent, dpi: u32) -> Result<&BadgeImage, RenderError> {
        let key = BadgeKey {
            content: content.clone(),
            dpi,
        };
        if !self.entries.contains_key(&key) {
            let image = render_badge(&self.font, self.metrics, content, dpi)?;
            if self.entries.len() >= MAX_ENTRIES {
                self.entries.clear();
            }
            self.entries.insert(key.clone(), image);
        }
        Ok(self.entries.get(&key).expect("successful raster is cached"))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

struct LabelInk {
    bounds: Rect,
    glyphs: Vec<OutlinedGlyph>,
}

fn measure_label(font: &FontRef<'_>, label: &str, px: f32) -> Option<LabelInk> {
    let scaled = font.as_scaled(px);
    let mut previous = None;
    let mut pen = 0.0;
    let mut bounds: Option<Rect> = None;
    let mut glyphs = Vec::new();
    for ch in label.chars() {
        let id = scaled.glyph_id(ch);
        if id == GlyphId(0) {
            return None;
        }
        if let Some(prev) = previous {
            pen += scaled.kern(prev, id);
        }
        let glyph = font.outline_glyph(id.with_scale_and_position(px, point(pen, 0.0)))?;
        let b = glyph.px_bounds();
        bounds = Some(match bounds {
            Some(old) => Rect {
                min: point(old.min.x.min(b.min.x), old.min.y.min(b.min.y)),
                max: point(old.max.x.max(b.max.x), old.max.y.max(b.max.y)),
            },
            None => b,
        });
        glyphs.push(glyph);
        pen += scaled.h_advance(id);
        previous = Some(id);
    }
    Some(LabelInk {
        bounds: bounds?,
        glyphs,
    })
}

pub fn render_badge(
    font: &FontRef<'_>,
    metrics: BadgeMetrics,
    content: &BadgeContent,
    dpi: u32,
) -> Result<BadgeImage, RenderError> {
    if dpi == 0 || !metrics.valid() {
        return Err(RenderError::InvalidMetrics);
    }
    let scale = dpi as f32 / 96.0;
    let height = (metrics.height_dip * scale).round();
    let text_px = (metrics.text_px_dip * scale).round();
    if height < 1.0 || text_px < 1.0 {
        return Err(RenderError::InvalidMetrics);
    }
    if height > MAX_EDGE || text_px > MAX_EDGE || content.label.chars().take(17).count() > 16 {
        return Err(RenderError::TooLarge);
    }
    let ink = if content.style == BadgeStyle::Text {
        measure_label(font, &content.label, text_px)
    } else {
        None
    };
    let width = match &ink {
        Some(ink) => (metrics.min_width_dip * scale)
            .max(ink.bounds.width() + 2.0 * metrics.pad_x_dip * scale)
            .round(),
        None => height,
    };
    if !width.is_finite() || width > MAX_EDGE {
        return Err(RenderError::TooLarge);
    }
    let (w, h) = (width as u32, height as u32);
    if u64::from(w) * u64::from(h) > MAX_PIXELS {
        return Err(RenderError::TooLarge);
    }
    let mut pixmap = Pixmap::new(w, h).ok_or(RenderError::Allocation)?;
    let path = rounded_rect(width, height, metrics.radius_dip * scale)
        .ok_or(RenderError::InvalidMetrics)?;
    let mut paint = Paint::default();
    paint.set_color_rgba8(content.bg.r, content.bg.g, content.bg.b, 255);
    pixmap.fill_path(
        &path,
        &paint,
        FillRule::Winding,
        Transform::identity(),
        None,
    );
    if let Some(ink) = ink {
        let mut mask = Mask::new(w, h).ok_or(RenderError::Allocation)?;
        // Integer translation preserves measured pixel bounds. Center actual ink,
        // rather than advance width, which includes invisible side bearings.
        let dx = ((width - ink.bounds.width()) / 2.0 - ink.bounds.min.x).round() as i64;
        let dy = ((height - ink.bounds.height()) / 2.0 - ink.bounds.min.y).round() as i64;
        for glyph in ink.glyphs {
            let origin = glyph.px_bounds().min;
            glyph.draw(|x, y, coverage| {
                let x = i64::from(x) + origin.x as i64 + dx;
                let y = i64::from(y) + origin.y as i64 + dy;
                if x >= 0 && y >= 0 && x < i64::from(w) && y < i64::from(h) {
                    let value = &mut mask.data_mut()[(y as u32 * w + x as u32) as usize];
                    *value = value.saturating_add((coverage * 255.0).round() as u8);
                }
            });
        }
        paint.set_color_rgba8(content.fg.r, content.fg.g, content.fg.b, 255);
        // Reuse the shape so even unusual text metrics cannot escape its corners.
        pixmap.fill_path(
            &path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            Some(&mask),
        );
    }
    let mut bgra_premul = pixmap.take();
    for px in bgra_premul.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
    Ok(BadgeImage {
        width: w,
        height: h,
        bgra_premul,
        dpi,
    })
}

fn rounded_rect(w: f32, h: f32, radius: f32) -> Option<Path> {
    let r = radius.min(w / 2.0).min(h / 2.0);
    let c = 0.5523 * r;
    let mut pb = PathBuilder::new();
    pb.move_to(r, 0.0);
    pb.line_to(w - r, 0.0);
    pb.cubic_to(w - r + c, 0.0, w, r - c, w, r);
    pb.line_to(w, h - r);
    pb.cubic_to(w, h - r + c, w - r + c, h, w - r, h);
    pb.line_to(r, h);
    pb.cubic_to(r - c, h, 0.0, h - r + c, 0.0, h - r);
    pb.line_to(0.0, r);
    pb.cubic_to(0.0, r - c, r - c, 0.0, r, 0.0);
    pb.close();
    pb.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use switcher_core::content::{BadgeStyle, Rgb8};

    fn content(style: BadgeStyle) -> BadgeContent {
        BadgeContent {
            label: "RU".into(),
            bg: Rgb8 {
                r: 214,
                g: 69,
                b: 30,
            },
            fg: Rgb8 {
                r: 255,
                g: 255,
                b: 255,
            },
            style,
        }
    }

    fn cache() -> BadgeCache {
        BadgeCache::new(FONT, BadgeMetrics::default()).unwrap()
    }

    fn pixel(image: &BadgeImage, x: u32, y: u32) -> &[u8] {
        let offset = ((y * image.width + x) * 4) as usize;
        &image.bgra_premul[offset..offset + 4]
    }

    #[test]
    fn size_scales_with_dpi_and_buffer_matches_dimensions() {
        let mut cache = cache();
        let c = content(BadgeStyle::Text);
        let low = cache.image(&c, 96).unwrap().clone();
        let high = cache.image(&c, 192).unwrap();
        assert_eq!(high.height, 2 * low.height);
        assert!(high.width >= 2 * low.width - 2);
        assert_eq!(high.dpi, 192);
        assert_eq!(
            high.bgra_premul.len(),
            (high.width * high.height * 4) as usize
        );
    }

    #[test]
    fn swatch_is_square_with_transparent_corners_and_bgra_background() {
        let mut cache = cache();
        let c = content(BadgeStyle::Color);
        let image = cache.image(&c, 96).unwrap();
        assert_eq!(image.width, image.height);
        assert_eq!(pixel(image, 0, 0), [0, 0, 0, 0]);
        assert_eq!(
            pixel(image, image.width / 2, image.height / 2),
            [c.bg.b, c.bg.g, c.bg.r, 255]
        );
    }

    #[test]
    fn text_is_visible_and_all_pixels_are_premultiplied() {
        let mut cache = cache();
        for dpi in [96, 144, 192, 288] {
            let image = cache.image(&content(BadgeStyle::Text), dpi).unwrap();
            assert_eq!(pixel(image, 0, 0)[3], 0);
            assert!(
                image
                    .bgra_premul
                    .chunks_exact(4)
                    .all(|p| p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3])
            );
            assert!(
                image
                    .bgra_premul
                    .chunks_exact(4)
                    .filter(|p| p[0] >= 200 && p[1] >= 200 && p[2] >= 200 && p[3] == 255)
                    .count()
                    >= 8
            );
        }
    }

    #[test]
    fn missing_or_empty_label_falls_back_to_a_visible_swatch() {
        let mut cache = cache();
        for label in ["ЖЖ", "RЖ", ""] {
            let mut c = content(BadgeStyle::Text);
            c.label = label.into();
            let image = cache.image(&c, 96).unwrap();
            assert_eq!(image.width, image.height);
            assert_eq!(
                pixel(image, image.width / 2, image.height / 2),
                [c.bg.b, c.bg.g, c.bg.r, 255]
            );
        }
    }

    #[test]
    fn ink_measurement_grows_with_text() {
        let font = FontRef::try_from_slice(FONT).unwrap();
        let r = measure_label(&font, "R", 16.0).unwrap();
        let ru = measure_label(&font, "RU", 16.0).unwrap();
        assert!(ru.bounds.width() > r.bounds.width());
        assert!(r.bounds.width() > 0.0);
    }

    #[test]
    fn cache_keys_include_content_and_dpi_and_eviction_is_bounded() {
        let mut cache = cache();
        let mut c = content(BadgeStyle::Text);
        cache.image(&c, 96).unwrap();
        cache.image(&c, 96).unwrap();
        assert_eq!(cache.len(), 1);
        c.label = "EN".into();
        cache.image(&c, 96).unwrap();
        assert_eq!(cache.len(), 2);
        for dpi in 96..(96 + MAX_ENTRIES as u32 + 1) {
            cache.image(&c, dpi).unwrap();
            assert!(cache.len() <= MAX_ENTRIES);
        }
        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn invalid_inputs_fail_before_allocating_and_do_not_enter_cache() {
        assert!(BadgeCache::new(b"bad font", BadgeMetrics::default()).is_err());
        let metrics = BadgeMetrics {
            height_dip: f32::NAN,
            ..BadgeMetrics::default()
        };
        assert!(BadgeCache::new(FONT, metrics).is_err());
        let mut cache = cache();
        let c = content(BadgeStyle::Text);
        assert!(cache.image(&c, 0).is_err());
        assert!(cache.image(&c, u32::MAX).is_err());
        assert!(cache.is_empty());
    }
}
