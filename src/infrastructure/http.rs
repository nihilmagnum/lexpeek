use std::time::Duration;

/// HTTP client that gives up on a request, including reading its body, after `timeout`.
pub fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(Some(timeout)).build().into()
}
