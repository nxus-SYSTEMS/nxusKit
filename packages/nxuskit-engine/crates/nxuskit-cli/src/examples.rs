//! Examples manifest browser for the nxusKit CLI.
//!
//! Reads `examples_manifest.json` and `example-groups.json` from the installed
//! SDK and provides list/show/filter functionality. Files are discovered at
//! runtime via `NXUSKIT_SDK_DIR` or `~/.nxuskit/sdk/current/`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

// ── Data types (T005, T006) ─────────────────────────────────────

/// Top-level manifest structure.
#[derive(Debug, Deserialize, Serialize)]
pub struct ExamplesManifest {
    pub version: String,
    pub examples: Vec<Example>,
}

/// A single SDK example with metadata, editorial content, and implementation links.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Example {
    pub name: String,
    pub description: String,
    pub category: String,
    pub scenario: String,
    pub real_world_application: String,
    pub tagline: String,
    pub blurb: String,
    pub content_hash: String,
    pub difficulty: Difficulty,
    pub tier: Tier,
    pub tech_tags: Vec<String>,
    pub languages: Vec<String>,
    pub implementations: HashMap<String, String>,
    // Optional fields present in manifest but not required by CLI.
    // These use serde_json::Value to accept any JSON type without failing.
    #[serde(default)]
    pub required: Option<bool>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub difficulty_override: Option<serde_json::Value>,
    #[serde(default)]
    pub difficulty_override_reason: Option<String>,
}

/// Difficulty level with display and sort ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Difficulty {
    Starter,
    Intermediate,
    Advanced,
}

impl Difficulty {
    /// Emoji indicator for human-readable output.
    pub fn emoji(self) -> &'static str {
        match self {
            Difficulty::Starter => "🟢",
            Difficulty::Intermediate => "🟡",
            Difficulty::Advanced => "🏁",
        }
    }

    /// Plain text indicator for NO_COLOR mode.
    pub fn plain(self) -> &'static str {
        match self {
            Difficulty::Starter => "[S]",
            Difficulty::Intermediate => "[I]",
            Difficulty::Advanced => "[A]",
        }
    }
}

impl fmt::Display for Difficulty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Difficulty::Starter => write!(f, "Starter"),
            Difficulty::Intermediate => write!(f, "Intermediate"),
            Difficulty::Advanced => write!(f, "Advanced"),
        }
    }
}

/// Tier classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Community,
    Pro,
}

impl fmt::Display for Tier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Tier::Community => write!(f, "Community"),
            Tier::Pro => write!(f, "Pro"),
        }
    }
}

/// Top-level groups configuration.
#[derive(Debug, Deserialize)]
pub struct ExampleGroupsConfig {
    #[allow(dead_code)]
    pub version: String,
    pub groups: Vec<ExampleGroup>,
}

/// A paradigm group definition for organizing examples.
#[derive(Debug, Clone, Deserialize)]
pub struct ExampleGroup {
    #[allow(dead_code)]
    pub slug: String,
    pub title: String,
    #[allow(dead_code)]
    pub description: String,
    #[serde(default)]
    pub filter_tags: Vec<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    pub order: u32,
}

// ── Manifest discovery and loading (T007, T008, T009) ───────────

/// Error type for manifest operations.
#[derive(Debug)]
pub enum ManifestError {
    NotFound { paths_searched: Vec<PathBuf> },
    ParseError { path: PathBuf, detail: String },
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ManifestError::NotFound { paths_searched } => {
                writeln!(f, "Error: Examples manifest not found.")?;
                writeln!(f)?;
                writeln!(f, "Searched:")?;
                for p in paths_searched {
                    writeln!(f, "  {}", p.display())?;
                }
                writeln!(f)?;
                write!(
                    f,
                    "Ensure the nxusKit SDK is installed at ~/.nxuskit/sdk/current/\n\
                     or set NXUSKIT_SDK_DIR to the SDK root directory."
                )
            }
            ManifestError::ParseError { path, detail } => {
                write!(
                    f,
                    "Error: Failed to parse manifest at {}: {}",
                    path.display(),
                    detail
                )
            }
        }
    }
}

