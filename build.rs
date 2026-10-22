fn main() {
    println!("cargo:rerun-if-changed=assets/flux.ico");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut resources = winres::WindowsResource::new();
        resources
            .set_icon("assets/flux.ico")
            .set("FileDescription", "Flux Auto Clicker")
            .set("ProductName", "Flux");
        resources
            .compile()
            .expect("Could not embed the Flux application icon");
    }
}
