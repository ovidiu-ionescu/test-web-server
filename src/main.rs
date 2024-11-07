use clap::Parser;
use std::{convert::Infallible, sync::Arc};

use http_body_util::Full;
use hyper::{
  body::{Bytes, Incoming},
  server::conn::http1,
  service::service_fn,
  Request, Response,
};
use hyper_util::rt::TokioIo;
use log::{debug, info};
use mime_guess::MimeGuess;
use tokio::net::TcpListener;

#[derive(Parser)]
struct Args {
  #[arg(short, long, default_value = "127.0.0.1:8080", help = "IP address to bind to")]
  address: String,
  #[arg(short, long, default_value = "Public", help = "Directory to serve files from")]
  dir: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
  env_logger::init();
  let args = Args::parse();
  let addr = args.address.parse::<std::net::SocketAddr>()?;
  let listener = TcpListener::bind(&addr).await?;
  info!("Listening on: 「{}」, serving files from 「{}」", addr, &args.dir);

  let config = Arc::new(args.dir.clone());

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
        eprintln!("Error serving connection: {:?}", err);
      }
    });
  }
}

async fn serve_file(req: Request<Incoming>, adir: Arc<String>) -> Result<Response<Full<Bytes>>, Infallible> {
  let path = req.uri().path();
  // check if path contains ".." to prevent directory traversal
  if path.contains("..") {
    eprintln!("Invalid path: {}", path);
    return Ok(Response::builder().status(400).body(Full::new(Bytes::from("400 Bad Request"))).unwrap());
  }
  let directory = &adir;
  let file_path = format!("{}/{}", directory, path);
  let file = match tokio::fs::read(file_path).await {
    Ok(file) => file,
    Err(_) => {
      eprintln!("File not found: {}", path);
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
