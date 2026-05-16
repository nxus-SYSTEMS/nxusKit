use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn default_license_endpoint_is_production() {
    let _guard = ENV_LOCK.lock().expect("env lock poisoned");
    let original = std::env::var("NXUSKIT_LICENSE_SERVER").ok();
    unsafe {
        std::env::remove_var("NXUSKIT_LICENSE_SERVER");
    }

    assert_eq!(
        nxuskit_core::license::default_license_server_url(),
        "https://nxus.systems/licensing-api/v1"
    );
    assert_eq!(
        nxuskit_core::license::license_server_url(),
        "https://nxus.systems/licensing-api/v1"
    );

    unsafe {
        match original {
            Some(value) => std::env::set_var("NXUSKIT_LICENSE_SERVER", value),
            None => std::env::remove_var("NXUSKIT_LICENSE_SERVER"),
        }
    }
}

#[test]
fn explicit_license_endpoint_override_is_visible() {
    let _guard = ENV_LOCK.lock().expect("env lock poisoned");
    let original = std::env::var("NXUSKIT_LICENSE_SERVER").ok();
    unsafe {
        std::env::set_var(
            "NXUSKIT_LICENSE_SERVER",
            "https://dev.nxus.systems/licensing-api/v1",
        );
    }

    assert_eq!(
        nxuskit_core::license::license_server_url(),
        "https://dev.nxus.systems/licensing-api/v1"
    );

    unsafe {
        match original {
            Some(value) => std::env::set_var("NXUSKIT_LICENSE_SERVER", value),
            None => std::env::remove_var("NXUSKIT_LICENSE_SERVER"),
        }
    }
}
