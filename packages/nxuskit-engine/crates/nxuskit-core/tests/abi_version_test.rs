use std::ffi::CStr;

#[test]
fn abi_version_reports_v094() {
    let ptr = nxuskit_core::nxuskit_abi_version();
    assert!(!ptr.is_null());
    let version = unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .expect("ABI version must be valid UTF-8");
    assert_eq!(version, "0.9.4");
}

#[test]
fn capabilities_report_same_abi_version() {
    let ptr = nxuskit_core::nxuskit_capabilities();
    assert!(!ptr.is_null());
    let json = unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .expect("capabilities JSON must be valid UTF-8")
        .to_string();
    unsafe {
        nxuskit_core::nxuskit_free_string(ptr);
    }

    let capabilities: serde_json::Value =
        serde_json::from_str(&json).expect("capabilities JSON must parse");
    assert_eq!(capabilities["abi_version"], "0.9.4");
    assert_eq!(capabilities["sdk_version"], "0.9.4");
}
