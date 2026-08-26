//! D1 SPIKE — the uniffi bindgen CLI, per uniffi's recommended layout.
//! Built with `--features cli`; generates the Kotlin bindings from the cdylib.

fn main() {
    uniffi::uniffi_bindgen_main()
}
