fn main() {
    // Tell cargo to recompile when embedded static files change
    println!("cargo::rerun-if-changed=src/static/viewer.html");
    println!("cargo::rerun-if-changed=src/static/viewer.js");
    println!("cargo::rerun-if-changed=src/static/viewer.css");
}
