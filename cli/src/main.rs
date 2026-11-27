use std::process::ExitCode;

fn main() -> ExitCode {
    match custom_ring_cli::parse_and_run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let chain = format!("{error:?}");
            eprintln!("Error: {}", zolana_ring_indexer::redact::redacted(&chain));
            ExitCode::FAILURE
        }
    }
}
