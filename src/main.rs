use clap::Parser;
use std::{collections::HashSet, convert::Infallible, sync::Arc};

use http_body_util::Full;
use hyper::{
  body::{Bytes, Incoming},
  server::conn::http1,
  service::service_fn,
  Request, Response,
};
use hyper_util::rt::TokioIo;
use log::{debug, error, info};
use mime_guess::MimeGuess;
use tokio::net::TcpListener;

#[derive(Parser)]
struct Args {
  #[arg(short, long, default_value = "127.0.0.1:8080", help = "IP address to bind to")]
  address: String,
  #[arg(short, long, default_value = "Public", help = "Directory to serve files from")]
  dir: String,
  #[arg(short, long, default_value = "index.html", help = "Default file to serve")]
  index: String,
  #[arg(num_args = 0.., short, long, help = "Paths equivalent to /index.html")]
  paths: Vec<String>,
}

struct Config {
  dir: String,
  index: String,
  paths: HashSet<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
  env_logger::init();
  let args = Args::parse();
  let addr = args.address.parse::<std::net::SocketAddr>()?;
  let listener = TcpListener::bind(&addr).await?;
  info!("Listening on: 「{}」, serving files from 「{}」", addr, &args.dir);

  let config = Config { dir: args.dir.clone(), index: args.index.clone(), paths: list_to_set(args.paths) };

  let config = Arc::new(config);

  loop {
    let (stream, _) = listener.accept().await?;
    let io = TokioIo::new(stream);

    let config = Arc::clone(&config);
    tokio::task::spawn(async move {
      if let Err(err) = http1::Builder::new()
        .serve_connection(
          io,
          service_fn(move |req| {
            let acfg = Arc::clone(&config);
            async move { serve_file(req, Arc::clone(&acfg)).await }
          }),
        )
        .await
      {
        error!("Error serving connection: {:?}", err);
      }
    });
  }
}

fn list_to_set(list: Vec<String>) -> HashSet<String> {
  let mut set: HashSet<String> = list.into_iter().map(|s| if s.starts_with('/') { s } else { format!("/{}", s) }).collect();
  set.insert("/".to_string());
  set
}

async fn serve_file(req: Request<Incoming>, a_config: Arc<Config>) -> Result<Response<Full<Bytes>>, Infallible> {
  let mut path = req.uri().path();
  info!("Request: {}", path);
  // check if path contains ".." to prevent directory traversal
  if path.contains("..") {
    error!("Invalid path: {}", path);
    return Ok(Response::builder().status(400).body(Full::new(Bytes::from("400 Bad Request"))).unwrap());
  }
  let config = &a_config;
  path = if config.paths.contains(path) { config.index.as_str() } else { path };
  let file_path = format!("{}/{}", config.dir, path);
  let file = match tokio::fs::read(file_path).await {
    Ok(file) => file,
    Err(_) => {
      error!("File not found: {}", path);
      return Ok(Response::builder().status(404).body(Full::new(Bytes::from("404 Not Found"))).unwrap());
    }
  };
  let length = file.len();
  let mime = MimeGuess::from_path(path).first_or_text_plain();
  debug!("{}: {:?}", path, mime);

  // set the length of the file in the response header
  let mut response = Response::new(Full::new(Bytes::from(file)));
  response.headers_mut().insert("Content-Length", length.to_string().parse().unwrap());
  response.headers_mut().insert("Content-Type", mime.to_string().parse().unwrap());
  Ok(response)
}
