pub use remote_merge_protocol::*;

pub const CLI_VERSION: &str = remote_merge_protocol::product_cli_version!();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expected_version_line_format() {
        let line = crate::agent::deploy::expected_version_line(CLI_VERSION);
        assert_eq!(
            line,
            format!(
                "remote-merge {} (protocol v{})",
                env!("CARGO_PKG_VERSION"),
                crate::agent::protocol::PROTOCOL_VERSION
            )
        );
    }
}
