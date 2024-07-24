use std::{
  borrow::Cow,
  fs::File,
  io::{BufRead, BufReader},
  path::{Path, PathBuf},
  str::FromStr,
  time::{Duration, Instant},
};

use anyhow::{bail, Context};
use clap::{builder::styling::*, ArgGroup, Parser};
use comfy_table::{modifiers::*, presets::*, Table};
use console::style;
use futures::StreamExt;
use http_body_util::BodyExt;
use hyper::{body::Bytes, Method, Request, Uri};
use hyper_util::{client::legacy::Client as HyperClient, rt::TokioExecutor};
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};

type Body = http_body_util::Full<Bytes>;
type TlsHyper = HyperClient<
  hyper_rustls::HttpsConnector<hyper_util::client::legacy::connect::HttpConnector>,
  Body,
>;

fn clap_v3_styles() -> Styles {
  Styles::styled()
    .header(AnsiColor::Yellow.on_default())
    .usage(AnsiColor::Green.on_default())
    .literal(AnsiColor::Green.on_default())
    .placeholder(AnsiColor::Green.on_default())
}

#[derive(Parser)]
#[command(author, version, about, long_about = None, styles = clap_v3_styles())]
#[clap(
  group = ArgGroup::new("url-input")
    .args(&["urls", "file"])
    .multiple(false)
    .required(true)
)]
struct Cli {
  urls: Option<Vec<Uri>>,
  #[clap(short, long)]
  file: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
  let cli = Cli::parse();
  let https = hyper_rustls::HttpsConnectorBuilder::new()
    .with_native_roots()?
    .https_or_http()
    .enable_http1()
    .enable_http2()
    .build();

  let client: TlsHyper = HyperClient::builder(TokioExecutor::new()).build(https);
  let builders = match cli {
    Cli {
      urls: Some(urls), ..
    } => urls
      .into_iter()
      .map(|i| Request::builder().method(Method::GET).uri(i))
      .collect(),
    Cli {
      file: Some(path), ..
    } => parse_from_path(path)?,
    _ => unreachable!(),
  };

  let mut results: Vec<TestData> = Vec::new();

  for builder in builders {
    let req = builder
      .body(Body::default())
      .context("Failed to build request")?;
    let uri = req.uri().clone();
    let method = req.method().clone();
    match test_and_render(&client, req).await {
      Ok(speed) => {
        results.push(TestData { uri, speed });
      },
      Err(err) => {
        let err = format!("{:?}", err.context(format!("Failed to {} {}", method, uri)));
        println!("{}", style(err).red());
        results.push(TestData { uri, speed: None });
        println!();
      },
    }
  }

  results.sort_unstable();

  let mut table = Table::new();
  table
    .load_preset(UTF8_FULL)
    .apply_modifier(UTF8_ROUND_CORNERS)
    .apply_modifier(UTF8_SOLID_INNER_BORDERS)
    .set_header(vec!["URL", "Speed"]);

  for data in results.into_iter().rev() {
    table.add_row([data.uri.to_string(), data.speed().into()]);
  }

  println!("{table}");

  Ok(())
}

#[derive(Debug, PartialEq, Eq)]
pub struct TestData {
  pub uri: Uri,
  pub speed: Option<u64>,
}

impl PartialOrd for TestData {
  fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
    self.speed.partial_cmp(&other.speed)
  }
}

impl Ord for TestData {
  fn cmp(&self, other: &Self) -> std::cmp::Ordering {
    self.speed.cmp(&other.speed)
  }
}

impl TestData {
  pub fn speed(&self) -> Cow<str> {
    match self.speed {
      Some(speed) => format!("{}/s", humansize::format_size(speed, humansize::BINARY)).into(),
      None => "N/A".into(),
    }
  }
}

