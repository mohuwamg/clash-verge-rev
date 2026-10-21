use super::policy;
use anyhow::{Result, bail};
use reqwest::{Client, Proxy, redirect::Policy};
use std::time::Duration;

pub fn client(port: u16, timeout_ms: u32, domains: Vec<String>) -> Result<Client> {
    Ok(Client::builder()
        .no_proxy()
        .proxy(Proxy::all(format!("http://127.0.0.1:{port}"))?)
        .pool_max_idle_per_host(0)
        .connect_timeout(Duration::from_millis(u64::from(timeout_ms.min(5_000))))
        .timeout(Duration::from_millis(u64::from(timeout_ms)))
        .redirect(Policy::custom(move |attempt| {
            if attempt.previous().len() >= 5 {
                attempt.error("redirect_limit")
            } else if policy::redirect_allowed(attempt.url(), &domains) {
                attempt.follow()
            } else {
                attempt.error("redirect_outside_managed_domains")
            }
        }))
        .user_agent("Mozilla/5.0 ClashVerge/AutoFailover")
        .build()?)
}

pub async fn check(client: &Client, url: &str) -> (Option<u16>, Result<()>) {
    let mut status_code = None;
    let result = async {
        let mut response = client.get(url).header("Cache-Control", "no-cache").send().await?;
        status_code = Some(response.status().as_u16());
        // Cloudflare explicitly identifies a challenge even when its body has no stable markers.
        if matches!(response.status().as_u16(), 200..=299 | 403)
            && response
                .headers()
                .get("cf-mitigated")
                .is_some_and(|value| value == "challenge")
        {
            bail!("verification_required");
        }
        let final_url = response.url().clone();
        let mut body = Vec::new();
        // Bound memory and traffic, even for a streaming or malicious response.
        while let Some(chunk) = response.chunk().await? {
            let remaining = 512 * 1024 - body.len();
            body.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
            if body.len() >= 512 * 1024 {
                break;
            }
        }
        policy::classify_response(
            &final_url,
            status_code.unwrap_or_default(),
            &String::from_utf8_lossy(&body),
        )
    }
    .await;
    (status_code, result)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use tokio::{
        io::{AsyncReadExt as _, AsyncWriteExt as _},
        net::TcpListener,
    };

    async fn proxy(response: &'static str) -> (u16, tokio::task::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = vec![0; 4096];
            let len = socket.read(&mut request).await.unwrap();
            socket.write_all(response.as_bytes()).await.unwrap();
            String::from_utf8_lossy(&request[..len]).to_string()
        });
        (port, task)
    }

    #[tokio::test]
    async fn challenge_header_is_pending_verification_but_plain_403_is_failure() {
        for (response, expected) in [
            (
                "HTTP/1.1 403 Forbidden\r\ncf-mitigated: challenge\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                "verification_required",
            ),
            (
                "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                "access_denied",
            ),
            (
                "HTTP/1.1 429 Too Many Requests\r\ncf-mitigated: challenge\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                "rate_limited",
            ),
        ] {
            let (port, request) = proxy(response).await;
            let (status, result) = check(&client(port, 1000, vec![]).unwrap(), "http://chatgpt.com/").await;
            assert!(status.is_some());
            assert_eq!(result.unwrap_err().to_string(), expected);
            request.await.unwrap();
        }
    }

    #[tokio::test]
    async fn requests_really_use_the_explicit_proxy() {
        let (port, received) =
            proxy("HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: 19\r\n\r\n<html>GitHub</html>\n").await;
        let client = client(port, 1000, vec!["github.com".into()]).unwrap();
        let (status, result) = check(&client, "http://github.com/").await;
        assert_eq!(status, Some(200));
        assert!(result.is_ok(), "{result:?}");
        assert!(received.await.unwrap().starts_with("GET http://github.com/ HTTP/1.1"));
    }

    #[tokio::test]
    async fn switching_uses_a_new_connection_and_checks_the_new_response() {
        let (first, first_request) =
            proxy("HTTP/1.1 403 Forbidden\r\nConnection: close\r\nContent-Length: 0\r\n\r\n").await;
        let (second, second_request) =
            proxy("HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: 19\r\n\r\n<html>GitHub</html>\n").await;
        assert!(
            check(&client(first, 1000, vec![]).unwrap(), "http://github.com/")
                .await
                .1
                .is_err()
        );
        assert!(
            check(&client(second, 1000, vec![]).unwrap(), "http://github.com/")
                .await
                .1
                .is_ok()
        );
        assert!(first_request.await.unwrap().contains("github.com"));
        assert!(second_request.await.unwrap().contains("github.com"));
    }

    #[tokio::test]
    async fn redirects_cannot_escape_the_managed_route() {
        let (port, received) = proxy("HTTP/1.1 302 Found\r\nLocation: https://unmanaged.invalid/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
        let result = check(
            &client(port, 1000, vec!["github.com".into()]).unwrap(),
            "http://github.com/",
        )
        .await
        .1;
        assert!(result.is_err());
        assert!(received.await.unwrap().contains("github.com"));
    }

    #[tokio::test]
    async fn stalled_proxy_is_bounded_by_the_total_timeout() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let result = tokio::time::timeout(
            Duration::from_secs(2),
            check(&client(port, 50, vec![]).unwrap(), "http://github.com/"),
        )
        .await
        .unwrap();
        assert!(result.1.is_err());
    }
}
