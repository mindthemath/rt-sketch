fn main() {
    // Tell cargo to recompile when embedded static files change
    println!("cargo::rerun-if-changed=src/viewer/static/viewer.html");
    println!("cargo::rerun-if-changed=src/viewer/static/viewer.js");
    println!("cargo::rerun-if-changed=src/viewer/static/viewer.css");
}
