//! Renders the application icon from the brand SVG and embeds it in the
//! executable, with a PNG of it that the app gives Windows for its toasts.
//! The `.ico` and the `.png` only ever exist in the build directory: the
//! SVG is the single source.

use std::fs::{self, File};
use std::path::{Path, PathBuf};

use resvg::{tiny_skia, usvg};

const SOURCE: &str = "../brand/final/weft-icon-dark.svg";
/// The sizes Windows picks from, for the notification area, the taskbar
/// and Explorer at the usual display scales.
const SIZES: [u32; 6] = [16, 20, 24, 32, 48, 256];

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={SOURCE}");
    println!("cargo:rerun-if-env-changed=WEFT_VERSION");
    println!("cargo:rerun-if-env-changed=WEFT_VERSION_CODE");

    let version_name = std::env::var("WEFT_VERSION")
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| std::env::var("CARGO_PKG_VERSION").expect("set by cargo"));
    let version_code: u64 = match std::env::var("WEFT_VERSION_CODE") {
        Ok(raw) if !raw.trim().is_empty() => raw
            .trim()
            .parse::<u64>()
            .ok()
            .filter(|&code| code > 0)
            .expect("WEFT_VERSION_CODE must be a positive integer"),
        _ => 1,
    };
    println!("cargo:rustc-env=WEFT_VERSION_NAME={version_name}");
    println!("cargo:rustc-env=WEFT_VERSION_CODE={version_code}");

    // Resources only exist in Windows executables.
    if std::env::var_os("CARGO_CFG_WINDOWS").is_none() {
        return;
    }
    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").expect("set by cargo"));
    let icon = out_dir.join("weft.ico");
    write_icon(&icon, &out_dir.join("weft.png"));

    // Resource 1 is the icon Explorer shows, and the one the tray loads.
    let script = out_dir.join("weft.rc");
    let icon_path = icon.to_string_lossy().replace('\\', "/");
    fs::write(&script, format!("1 ICON \"{icon_path}\"\n")).expect("writing the resource script");
    embed_resource::compile(&script, embed_resource::NONE)
        .manifest_optional()
        .expect("compiling the icon resource");
}

/// Writes the icon to `path`, and its largest image as a PNG to `png`.
fn write_icon(path: &Path, png: &Path) {
    let svg = fs::read(SOURCE).expect("reading the brand icon");
    let tree = usvg::Tree::from_data(&svg, &usvg::Options::default()).expect("parsing the SVG");
    let mut directory = ico::IconDir::new(ico::ResourceType::Icon);
    for size in SIZES {
        let mut pixmap = tiny_skia::Pixmap::new(size, size).expect("a non-empty image");
        let scale = size as f32 / tree.size().width();
        resvg::render(
            &tree,
            tiny_skia::Transform::from_scale(scale, scale),
            &mut pixmap.as_mut(),
        );
        // The renderer works with premultiplied alpha; icons do not.
        let rgba = pixmap
            .pixels()
            .iter()
            .flat_map(|pixel| {
                let color = pixel.demultiply();
                [color.red(), color.green(), color.blue(), color.alpha()]
            })
            .collect();
        let image = ico::IconImage::from_rgba_data(size, size, rgba);
        // PNG for the large image, as Windows expects; bitmaps for the
        // small ones, which every icon reader understands.
        if size == SIZES[SIZES.len() - 1] {
            let file = File::create(png).expect("creating the PNG file");
            image.write_png(file).expect("writing the PNG file");
        }
        let entry = if size >= 256 {
            ico::IconDirEntry::encode_as_png(&image)
        } else {
            ico::IconDirEntry::encode_as_bmp(&image)
        };
        directory.add_entry(entry.expect("encoding the icon"));
    }
    let file = File::create(path).expect("creating the icon file");
    directory.write(file).expect("writing the icon file");
}
