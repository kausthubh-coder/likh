fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    #[cfg(windows)]
    {
        winresource::WindowsResource::new()
            .set_icon("assets/icon.ico")
            .set("ProductName", "likh")
            .set("FileDescription", "likh — Talk. It types.")
            .compile()
            .expect("compile the Windows app icon and version information");
    }
}
