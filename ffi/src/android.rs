//! Android only: hand iroh the JVM and an application context so its DNS
//! resolver can read the phone's nameservers (D3.3).
//!
//! iroh's resolver reads system DNS through JNI (`LinkProperties`), which
//! needs a `JavaVM` and a `Context` installed before the first endpoint
//! binds; without them it falls back to public nameservers, which is not
//! what a phone on a captive or private network resolves through. Upstream
//! iroh-ffi did this as `IrohAndroid.installAndroidContext`; this is the
//! same hook for our library, reached from Kotlin as
//! `ing.croft.call.net.CroftAndroid.installContext(context)` — a JNI native
//! method, not a uniffi export, because uniffi cannot carry a `jobject`.
//!
//! Idempotent, and that is not decoration: ndk-context's initializer
//! `assert!`s it was never called before, and an activity started twice in
//! one process (observed 2026-09-21 — two `START u0` a second apart on the
//! launch after a reinstall, two `MainViewModel`s, two calls) turned that
//! assert into a SIGABRT of the whole app. The first call wins; the second
//! is logged and ignored.
//!
//! Not testable on the JVM (there is no Android here); verified on a phone
//! by the fact that `relay.croft.ing` resolves and the endpoint camps.

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};

use jni::objects::{JClass, JObject};
use jni::JNIEnv;

/// `CroftAndroid.installContext(Context)`: installs the JVM and a global
/// reference to `context` for iroh's DNS resolver.
///
/// # Safety
///
/// Called by the JVM with a valid `JNIEnv` and a live `Context`. The global
/// reference is leaked on purpose — iroh's contract is that both pointers
/// stay valid until the process exits.
#[no_mangle]
pub unsafe extern "system" fn Java_ing_croft_call_net_CroftAndroid_installContext(
    env: JNIEnv,
    _class: JClass,
    context: JObject,
) {
    static INSTALLED: AtomicBool = AtomicBool::new(false);
    if INSTALLED.swap(true, Ordering::SeqCst) {
        tracing::info!("android context already installed for this process; keeping the first");
        return;
    }
    let vm = match env.get_java_vm() {
        Ok(vm) => vm,
        Err(e) => {
            tracing::error!(error = %e, "no JavaVM from JNIEnv; DNS will use fallback nameservers");
            return;
        }
    };
    let global = match env.new_global_ref(context) {
        Ok(g) => g,
        Err(e) => {
            tracing::error!(error = %e, "could not pin the Context; DNS will use fallback nameservers");
            return;
        }
    };
    let vm_ptr = vm.get_java_vm_pointer().cast::<c_void>();
    let ctx_ptr = global.as_raw().cast::<c_void>();
    // The global ref must outlive the process: iroh reads through it lazily.
    std::mem::forget(global);
    // SAFETY: both pointers come from a live JVM and are never released.
    unsafe {
        iroh_dns::install_android_jni_context(vm_ptr, ctx_ptr);
    }
    tracing::info!("android context installed for iroh's DNS resolver");
}
