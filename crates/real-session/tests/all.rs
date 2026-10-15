//! The integration tests of real-session in one test binary: a build
//! links one binary, not one for each file. Each file is a module.

use std::sync::Mutex;

/// Held while a test writes a fake tool, and while it starts a process.
/// A process that starts while another thread writes the script gets a
/// copy of its open file, and then the exec of the script fails with
/// "Text file busy".
static SPAWN: Mutex<()> = Mutex::new(());

mod flow;
