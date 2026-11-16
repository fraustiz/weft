//! Renders the application icon from the brand SVG and embeds it in the
//! executable. The `.ico` only ever exists in the build directory: the SVG
//! is the single source.

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
    // Resources only exist in Windows executables.
    if std::env::var_os("CARGO_CFG_WINDOWS").is_none() {
        return;
    }
    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").expect("set by cargo"));
    let icon = out_dir.join("weft.ico");
    write_icon(&icon);

    // Resource 1 is the icon Explorer shows, and the one the tray loads.
    let script = out_dir.join("weft.rc");
    let icon_path = icon.to_string_lossy().replace('\\', "/");
    fs::write(&script, format!("1 ICON \"{icon_path}\"\n")).expect("writing the resource script");
    embed_resource::compile(&script, embed_resource::NONE)
        .manifest_optional()
        .expect("compiling the icon resource");
}

fn write_icon(path: &Path) {
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
