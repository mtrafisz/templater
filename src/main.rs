mod templater;
mod cli;

fn main() {
    colog::init();
    let args = cli::Command::clap_parse();
    match templater::Templater::run_command(args) {
        Ok(_) => {}
        Err(e) => {
            let error_chain: Vec<String> = e.chain()
                .map(|e| e.to_string())
                .collect();
            log::error!("{}", error_chain.join("\n"));
            std::process::exit(1);
        }
    }
}