/// Discover the conformance directory containing manifest files.
///
/// Search order:
/// 1. `NXUSKIT_SDK_DIR/conformance/`
/// 2. `~/.nxuskit/sdk/current/conformance/`
/// 3. Dev fallback: `sdk-packaging/conformance/` (relative to repo root)
pub fn find_conformance_dir() -> Result<PathBuf, ManifestError> {
    let manifest_name = "examples_manifest.json";
    let mut searched = Vec::new();

    // 1. NXUSKIT_SDK_DIR
    if let Ok(sdk_dir) = std::env::var("NXUSKIT_SDK_DIR") {
        let p = PathBuf::from(&sdk_dir).join("conformance");
        if p.join(manifest_name).is_file() {
            return Ok(p);
        }
        searched.push(p.join(manifest_name));
    }

    // 2. Standard install path
    if let Some(home) = dirs_home() {
        let p = home
            .join(".nxuskit")
            .join("sdk")
            .join("current")
            .join("conformance");
        if p.join(manifest_name).is_file() {
            return Ok(p);
        }
        searched.push(p.join(manifest_name));
    }

    // 3. Dev fallback — sdk-packaging/conformance/ relative to working dir
    let dev = PathBuf::from("sdk-packaging/conformance");
    if dev.join(manifest_name).is_file() {
        return Ok(dev);
    }
    searched.push(dev.join(manifest_name));

    Err(ManifestError::NotFound {
        paths_searched: searched,
    })
}

/// Load and parse the examples manifest from a conformance directory.
pub fn load_manifest(conformance_dir: &Path) -> Result<ExamplesManifest, ManifestError> {
    let path = conformance_dir.join("examples_manifest.json");
    let content = std::fs::read_to_string(&path).map_err(|e| ManifestError::ParseError {
        path: path.clone(),
        detail: e.to_string(),
    })?;
    serde_json::from_str(&content).map_err(|e| ManifestError::ParseError {
        path,
        detail: e.to_string(),
    })
}

