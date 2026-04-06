use nicecli_backend::{load_state_from_bootstrap, serve, BackendBootstrap};
use tokio::net::TcpListener;

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.windows(2).find_map(|pair| {
        if pair[0] == flag {
            Some(pair[1].clone())
        } else {
            None
        }
    })
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let config_path = arg_value(&args, "--config").unwrap_or_else(|| "config.yaml".to_string());
    let password = arg_value(&args, "--password")
        .or_else(|| std::env::var("NICECLI_LOCAL_PASSWORD").ok())
        .unwrap_or_default();

    let bootstrap = BackendBootstrap::new(config_path).with_local_management_password(password);
    let state = match load_state_from_bootstrap(bootstrap.clone()) {
        Ok(state) => state,
        Err(error) => {
            eprintln!("failed to load backend state: {error}");
            std::process::exit(1);
        }
    };

    let bind_addr = format!(
        "{}:{}",
        state.config.effective_host(),
        state.config.effective_port()
    );

    let listener = match TcpListener::bind(&bind_addr).await {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("failed to bind {bind_addr}: {error}");
            std::process::exit(1);
        }
    };

    println!("nicecli-backend listening on {bind_addr}");
    if let Err(error) = serve(bootstrap, listener).await {
        eprintln!("backend exited with error: {error}");
        std::process::exit(1);
    }
}
