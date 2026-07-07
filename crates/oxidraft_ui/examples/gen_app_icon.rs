use std::path::Path;

fn main() {
    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/oxidraft_app/assets");
    std::fs::create_dir_all(&assets).expect("create assets dir");

    let ico = oxidraft_ui::icons::app_icon_ico().expect("rasterize .ico");
    std::fs::write(assets.join("oxidraft.ico"), &ico).expect("write .ico");

    let png = oxidraft_ui::icons::app_icon_png(512).expect("rasterize .png");
    std::fs::write(assets.join("oxidraft.png"), &png).expect("write .png");

    println!(
        "wrote oxidraft.ico ({} bytes), oxidraft.png ({} bytes) to {}",
        ico.len(),
        png.len(),
        assets.display()
    );
}
