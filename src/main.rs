use std::{convert::Infallible, net::SocketAddr};

use http_body_util::Full;
use hyper::{
  body::{Bytes, Incoming},
  server::conn::http1,
  service::service_fn,
  Request, Response,
};
use hyper_util::rt::TokioIo;
use mime_guess::MimeGuess;
use tokio::net::TcpListener;
use log::debug;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
  let addr = SocketAddr::from(([127, 0, 0, 1], 8080));
  let listener = TcpListener::bind(&addr).await?;

  loop {
    let (stream, _) = listener.accept().await?;
    let io = TokioIo::new(stream);

    tokio::task::spawn(async move {
      if let Err(err) = http1::Builder::new().serve_connection(io, service_fn(hello)).await {
        eprintln!("Error serving connection: {:?}", err);
      }
    });
  }
}

const DIR: &str = "Public";

async fn hello(req: Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
  let path = req.uri().path();
  // check if path contains ".." to prevent directory traversal
  if path.contains("..") {
    eprintln!("Invalid path: {}", path);
    return Ok(Response::builder().status(400).body(Full::new(Bytes::from("400 Bad Request"))).unwrap());
  }
  let file_path = format!("{}/{}", DIR, path);
  let file = match tokio::fs::read(file_path).await {
    Ok(file) => file,
    Err(_) => {
      eprintln!("File not found: {}", path);
      return Ok(Response::builder().status(404).body(Full::new(Bytes::from("404 Not Found"))).unwrap());
    }
  };
  let length = file.len();
  //let mime = "text/html";
  let mime = MimeGuess::from_path(&path).first_or_text_plain();
  debug!("{}: {:?}", path, mime);

  // set the length of the file in the response header
  let mut response = Response::new(Full::new(Bytes::from(file)));
  response.headers_mut().insert("Content-Length", length.to_string().parse().unwrap());
  response.headers_mut().insert("Content-Type", mime.to_string().parse().unwrap());
  Ok(response)
}
