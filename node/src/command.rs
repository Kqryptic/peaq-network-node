use crate::{
	chain_spec,
	cli::{Cli, Subcommand},
	cli_opt::{EthApi, RpcConfig},
	service::{self, frontier_database_dir},
};
use frame_benchmarking_cli::BenchmarkCmd;
use peaq_node_runtime::Block;
use sc_cli::{ChainSpec, RuntimeVersion, SubstrateCli};
use sc_service::{DatabaseSource, PartialComponents};

impl SubstrateCli for Cli {
	fn impl_name() -> String {
		"PEAQ Node".into()
	}

	fn impl_version() -> String {
		env!("SUBSTRATE_CLI_IMPL_VERSION").into()
	}

	fn description() -> String {
		//[TODO]
		env!("CARGO_PKG_DESCRIPTION").into()
	}

	fn author() -> String {
		env!("CARGO_PKG_AUTHORS").into()
	}

	fn support_url() -> String {
		//[TODO]
		"support.anonymous.an".into()
	}

	fn copyright_start_year() -> i32 {
		2021
	}

	fn load_spec(&self, id: &str) -> Result<Box<dyn sc_service::ChainSpec>, String> {
		Ok(match id {
			"dev" => Box::new(chain_spec::development_config()?),
			"agung" => Box::new(chain_spec::agung_net_config()?),
			path =>
				Box::new(chain_spec::ChainSpec::from_json_file(std::path::PathBuf::from(path))?),
		})
	}

	fn native_runtime_version(_: &Box<dyn ChainSpec>) -> &'static RuntimeVersion {
		&peaq_node_runtime::VERSION
	}
}

fn validate_trace_environment(cli: &Cli) -> sc_cli::Result<()> {
	if (cli.run.ethapi.contains(&EthApi::Debug) || cli.run.ethapi.contains(&EthApi::Trace)) &&
		cli.run.base.import_params.wasm_runtime_overrides.is_none()
	{
		return Err(
			"`debug` or `trace` namespaces requires `--wasm-runtime-overrides /path/to/overrides`."
				.into(),
		)
	}
	Ok(())
}

/// Parse and run command line arguments
pub fn run() -> sc_cli::Result<()> {
	let cli = Cli::from_args();
	validate_trace_environment(&cli)?;

	match &cli.subcommand {
		Some(Subcommand::Key(cmd)) => cmd.run(&cli),
		Some(Subcommand::BuildSpec(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.sync_run(|config| cmd.run(config.chain_spec, config.network))
		},
		Some(Subcommand::CheckBlock(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.async_run(|mut config| {
				let PartialComponents { client, task_manager, import_queue, .. } =
					service::new_partial(&mut config, &cli)?;
				Ok((cmd.run(client, import_queue), task_manager))
			})
		},
		Some(Subcommand::ExportBlocks(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.async_run(|mut config| {
				let PartialComponents { client, task_manager, .. } =
					service::new_partial(&mut config, &cli)?;
				Ok((cmd.run(client, config.database), task_manager))
			})
		},
		Some(Subcommand::ExportState(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.async_run(|mut config| {
				let PartialComponents { client, task_manager, .. } =
					service::new_partial(&mut config, &cli)?;
				Ok((cmd.run(client, config.chain_spec), task_manager))
			})
		},
		Some(Subcommand::ImportBlocks(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.async_run(|mut config| {
				let PartialComponents { client, task_manager, import_queue, .. } =
					service::new_partial(&mut config, &cli)?;
				Ok((cmd.run(client, import_queue), task_manager))
			})
		},
		Some(Subcommand::PurgeChain(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.sync_run(|config| {
				// Remove Frontier offchain db
				let frontier_database_config = match config.database {
					DatabaseSource::RocksDb { .. } => DatabaseSource::RocksDb {
						path: frontier_database_dir(&config, "db"),
						cache_size: 0,
					},
					DatabaseSource::ParityDb { .. } => DatabaseSource::ParityDb {
						path: frontier_database_dir(&config, "paritydb"),
					},
					_ =>
						return Err(format!("Cannot purge `{:?}` database", config.database).into()),
				};
				cmd.run(frontier_database_config)
			})
		},
		Some(Subcommand::Revert(cmd)) => {
			let runner = cli.create_runner(cmd)?;
			runner.async_run(|mut config| {
				let PartialComponents { client, task_manager, backend, .. } =
					service::new_partial(&mut config, &cli)?;
				Ok((cmd.run(client, backend, None), task_manager))
			})
		},
		// ExportGenesisState and ExortGenesisWasm uses for Substrate-based Polkadot parachains,
		// so won't porting it now.
		Some(Subcommand::Benchmark(cmd)) =>
			if cfg!(feature = "runtime-benchmarks") {
				let runner = cli.create_runner(cmd)?;
				match cmd {
					BenchmarkCmd::Pallet(cmd) => runner
						.sync_run(|config| cmd.run::<Block, service::ExecutorDispatch>(config)),
					BenchmarkCmd::Block(cmd) => runner.sync_run(|mut config| {
						let params = service::new_partial(&mut config, &cli)?;

						cmd.run(params.client)
					}),
					BenchmarkCmd::Storage(cmd) => runner.sync_run(|mut config| {
						let params = service::new_partial(&mut config, &cli)?;

						let db = params.backend.expose_db();
						let storage = params.backend.expose_storage();

						cmd.run(config, params.client, db, storage)
					}),
					BenchmarkCmd::Extrinsic(_) => Err("Unsupported benchmarking command".into()),
					BenchmarkCmd::Overhead(_) => Err("Unsupported benchmarking command".into()),
					BenchmarkCmd::Machine(cmd) => runner.sync_run(|config| {
						cmd.run(
							&config,
							frame_benchmarking_cli::SUBSTRATE_REFERENCE_HARDWARE.clone(),
						)
					}),
				}
			} else {
				Err("Benchmarking wasn't enabled when building the node. You can enable it with \
					 `--features runtime-benchmarks`."
					.into())
			},
		None => {
			let runner = cli.create_runner(&cli.run.base)?;
			runner.run_node_until_exit(|config| async move {
				let rpc_config = RpcConfig {
					ethapi: cli.run.ethapi.clone(),
					ethapi_max_permits: cli.run.ethapi_max_permits,
					ethapi_trace_max_count: cli.run.ethapi_trace_max_count,
					ethapi_trace_cache_duration: cli.run.ethapi_trace_cache_duration,
					eth_log_block_cache: cli.run.eth_log_block_cache,
					eth_statuses_cache: cli.run.eth_statuses_cache,
					fee_history_limit: cli.run.fee_history_limit,
					max_past_logs: cli.run.max_past_logs,
					tracing_raw_max_memory_usage: cli.run.tracing_raw_max_memory_usage,
				};

				service::new_full(config, &cli, rpc_config).map_err(sc_cli::Error::Service)
			})
		},
	}
}
