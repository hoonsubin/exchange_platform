//! Substrate Node Template CLI library.

#![warn(missing_docs)]
#![warn(unused_extern_crates)]

mod chain_spec;
mod service;
mod cli;

pub use substrate_cli::{VersionInfo, IntoExit, error};

fn run() -> cli::error::Result<()> {
	let version = VersionInfo {
		name: "Stock Exchange Node",
		commit: env!("VERGEN_SHA_SHORT"),
		version: env!("CARGO_PKG_VERSION"),
		executable_name: "exchange-platform",
		author: "Hoon Kim",
		description: "exchange_platform",
		support_url: "https://github.com/hoonsubin/exchange_platform",
	};
	cli::run(::std::env::args(), cli::Exit, version)
}

error_chain::quick_main!(run);
