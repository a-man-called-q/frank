These task definitions are inherited by Rust projects. The explicit project
tasks live next to each synthetic Moon project so every public command has a
stable, inspectable target. Retries are intentionally disabled globally: a
flaky test is a failure and must not be hidden by Moon's pipeline.
