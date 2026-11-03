//! Generates the application icons from `assets/icon/ircnext.svg`:
//! PNG sizes, the macOS iconset (turned into `.icns` by `iconutil`), the
//! Windows `.ico` and the raw RGBA window icon.
//!
//! `cargo run --manifest-path tools/icons/Cargo.toml` from the repository root.

use std::fs;
use std::path::Path;

use resvg::{tiny_skia, usvg};

fn render(tree: &usvg::Tree, size: u32) -> tiny_skia::Pixmap {
    let mut pixmap = tiny_skia::Pixmap::new(size, size).expect("valid size");
    let scale = size as f32 / tree.size().width();
    resvg::render(tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    pixmap
}

/// Straight (not premultiplied) RGBA bytes.
fn rgba(pixmap: &tiny_skia::Pixmap) -> Vec<u8> {
    pixmap
        .pixels()
        .iter()
        .flat_map(|pixel| {
            let color = pixel.demultiply();
            [color.red(), color.green(), color.blue(), color.alpha()]
        })
        .collect()
}

fn main() {
    let dir = Path::new("assets/icon");
    let svg = fs::read(dir.join("ircnext.svg")).expect("assets/icon/ircnext.svg, run from the repository root");
    let tree = usvg::Tree::from_data(&svg, &usvg::Options::default()).expect("valid SVG");

    fs::create_dir_all(dir.join("png")).unwrap();
    for size in [16, 32, 48, 64, 128, 256, 512, 1024] {
        render(&tree, size).save_png(dir.join(format!("png/ircnext-{size}.png"))).unwrap();
    }

    // macOS iconset: `iconutil -c icns assets/icon/ircnext.iconset`.
    let iconset = dir.join("ircnext.iconset");
    fs::create_dir_all(&iconset).unwrap();
    for (name, size) in [
        ("16x16", 16),
        ("16x16@2x", 32),
        ("32x32", 32),
        ("32x32@2x", 64),
        ("128x128", 128),
        ("128x128@2x", 256),
        ("256x256", 256),
        ("256x256@2x", 512),
        ("512x512", 512),
        ("512x512@2x", 1024),
    ] {
        render(&tree, size).save_png(iconset.join(format!("icon_{name}.png"))).unwrap();
    }

    // Windows icon.
    let mut icon = ico::IconDir::new(ico::ResourceType::Icon);
    for size in [16, 24, 32, 48, 64, 128, 256] {
        let pixmap = render(&tree, size);
        let image = ico::IconImage::from_rgba_data(size, size, rgba(&pixmap));
        icon.add_entry(ico::IconDirEntry::encode(&image).unwrap());
    }
    icon.write(fs::File::create(dir.join("ircnext.ico")).unwrap()).unwrap();

    // Window icon, as raw RGBA so that the application needs no decoder.
    fs::write(dir.join("ircnext-128.rgba"), rgba(&render(&tree, 128))).unwrap();
    println!("icons written to {}", dir.display());
}
