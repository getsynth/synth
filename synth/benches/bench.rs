// we can ignore irrefutable patterns here, because we might run this with or without a feature
#![allow(irrefutable_let_patterns)]

use std::io;

use synth::cli::{Args, Cli, GenerateCommand};

fn bench_generate_1_to_stdout() {
    bench_generate_n_to_stdout(1);
}

fn bench_generate_100_to_stdout() {
    bench_generate_n_to_stdout(100);
}

fn bench_generate_10000_to_stdout() {
    bench_generate_n_to_stdout(10000);
}

fn bench_generate_n_to_stdout(size: usize) {
    async_std::task::block_on(async {
        let namespace = std::path::PathBuf::from("testing_harness/postgres/hospital_master");
        let args = Args::Generate(GenerateCommand {
            namespace,
            collection: None,
            scenario: None,
            size,
            to: "json:".to_string(),
            seed: Some(0),
            random: false,
            schema: None,
            concurrency: 3,
        });
        let output = io::stdout();
        Cli::new().unwrap().run(args, output).await.unwrap()
    });
}

iai::main!(
    bench_generate_1_to_stdout,
    bench_generate_100_to_stdout,
    bench_generate_10000_to_stdout,
);