/// Load and parse the example groups configuration. Returns None if the file
/// doesn't exist (groups are optional — the CLI can fall back to category-based grouping).
pub fn load_groups(conformance_dir: &Path) -> Option<ExampleGroupsConfig> {
    let path = conformance_dir.join("example-groups.json");
    let content = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

// ── Grouping and sorting (T013) ─────────────────────────────────

/// A resolved group with its matched examples, ready for display.
pub struct ResolvedGroup {
    pub title: String,
    pub examples: Vec<Example>,
}

/// Group and sort examples using the groups configuration.
///
/// Each example is assigned to the first group whose `filter_tags` overlap with
/// the example's `tech_tags`, or whose `categories` list contains the example's
/// `category`. Examples that match no group are collected into an "Other" group.
/// Within each group, examples are sorted by difficulty ascending, then name.
pub fn group_and_sort(
    examples: &[Example],
    groups: Option<&ExampleGroupsConfig>,
) -> Vec<ResolvedGroup> {
    match groups {
        Some(config) => group_by_config(examples, config),
        None => group_by_category(examples),
    }
}

fn group_by_config(examples: &[Example], config: &ExampleGroupsConfig) -> Vec<ResolvedGroup> {
    let mut groups_sorted: Vec<&ExampleGroup> = config.groups.iter().collect();
    groups_sorted.sort_by_key(|g| g.order);

    let mut assigned: Vec<bool> = vec![false; examples.len()];
    let mut result = Vec::new();

    for group in &groups_sorted {
        let mut matched: Vec<Example> = Vec::new();
        for (i, ex) in examples.iter().enumerate() {
            if assigned[i] {
                continue;
            }
            let tag_match = !group.filter_tags.is_empty()
                && ex.tech_tags.iter().any(|t| {
                    group
                        .filter_tags
                        .iter()
                        .any(|ft| ft.eq_ignore_ascii_case(t))
                });
            let cat_match = !group.categories.is_empty()
                && group
                    .categories
                    .iter()
                    .any(|c| c.eq_ignore_ascii_case(&ex.category));
            if tag_match || cat_match {
                matched.push(ex.clone());
                assigned[i] = true;
            }
        }
        if !matched.is_empty() {
            matched.sort_by(|a, b| a.difficulty.cmp(&b.difficulty).then(a.name.cmp(&b.name)));
            result.push(ResolvedGroup {
                title: group.title.clone(),
                examples: matched,
            });
        }
    }

    // Collect unmatched into "Other"
    let other: Vec<Example> = examples
        .iter()
        .enumerate()
        .filter(|(i, _)| !assigned[*i])
        .map(|(_, ex)| ex.clone())
        .collect();
    if !other.is_empty() {
        let mut other = other;
        other.sort_by(|a, b| a.difficulty.cmp(&b.difficulty).then(a.name.cmp(&b.name)));
        result.push(ResolvedGroup {
            title: "Other".to_string(),
            examples: other,
        });
    }

    result
}

fn group_by_category(examples: &[Example]) -> Vec<ResolvedGroup> {
    let category_order: &[(&str, &str)] = &[
        ("patterns", "LLM Patterns"),
        ("integrations", "Integrations"),
        ("apps", "Applications"),
    ];

    let mut result = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for &(cat, title) in category_order {
        let mut matched: Vec<Example> = examples
            .iter()
            .filter(|ex| ex.category == cat)
            .cloned()
            .collect();
        if !matched.is_empty() {
            matched.sort_by(|a, b| a.difficulty.cmp(&b.difficulty).then(a.name.cmp(&b.name)));
            for ex in &matched {
                seen.insert(ex.name.clone());
            }
            result.push(ResolvedGroup {
                title: title.to_string(),
                examples: matched,
            });
        }
    }

    // Catch-all for unknown categories
    let other: Vec<Example> = examples
        .iter()
        .filter(|ex| !seen.contains(&ex.name))
        .cloned()
        .collect();
    if !other.is_empty() {
        let mut other = other;
        other.sort_by(|a, b| a.difficulty.cmp(&b.difficulty).then(a.name.cmp(&b.name)));
        result.push(ResolvedGroup {
            title: "Other".to_string(),
            examples: other,
        });
    }

    result
}

// ── Filtering (T020) ────────────────────────────────────────────

/// Filter options for the list command.
#[derive(Default)]
pub struct FilterOptions {
    pub difficulty: Option<String>,
    pub tier: Option<String>,
    pub lang: Option<String>,
    pub tag: Option<String>,
}

impl FilterOptions {
    /// Build a human-readable description of active filters.
    pub fn description(&self) -> String {
        let mut parts = Vec::new();
        if let Some(d) = &self.difficulty {
            parts.push(format!("difficulty: {d}"));
        }
        if let Some(t) = &self.tier {
            parts.push(format!("tier: {t}"));
        }
        if let Some(l) = &self.lang {
            parts.push(format!("lang: {l}"));
        }
        if let Some(t) = &self.tag {
            parts.push(format!("tag: {t}"));
        }
        parts.join(", ")
    }
}

/// Validate filter values. Returns an error message if any value is invalid.
pub fn validate_filters(opts: &FilterOptions) -> Result<(), String> {
    if let Some(d) = &opts.difficulty {
        match d.to_lowercase().as_str() {
            "starter" | "intermediate" | "advanced" => {}
            _ => {
                return Err(format!(
                    "Invalid difficulty: '{d}'. Valid values: starter, intermediate, advanced"
                ));
            }
        }
    }
    if let Some(t) = &opts.tier {
        match t.to_lowercase().as_str() {
            "community" | "pro" => {}
            _ => return Err(format!("Invalid tier: '{t}'. Valid values: community, pro")),
        }
    }
    Ok(())
}

/// Filter examples by the given options. All filters combine with AND logic.
pub fn filter_examples(examples: &[Example], opts: &FilterOptions) -> Vec<Example> {
    examples
        .iter()
        .filter(|ex| {
            if let Some(d) = &opts.difficulty
                && !ex.difficulty.to_string().eq_ignore_ascii_case(d)
            {
                return false;
            }
            if let Some(t) = &opts.tier
                && !ex.tier.to_string().eq_ignore_ascii_case(t)
            {
                return false;
            }
            if let Some(l) = &opts.lang
                && !ex.languages.iter().any(|lang| lang.eq_ignore_ascii_case(l))
            {
                return false;
            }
            if let Some(tag) = &opts.tag
                && !ex.tech_tags.iter().any(|t| t.eq_ignore_ascii_case(tag))
            {
                return false;
            }
            true
        })
        .cloned()
        .collect()
}

// ── Formatting (T014, T025) ─────────────────────────────────────

/// Check if color/emoji output should be suppressed.
fn no_color() -> bool {
    std::env::var_os("NO_COLOR").is_some()
}

/// Format the list output as a human-readable grouped table.
pub fn format_examples_table(
    groups: &[ResolvedGroup],
    total_count: usize,
    filtered_count: usize,
    filter_desc: &str,
) -> String {
    let use_emoji = !no_color();
    let mut out = String::new();

    for (gi, group) in groups.iter().enumerate() {
        if gi > 0 {
            out.push('\n');
        }
        out.push_str(&group.title);
        out.push('\n');

        for ex in &group.examples {
            let diff_indicator = if use_emoji {
                format!("{} {:13}", ex.difficulty.emoji(), ex.difficulty)
            } else {
                format!("{} {:13}", ex.difficulty.plain(), ex.difficulty)
            };

            let tier_badge = if ex.tier == Tier::Pro {
                "  Pro"
            } else {
                "     "
            };

            let langs = ex
                .languages
                .iter()
                .map(|l| capitalize(l))
                .collect::<Vec<_>>()
                .join(" · ");

            // Truncate tagline to fit ~120 cols
            let prefix_len = 2 + 20 + diff_indicator.len() + 5 + langs.len() + 4;
            let max_tagline = 120usize.saturating_sub(prefix_len);
            let tagline = truncate_str(&ex.tagline, max_tagline);

            out.push_str(&format!(
                "  {:<20}{}{} {:18} {}\n",
                ex.name, diff_indicator, tier_badge, langs, tagline
            ));
        }
    }

    // Summary line
    if filtered_count == total_count {
        out.push_str(&format!("\nShowing {total_count} examples\n"));
    } else if filter_desc.is_empty() {
        out.push_str(&format!(
            "\nShowing {filtered_count} of {total_count} examples\n"
        ));
    } else {
        out.push_str(&format!(
            "\nShowing {filtered_count} of {total_count} examples ({filter_desc})\n"
        ));
    }

    out
}

/// Format full detail output for a single example.
pub fn format_example_detail(ex: &Example) -> String {
    let use_emoji = !no_color();
    let mut out = String::new();

    // Header
    out.push_str(&format!("{} — {}\n", ex.name, ex.tagline));
    out.push('\n');

    // Blurb (word-wrapped at ~78 cols with 2-space indent)
    for line in word_wrap(&ex.blurb, 76) {
        out.push_str(&format!("  {line}\n"));
    }
    out.push('\n');

    // Metadata
    let diff_str = if use_emoji {
        format!("{} {}", ex.difficulty.emoji(), ex.difficulty)
    } else {
        format!("{} {}", ex.difficulty.plain(), ex.difficulty)
    };
    out.push_str(&format!("  Difficulty:   {diff_str}\n"));
    out.push_str(&format!("  Tier:         {}\n", ex.tier));
    out.push_str(&format!("  Tags:         {}\n", ex.tech_tags.join(", ")));

    let langs = ex
        .languages
        .iter()
        .map(|l| capitalize(l))
        .collect::<Vec<_>>()
        .join(" · ");
    out.push_str(&format!("  Languages:    {langs}\n"));

    // Source links
    if !ex.implementations.is_empty() {
        out.push('\n');
        out.push_str("  Source:\n");
        let mut impls: Vec<_> = ex.implementations.iter().collect();
        impls.sort_by_key(|(lang, _)| lang.to_string());
        for (lang, path) in impls {
            let url = format!("https://github.com/nxus-SYSTEMS/nxusKit-examples/tree/main/{path}");
            out.push_str(&format!("    {:<10}{url}\n", capitalize(lang)));
        }
    }

    out
}

// ── Fuzzy matching (T026) ───────────────────────────────────────

/// Suggest similar example names using Jaro-Winkler distance.
pub fn suggest_similar_names(query: &str, examples: &[Example]) -> Vec<(String, f64)> {
    let mut suggestions: Vec<(String, f64)> = examples
        .iter()
        .map(|ex| {
            let score = strsim::jaro_winkler(&ex.name, query);
            (ex.name.clone(), score)
        })
        .filter(|(_, score)| *score > 0.7)
        .collect();
    suggestions.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    suggestions.truncate(3);
    suggestions
}

// ── CLI dispatch (T012, T015, T016) ─────────────────────────────

use clap::Subcommand;

/// Examples subcommand actions.
#[derive(Subcommand)]
pub enum ExamplesAction {
    /// List all examples with optional filtering
    List {
        /// Filter by difficulty: starter, intermediate, advanced
        #[arg(long)]
        difficulty: Option<String>,

        /// Filter by tier: community, pro
        #[arg(long)]
        tier: Option<String>,

        /// Filter by language: rust, go, python
        #[arg(long)]
        lang: Option<String>,

        /// Filter by tech tag (case-insensitive): LLM, CLIPS, Solver, BN, ZEN, etc.
        #[arg(long)]
        tag: Option<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show details for a specific example
    Show {
        /// Example name (e.g., cost-routing, basic-chat)
        name: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

/// Main entry point for the `examples` subcommand.
pub fn handle_examples_command(action: ExamplesAction) -> i32 {
    let conformance_dir = match find_conformance_dir() {
        Ok(dir) => dir,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };

    let manifest = match load_manifest(&conformance_dir) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };

    let groups_config = load_groups(&conformance_dir);

    match action {
        ExamplesAction::List {
            difficulty,
            tier,
            lang,
            tag,
            json,
        } => handle_list(
            &manifest,
            groups_config.as_ref(),
            difficulty,
            tier,
            lang,
            tag,
            json,
        ),
        ExamplesAction::Show { name, json } => handle_show(&manifest, &name, json),
    }
}

fn handle_list(
    manifest: &ExamplesManifest,
    groups_config: Option<&ExampleGroupsConfig>,
    difficulty: Option<String>,
    tier: Option<String>,
    lang: Option<String>,
    tag: Option<String>,
    json_output: bool,
) -> i32 {
    let opts = FilterOptions {
        difficulty,
        tier,
        lang,
        tag,
    };

    if let Err(e) = validate_filters(&opts) {
        eprintln!("Error: {e}");
        return 2;
    }

    let total = manifest.examples.len();
    let filtered = filter_examples(&manifest.examples, &opts);
    let filtered_count = filtered.len();

    if json_output {
        match serde_json::to_string_pretty(&filtered) {
            Ok(json) => {
                println!("{json}");
                return 0;
            }
            Err(e) => {
                eprintln!("Error: Failed to serialize JSON: {e}");
                return 1;
            }
        }
    }

    if filtered.is_empty() {
        let desc = opts.description();
        if desc.is_empty() {
            println!("No examples found in manifest.");
        } else {
            println!("No examples match the given filters ({desc}).");
        }
        return 0;
    }

    let groups = group_and_sort(&filtered, groups_config);
    let filter_desc = opts.description();
    let table = format_examples_table(&groups, total, filtered_count, &filter_desc);
    print!("{table}");
    0
}

fn handle_show(manifest: &ExamplesManifest, name: &str, json_output: bool) -> i32 {
    if let Some(ex) = manifest.examples.iter().find(|e| e.name == name) {
        if json_output {
            match serde_json::to_string_pretty(ex) {
                Ok(json) => {
                    println!("{json}");
                    return 0;
                }
                Err(e) => {
                    eprintln!("Error: Failed to serialize JSON: {e}");
                    return 1;
                }
            }
        }
        print!("{}", format_example_detail(ex));
        return 0;
    }

    // Not found — suggest similar names
    eprintln!("Error: No example named \"{name}\"");
    let suggestions = suggest_similar_names(name, &manifest.examples);
    if !suggestions.is_empty() {
        eprintln!();
        eprintln!("Did you mean?");
        for (suggestion, _) in &suggestions {
            if let Some(ex) = manifest.examples.iter().find(|e| &e.name == suggestion) {
                eprintln!("  {:<20}{}", suggestion, truncate_str(&ex.tagline, 80));
            }
        }
    }
    2
}

// ── Helpers ─────────────────────────────────────────────────────

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().to_string() + c.as_str(),
    }
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max || max < 4 {
        s.to_string()
    } else {
        format!("{}…", &s[..max - 1])
    }
}

fn word_wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if current.is_empty() {
            current = word.to_string();
        } else if current.len() + 1 + word.len() > width {
            lines.push(current);
            current = word.to_string();
        } else {
            current.push(' ');
            current.push_str(word);
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

// ── Tests (T007/T009, T010, T011, T019, T023, T024, T029) ──────

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;

    fn fixture_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
    }

    fn load_fixture_manifest() -> ExamplesManifest {
        load_manifest(&fixture_dir()).expect("fixture manifest should load")
    }

    fn load_fixture_groups() -> ExampleGroupsConfig {
        load_groups(&fixture_dir()).expect("fixture groups should load")
    }

    // ── T007/T009: manifest loading tests ───────────────────────

    #[test]
    fn test_load_manifest_from_fixture() {
        let manifest = load_fixture_manifest();
        assert_eq!(manifest.version, "4.0.0");
        assert_eq!(manifest.examples.len(), 5);
        assert_eq!(manifest.examples[0].name, "basic-chat");
    }

    #[test]
    fn test_load_manifest_missing_file() {
        let result = load_manifest(Path::new("/nonexistent/path"));
        assert!(result.is_err());
        match result.unwrap_err() {
            ManifestError::ParseError { path, .. } => {
                assert!(path.to_string_lossy().contains("nonexistent"));
            }
            _ => panic!("expected ParseError"),
        }
    }

    #[test]
    fn test_find_conformance_dir_not_found() {
        // With no env var and fake HOME, should fail
        let saved_sdk = std::env::var("NXUSKIT_SDK_DIR").ok();
        let saved_home = std::env::var("HOME").ok();
        // SAFETY: test-only env var manipulation, tests run single-threaded via
        // cargo test default (or --test-threads=1 for this specific test).
        unsafe {
            std::env::remove_var("NXUSKIT_SDK_DIR");
            std::env::set_var("HOME", "/nonexistent-home-for-test");
        }

        // Change to a temp dir so dev fallback doesn't work
        let tmp = std::env::temp_dir().join("nxuskit-test-empty");
        let _ = std::fs::create_dir_all(&tmp);
        let saved_cwd = std::env::current_dir().unwrap();
        let _ = std::env::set_current_dir(&tmp);

        let result = find_conformance_dir();
        assert!(result.is_err());

        // Restore
        let _ = std::env::set_current_dir(saved_cwd);
        unsafe {
            if let Some(v) = saved_sdk {
                std::env::set_var("NXUSKIT_SDK_DIR", v);
            }
            if let Some(v) = saved_home {
                std::env::set_var("HOME", v);
            }
        }
    }

    #[test]
    fn test_load_groups_from_fixture() {
        let groups = load_fixture_groups();
        assert_eq!(groups.groups.len(), 4);
        assert_eq!(groups.groups[0].slug, "llm-patterns");
    }

    // ── T010: format_examples_table ─────────────────────────────

    #[test]
    fn test_format_examples_table_contains_metadata() {
        let manifest = load_fixture_manifest();
        let groups_config = load_fixture_groups();
        let groups = group_and_sort(&manifest.examples, Some(&groups_config));
        let output = format_examples_table(&groups, 5, 5, "");

        assert!(
            output.contains("basic-chat"),
            "should contain example name, got:\n{output}"
        );
        assert!(
            output.contains("Starter"),
            "should contain difficulty, got:\n{output}"
        );
        assert!(
            output.contains("Pro"),
            "should contain Pro badge for solver, got:\n{output}"
        );
        assert!(
            output.contains("Rust"),
            "should contain language, got:\n{output}"
        );
        // Tagline may be truncated — check for partial match
        assert!(
            output.contains("one-shot") || output.contains("hello"),
            "should contain tagline text (possibly truncated), got:\n{output}"
        );
        assert!(
            output.contains("Showing 5 examples"),
            "should contain summary line, got:\n{output}"
        );
    }

    // ── T011: sort ordering ─────────────────────────────────────

    #[test]
    fn test_group_and_sort_ordering() {
        let manifest = load_fixture_manifest();
        let groups_config = load_fixture_groups();
        let groups = group_and_sort(&manifest.examples, Some(&groups_config));

        // First group should be LLM Patterns (order 1)
        assert_eq!(groups[0].title, "LLM Patterns");
        // Within LLM Patterns, starter comes before intermediate
        let difficulties: Vec<_> = groups[0].examples.iter().map(|e| e.difficulty).collect();
        for w in difficulties.windows(2) {
            assert!(w[0] <= w[1], "difficulty should be ascending within group");
        }
    }

    #[test]
    fn test_group_by_config_assigns_correctly() {
        let manifest = load_fixture_manifest();
        let groups_config = load_fixture_groups();
        let groups = group_and_sort(&manifest.examples, Some(&groups_config));

        // solver has Solver tag → should be in Constraint Solvers group
        let solver_group = groups
            .iter()
            .find(|g| g.title == "Constraint Solvers")
            .expect("should have Constraint Solvers group");
        assert!(solver_group.examples.iter().any(|e| e.name == "solver"));

        // clips-basics has CLIPS tag → Rule Engines group
        let clips_group = groups
            .iter()
            .find(|g| g.title == "Rule Engines & Decision Tables")
            .expect("should have Rule Engines group");
        assert!(
            clips_group
                .examples
                .iter()
                .any(|e| e.name == "clips-basics")
        );

        // cli-assistant has LLM+Streaming tags → assigned to LLM Patterns
        // (first matching group wins; tag match takes priority over category)
        let llm_group = &groups[0];
        assert!(
            llm_group.examples.iter().any(|e| e.name == "cli-assistant"),
            "cli-assistant should be in LLM Patterns (matched by LLM+Streaming tags)"
        );
    }

    // ── T019: filter_examples ───────────────────────────────────

    #[test]
    fn test_filter_by_difficulty() {
        let manifest = load_fixture_manifest();
        let opts = FilterOptions {
            difficulty: Some("starter".to_string()),
            ..Default::default()
        };
        let result = filter_examples(&manifest.examples, &opts);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "basic-chat");
    }

    #[test]
    fn test_filter_by_tier() {
        let manifest = load_fixture_manifest();
        let opts = FilterOptions {
            tier: Some("pro".to_string()),
            ..Default::default()
        };
        let result = filter_examples(&manifest.examples, &opts);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "solver");
    }

    #[test]
    fn test_filter_by_lang() {
        let manifest = load_fixture_manifest();
        let opts = FilterOptions {
            lang: Some("python".to_string()),
            ..Default::default()
        };
        let result = filter_examples(&manifest.examples, &opts);
        // basic-chat, solver, clips-basics have python
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_filter_by_tag_case_insensitive() {
        let manifest = load_fixture_manifest();
        let opts = FilterOptions {
            tag: Some("clips".to_string()), // lowercase, manifest has "CLIPS"
            ..Default::default()
        };
        let result = filter_examples(&manifest.examples, &opts);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "clips-basics");
    }

    #[test]
    fn test_filter_and_combination() {
        let manifest = load_fixture_manifest();
        let opts = FilterOptions {
            difficulty: Some("intermediate".to_string()),
            lang: Some("python".to_string()),
            ..Default::default()
        };
        let result = filter_examples(&manifest.examples, &opts);
        // intermediate + python = solver, clips-basics
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_filter_zero_results() {
        let manifest = load_fixture_manifest();
        let opts = FilterOptions {
            difficulty: Some("advanced".to_string()),
            tier: Some("pro".to_string()),
            ..Default::default()
        };
        let result = filter_examples(&manifest.examples, &opts);
        assert!(result.is_empty());
    }

    #[test]
    fn test_validate_filters_rejects_invalid() {
        let opts = FilterOptions {
            difficulty: Some("expert".to_string()),
            ..Default::default()
        };
        assert!(validate_filters(&opts).is_err());
    }

    // ── T023: format_example_detail ─────────────────────────────

    #[test]
    fn test_format_example_detail_contains_all_fields() {
        let manifest = load_fixture_manifest();
        let ex = manifest
            .examples
            .iter()
            .find(|e| e.name == "cost-routing")
            .unwrap();
        let output = format_example_detail(ex);

        assert!(output.contains("cost-routing"));
        assert!(output.contains(&ex.tagline));
        assert!(output.contains("Cost-aware routing")); // blurb start
        assert!(output.contains("Intermediate"));
        assert!(output.contains("Community"));
        assert!(output.contains("LLM"));
        assert!(output.contains("Rust · Go"));
        assert!(output.contains("github.com/nxus-SYSTEMS/nxusKit-examples"));
    }

    // ── T024: suggest_similar_names ─────────────────────────────

    #[test]
    fn test_suggest_similar_names_finds_match() {
        let manifest = load_fixture_manifest();
        let suggestions = suggest_similar_names("cost-route", &manifest.examples);
        assert!(!suggestions.is_empty());
        assert_eq!(suggestions[0].0, "cost-routing");
    }

    #[test]
    fn test_suggest_similar_names_no_match_for_unrelated() {
        let manifest = load_fixture_manifest();
        let suggestions = suggest_similar_names("zzzzzzzzz", &manifest.examples);
        assert!(suggestions.is_empty());
    }

    // ── T029: JSON output ───────────────────────────────────────

    #[test]
    fn test_json_list_output_valid() {
        let manifest = load_fixture_manifest();
        let json = serde_json::to_string_pretty(&manifest.examples).unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.len(), 5);
        assert_eq!(parsed[0]["name"], "basic-chat");
        assert!(parsed[0]["tagline"].is_string());
        assert!(parsed[0]["difficulty"].is_string());
    }

    #[test]
    fn test_json_show_output_valid() {
        let manifest = load_fixture_manifest();
        let ex = &manifest.examples[0];
        let json = serde_json::to_string_pretty(ex).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["name"], "basic-chat");
        assert!(parsed["blurb"].is_string());
        assert!(parsed["implementations"].is_object());
    }
}