async fn test_and_render(client: &TlsHyper, request: Request<Body>) -> anyhow::Result<Option<u64>> {
  println!(
    "{} {} {}",
    style("==>").magenta(),
    style(request.method()).green(),
    request.uri(),
  );

  let req_start = Instant::now();
  let resp = tokio::time::timeout(Duration::from_secs(10), async move {
    client.request(request).await
  })
  .await
  .context("Timed out for 10s")?
  .context("Failed to send request")?;
  let elapsed = req_start.elapsed();

  println!("{:?} {} {:?}", resp.version(), resp.status(), elapsed);

  if !resp.status().is_success() {
    bail!("HTTP response status is not success")
  }

  let total: Option<u64> = resp
    .headers()
    .get(hyper::header::CONTENT_LENGTH)
    .and_then(|val| {
      let str = std::str::from_utf8(val.as_bytes()).ok()?;
      str.parse().ok()
    });

  let mut body = resp.into_body().into_data_stream();

  let (tx, mut rx) = tokio::sync::mpsc::channel::<usize>(1);
  let download = tokio::spawn(async move {
    let tx = tx;
    while let Some(body) = body.next().await {
      let body = body.unwrap();
      tx.send(body.len()).await.unwrap();
    }
  });

  let render = tokio::spawn(async move {
    let pb = ProgressBar::with_draw_target(total, ProgressDrawTarget::stderr());
    pb.enable_steady_tick(Duration::from_millis(200));
    const STY_TEMP: &str = "{spinner:.green} [{elapsed_precise}] [{bar:.cyan/blue}] {percent}% ({binary_bytes_per_sec}, {eta})";
    pb.set_style(
      ProgressStyle::with_template(STY_TEMP)
        .unwrap()
        .progress_chars("#>-"),
    );

    let update = |len: usize, _immediate: bool| {
      pb.inc(len as u64);
    };

    while let Some(len) = rx.recv().await {
      if pb.elapsed().as_secs() > 60 {
        bail!("Testing takes too long (> 60s), stopping...");
      }
      update(len, false);
    }

    update(0, true);
    pb.finish();

    println!();
    println!();

    Ok((pb.position() * 1000).checked_div(pb.elapsed().as_millis() as u64))
  });
  download.await.context("Error when downloading")?;
  let speed = render.await.context("Failed to wait render thread")??;

  Ok(speed)
}

fn parse_from_path<P: AsRef<Path>>(path: P) -> anyhow::Result<Vec<http::request::Builder>> {
  let path = path.as_ref();
  let file =
    File::open(&path).with_context(|| format!("Failed to open file: {}", path.display()))?;
  let buf_rdr = BufReader::new(file);
  let mut vec = Vec::new();
  for (idx, line) in buf_rdr.lines().enumerate() {
    let line_num = idx + 1;
    let line = line.with_context(|| {
      format!(
        "Failed to parse txt line at {}:{}",
        path.display(),
        line_num
      )
    })?;
    if line.is_empty() || line.starts_with("#") || line.starts_with("//") {
      continue;
    }
    let mut split = line.splitn(3, ' ');
    let first = split.next();
    let second = split.next();
    let trail = split.next();
    let (method, uri) = match (first, second, trail) {
      (Some(uri), None, None) => (Method::GET, uri),
      (Some(method), Some(uri), None) => {
        let method = Method::from_str(method).with_context(|| {
          format!(
            "Unable to parse url file at {}:{}, invalid method",
            path.display(),
            line_num
          )
        })?;
        (method, uri)
      },
      (_, _, Some(_)) => {
        bail!(
          "Unable to parse url file at {}:{}, unexpected character after URL",
          path.display(),
          line_num
        );
      },
      _ => unreachable!(),
    };

    let uri = Uri::from_str(uri).with_context(|| {
      format!(
        "Unable to parse url file at {}:{}, invalid method",
        path.display(),
        line_num
      )
    })?;
    vec.push(Request::builder().method(method).uri(uri));
  }
  Ok(vec)
}
