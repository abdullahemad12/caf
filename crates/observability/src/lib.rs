// plan:
// setup graphana. Report all errors there. If they are mainly part of a bad local setup, just
// report them as logs. If the are part of a long running operation create traces out of it.
// Enable info, warning and debug logs but only write them locally
// you will need a proxy though that actually conntect to graphana and forwards the logs

// This should not have a dependency on any other module from this project

pub mod error;
pub mod tracing;
