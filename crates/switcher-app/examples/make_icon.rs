//! Regenerates `assets/icons/lang-switcher.ico` from the badge rasterizer.
//!
//! The application icon is the product's own badge so that the tray, the Start menu entry
//! and Add/Remove Programs all look like the same thing. The glyph is `A`, the convention
//! Windows itself uses for an input indicator, on the badge palette — not `RU` or `EN`,
//! because the icon must not claim one particular language.
//!
//! Run after changing the badge metrics or palette:
//!
//! ```text
//! cargo run -p switcher-app --example make_icon
//! ```
//!
//! The result is committed; the build never runs this.

use std::path::PathBuf;

use switcher_app::render::{BadgeCache, BadgeMetrics, FONT};
use switcher_core::content::{BADGE_FG, BadgeContent, BadgeStyle, Rgb8};

/// Sizes Windows asks for: shell lists, Alt+Tab, the Start menu and Add/Remove Programs.
/// `BadgeCache::tray_rgba` accepts 8..=128, so 256 is deliberately not produced; Windows
/// scales the 128 entry for the "extra large icons" view.
const SIZES: [u32; 7] = [16, 20, 24, 32, 48, 64, 128];

/// The badge blue, so the icon reads as this application rather than as a generic square.
const ICON_BG: Rgb8 = Rgb8 {
    r: 0x3D,
    g: 0x6F,
    b: 0xD9,
};

/// One 32bpp BGRA image in the DIB form an `.ico` entry uses: a `BITMAPINFOHEADER` whose
/// height covers the colour rows *and* the AND mask, bottom-up colour rows, then the mask.
///
/// The mask is all zeros. For a 32bpp entry Windows composites through the alpha channel
/// and ignores it, but the format still requires it to be present and correctly sized.
fn dib(rgba_straight: &[u8], size: u32) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&40u32.to_le_bytes()); // biSize
    out.extend_from_slice(&(size as i32).to_le_bytes()); // biWidth
    out.extend_from_slice(&((size * 2) as i32).to_le_bytes()); // biHeight: colour + mask
    out.extend_from_slice(&1u16.to_le_bytes()); // biPlanes
    out.extend_from_slice(&32u16.to_le_bytes()); // biBitCount
    out.extend_from_slice(&0u32.to_le_bytes()); // biCompression = BI_RGB
    out.extend_from_slice(&0u32.to_le_bytes()); // biSizeImage
    out.extend_from_slice(&0i32.to_le_bytes()); // biXPelsPerMeter
    out.extend_from_slice(&0i32.to_le_bytes()); // biYPelsPerMeter
    out.extend_from_slice(&0u32.to_le_bytes()); // biClrUsed
    out.extend_from_slice(&0u32.to_le_bytes()); // biClrImportant

    for y in (0..size).rev() {
        for x in 0..size {
            let i = ((y * size + x) * 4) as usize;
            let [r, g, b, a] = [
                rgba_straight[i],
                rgba_straight[i + 1],
                rgba_straight[i + 2],
                rgba_straight[i + 3],
            ];
            out.extend_from_slice(&[b, g, r, a]);
        }
    }
    // 1 bit per pixel, rows padded to a 4-byte boundary.
    let mask_stride = size.div_ceil(32) * 4;
    out.resize(out.len() + (mask_stride * size) as usize, 0);
    out
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let content = BadgeContent {
        label: "A".to_owned(),
        bg: ICON_BG,
        fg: BADGE_FG,
        style: BadgeStyle::Text,
    };
    let cache = BadgeCache::new(FONT, BadgeMetrics::default())?;
    let images: Vec<(u32, Vec<u8>)> = SIZES
        .iter()
        .map(|&size| Ok((size, dib(&cache.tray_rgba(&content, size)?, size))))
        .collect::<Result<_, Box<dyn std::error::Error>>>()?;

    let mut ico = Vec::new();
    ico.extend_from_slice(&0u16.to_le_bytes()); // reserved
    ico.extend_from_slice(&1u16.to_le_bytes()); // type: icon
    ico.extend_from_slice(&(images.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * images.len() as u32;
    for (size, data) in &images {
        // A 0 in the size byte means 256; every size here is below that, so the cast is
        // exact for all of them.
        ico.push(u8::try_from(*size).expect("icon sizes are below 256"));
        ico.push(u8::try_from(*size).expect("icon sizes are below 256"));
        ico.extend_from_slice(&[0, 0]); // colour count, reserved
        ico.extend_from_slice(&1u16.to_le_bytes()); // planes
        ico.extend_from_slice(&32u16.to_le_bytes()); // bit count
        ico.extend_from_slice(&(data.len() as u32).to_le_bytes());
        ico.extend_from_slice(&offset.to_le_bytes());
        offset += data.len() as u32;
    }
    for (_, data) in &images {
        ico.extend_from_slice(data);
    }

    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/icons/lang-switcher.ico");
    std::fs::create_dir_all(path.parent().expect("the asset path has a directory"))?;
    std::fs::write(&path, &ico)?;
    println!(
        "wrote {} ({} bytes, {} sizes)",
        path.display(),
        ico.len(),
        images.len()
    );
    Ok(())
}
