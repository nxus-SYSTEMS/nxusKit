fn forbidden_symbols() -> Vec<&'static str> {
    include_str!("fixtures/forbidden_tool_symbols.txt")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
}

fn contains_forbidden_symbol(surface: &str, symbol: &str) -> bool {
    if symbol.contains("::") || symbol.contains('_') {
        return surface.contains(symbol);
    }

    surface
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
        .any(|token| token == symbol)
}

#[test]
fn tools_public_items_are_accessible_without_internal_types() {
    let _ = std::any::type_name::<nxuskit::tools::ExternalTool>();
    let _ = std::any::type_name::<nxuskit::tools::ToolResult>();
    let _ = std::any::type_name::<nxuskit::tools::CeFacadeError>();
    let _ = std::any::type_name::<nxuskit::tools::ToolCallingSupport>();
    let _ = std::any::type_name::<nxuskit::tools::CeToolPosture>();
}

#[test]
fn tools_module_source_does_not_leak_internal_symbols_or_paths() {
    let tools_mod = include_str!("../src/tools/mod.rs");
    let lib_rs = include_str!("../src/lib.rs");
    let public_surface = format!("{tools_mod}\n{lib_rs}");

    for symbol in forbidden_symbols() {
        assert!(
            !contains_forbidden_symbol(&public_surface, symbol),
            "public CE tools surface leaked forbidden symbol or path `{symbol}`"
        );
    }
}

#[test]
fn ce_facade_errors_do_not_expose_internal_topology() {
    let rendered = format!(
        "{:?} {}",
        nxuskit::tools::CeFacadeError::Unavailable,
        nxuskit::tools::CeFacadeError::Contract("bad user tool schema".to_string())
    );
    let rendered_lower = rendered.to_lowercase();

    for forbidden in [
        "channel",
        "tool_catalog",
        "internal",
        "pro_",
        "nxuskit_engine",
        "packages/",
    ] {
        assert!(
            !rendered_lower.contains(forbidden),
            "CE facade error leaked forbidden detail `{forbidden}`"
        );
    }
}
