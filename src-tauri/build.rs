fn main() {
    // The desktop Release executable embeds frontend assets. Without this
    // dependency, a frontend-only V0-014 change can leave an old UI in the
    // raw executable even though `npm run build` completed successfully.
    println!("cargo:rerun-if-changed=../dist");
    tauri_build::build()
}
