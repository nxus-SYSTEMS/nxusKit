//! Demonstrates Community-tier CLIPS session limit behavior.
//!
//! Creates CLIPS sessions in a loop until the Community-tier limit is hit,
//! then displays the error message showing tier name, limit, and upgrade URL.

fn main() {
    println!("=== nxusKit Session Limit Example ===\n");
    println!("Creating CLIPS sessions until the Community-tier limit is hit...\n");

    let mut sessions = Vec::new();
    let mut count = 0;

    loop {
        match nxuskit::ClipsSession::create() {
            Ok(session) => {
                count += 1;
                println!("  Created session #{count}");
                sessions.push(session);
            }
            Err(e) => {
                println!("\n--- Session limit reached after {count} sessions ---");
                println!("Error: {e}");
                println!("\nThis is expected behavior for the Community tier.");
                println!("Upgrade at https://nxus.systems/pricing for higher limits.");
                break;
            }
        }

        // Safety: prevent infinite loop if limits are not enforced
        if count > 1000 {
            println!("\nReached safety cap of 1000 sessions without hitting a limit.");
            println!("This may indicate Pro/Enterprise tier or unlimited sessions.");
            break;
        }
    }

    // Cleanup
    let cleanup_count = sessions.len();
    drop(sessions);
    println!("\nCleaned up {cleanup_count} sessions.");
}
