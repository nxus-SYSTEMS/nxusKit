//! Demonstrates Community-tier per-session rule limit behavior.
//!
//! Creates a single CLIPS session and loads rules until the per-session
//! rule limit is hit, then displays the error message with limit details.

fn main() {
    println!("=== nxusKit Rule Limit Example ===\n");

    let session = match nxuskit::ClipsSession::create() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to create session: {e}");
            std::process::exit(1);
        }
    };

    println!("Session created. Loading rules until the limit is hit...\n");

    let mut count = 0;
    loop {
        let rule = format!(
            "(defrule limit-test-rule-{count} (test-fact-{count}) => (assert (result-{count})))"
        );

        match session.build(&rule) {
            Ok(_) => {
                count += 1;
                if count % 100 == 0 {
                    println!("  Loaded {count} rules...");
                }
            }
            Err(e) => {
                println!("\n--- Rule limit reached after {count} rules ---");
                println!("Error: {e}");
                println!("\nThis is expected behavior for the Community tier.");
                println!("Upgrade at https://nxus.systems/pricing for higher limits.");
                break;
            }
        }

        // Safety: prevent infinite loop
        if count > 100_000 {
            println!("\nReached safety cap of 100,000 rules without hitting a limit.");
            println!("This may indicate Pro/Enterprise tier or unlimited rules.");
            break;
        }
    }

    println!("\nSession info: {:?}", session.info());
}
