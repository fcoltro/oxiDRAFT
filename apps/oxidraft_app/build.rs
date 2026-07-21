fn main() {
    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=assets/oxidraft.ico");
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/oxidraft.ico");
        if let Err(e) = res.compile() {
            println!("cargo:warning=could not embed app icon: {e}");
        }
    }
}
